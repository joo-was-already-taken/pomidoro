use pomidoro::{Client, Config, Request, StatusResponse, TimerState};

use ksni::menu::StandardItem;
use ksni::{MenuItem, Tray, TrayMethods};
use tokio::sync::mpsc;

use std::time::Duration;

const PKG_NAME: &str = env!("CARGO_PKG_NAME");

const TOMATO_ART: [&str; 16] = [
    "     #  #  #    ",
    "    # ## #  ##  ",
    "   ##      #    ",
    "  # # ####  ##  ",
    "  #  #  #    #  ",
    "  # ##  #  ## # ",
    "  ###  ##  ###  ",
    " ###  #### #### ",
    " ############## ",
    " ############## ",
    " ############## ",
    " ############## ",
    "  ############  ",
    "  ############  ",
    "   ##########   ",
    "     ######     ",
];

#[derive(Debug)]
struct PomidoroTray {
    status: Option<StatusResponse>,
    command_tx: mpsc::UnboundedSender<Request>,
}

impl PomidoroTray {
    fn is_running(&self) -> bool {
        self.status
            .as_ref()
            .is_some_and(|s| s.state == TimerState::Running)
    }

    fn is_stopped(&self) -> bool {
        self.status
            .as_ref()
            .is_none_or(|s| s.state == TimerState::Stopped)
    }

    fn format_time(&self) -> Option<String> {
        let seconds_fmt = |seconds| {
            let minutes = seconds / 60;
            let seconds_part = seconds % 60;
            format!("{minutes:02}:{seconds_part:02}")
        };
        self.status.as_ref().map(|status| {
            if status.is_overtime {
                format!("{} (overtime)", seconds_fmt(status.overtime))
            } else {
                seconds_fmt(status.time_left)
            }
        })
    }

    fn render_tomato_icon(&self) -> Vec<ksni::Icon> {
        let width = 16;
        let height = 16;
        let mut data = Vec::with_capacity(width * height * 4);

        let (r, g, b) = if self.is_running() {
            (255, 255, 255)
        } else {
            (128, 128, 128)
        };

        for line in &TOMATO_ART {
            for ch in line.chars() {
                if ch == '#' {
                    data.extend([255, r, g, b]);
                } else {
                    data.extend([0, 0, 0, 0]);
                }
            }
        }

        vec![ksni::Icon {
            width: i32::try_from(width).unwrap(),
            height: i32::try_from(height).unwrap(),
            data,
        }]
    }
}

impl Tray for PomidoroTray {
    fn id(&self) -> String {
        PKG_NAME.into()
    }

    fn icon_name(&self) -> String {
        String::new()
    }

    fn title(&self) -> String {
        String::new()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let title = self.format_time().unwrap_or_else(|| PKG_NAME.into());
        ksni::ToolTip {
            title,
            description: String::new(),
            ..Default::default()
        }
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.render_tomato_icon()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items = Vec::new();

        let state = self.status.as_ref().map_or_else(
            || "Connecting...".into(),
            |status| {
                format!("{} - {}", status.interval_type, self.format_time().unwrap())
            },
        );

        items.push(
            StandardItem {
                label: state,
                enabled: false,
                ..Default::default()
            }
            .into(),
        );

        if self.is_running() {
            items.push(
                StandardItem {
                    label: "Pause".into(),
                    icon_name: "media-playback-pause".into(),
                    activate: Box::new(|this: &mut Self| {
                        let _ = this.command_tx.send(Request::Toggle);
                    }),
                    ..Default::default()
                }
                .into(),
            );
            // Add a dummy invisible item to force length change -> layout update on state toggle
            items.push(
                StandardItem {
                    visible: false,
                    ..Default::default()
                }
                .into(),
            );
        } else {
            items.push(
                StandardItem {
                    label: "Start / Resume".into(),
                    icon_name: "media-playback-start".into(),
                    activate: Box::new(|this: &mut Self| {
                        let req = if this.is_stopped() {
                            Request::Start
                        } else {
                            Request::Toggle
                        };
                        let _ = this.command_tx.send(req);
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }

        items.push(
            StandardItem {
                label: "Next".into(),
                icon_name: "media-skip-forward".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.command_tx.send(Request::NextInterval);
                }),
                ..Default::default()
            }
            .into(),
        );

        items.push(
            StandardItem {
                label: "Stop".into(),
                icon_name: "media-playback-stop".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.command_tx.send(Request::Stop);
                }),
                ..Default::default()
            }
            .into(),
        );

        items.push(
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|_| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        );

        // Force continuous layout updates while running to bypass DE caching bugs
        if let Some(status) = &self.status
            && status.time_elapsed % 2 == 0
        {
            items.push(
                StandardItem {
                    visible: false,
                    ..Default::default()
                }
                .into(),
            );
        }

        items
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    let config = Config::load(None).unwrap_or_else(|e| {
        log::error!("Configuration error: {e}");
        std::process::exit(1);
    });

    let client = Client::new(config);
    let (command_tx, mut command_rx) = mpsc::unbounded_channel();

    let tray = PomidoroTray {
        status: None,
        command_tx,
    };

    let handle = tray.spawn().await.unwrap();

    let cmd_client = client.clone();
    tokio::spawn(async move {
        while let Some(request) = command_rx.recv().await {
            let _ = cmd_client.send_and_confirm(request).await;
        }
    });

    loop {
        if let Ok(mut reader) = client.send_request(Request::Listen).await {
            use tokio::io::AsyncBufReadExt;
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 {
                    break;
                }
                if let Ok(status) = serde_json::from_str::<StatusResponse>(&line) {
                    handle
                        .update(|tray: &mut PomidoroTray| {
                            tray.status = Some(status);
                        })
                        .await;
                }
                line.clear();
            }
        }

        handle
            .update(|tray: &mut PomidoroTray| {
                tray.status = None;
            })
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
