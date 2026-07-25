use pomidoro::{ListenMode, Request, ServerStatus, config};

use clap::{Args, Parser, Subcommand};
use thiserror::Error;
use tokio::io::AsyncBufReadExt;
use tokio::net::{UnixListener, UnixStream};

use std::future::Future;
use std::path::{Path, PathBuf};

#[derive(Error, Debug)]
enum StartupError {
    #[error("Failed to bind abstract socket {0:?}: {1}")]
    AbstractSocketBind(String, #[source] std::io::Error),
    #[error("Failed to bind normal socket {0:?}: {1}")]
    NormalSocketBind(PathBuf, #[source] std::io::Error),
}

#[derive(Parser, Debug)]
#[command(
    name = "pomidoro",
    version,
    about = "A pomodoro timer with client-server architecture and statistic collection."
)]
struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Non-standard path to a configuration file
    #[arg(short = 'C', long = "config")]
    pub config_path: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Start the server
    StartServer {
        /// Move the server to the background (Daemonize)
        #[arg(short, long)]
        daemon: bool,
    },

    /// Start the timer
    #[command(name = "start", visible_alias = "s")]
    StartTimer(SimpleCommandArgs),

    /// Skip to the next interval in the cycle
    #[command(visible_alias = "next", visible_alias = "n")]
    NextInterval(SimpleCommandArgs),

    /// Pause a running timer
    #[command(visible_alias = "p")]
    Pause(SimpleCommandArgs),

    /// Resume a paused timer
    #[command(visible_alias = "r")]
    Resume(SimpleCommandArgs),

    /// Toggle between pause and resume
    #[command(visible_alias = "t")]
    Toggle(SimpleCommandArgs),

    /// Stop the timer and reset the cycle
    Stop(SimpleCommandArgs),

    /// Get the current status of the timer
    Status(StatusCommandArgs),

    /// Listen to status updates or discrete events
    Listen(ListenCommandArgs),

    /// Get information about the current timer configuration
    #[command(visible_alias = "config")]
    ConfigInfo,
}

impl Command {
    #[must_use]
    pub fn into_request_and_format(self) -> Option<(Request, OutputFormat)> {
        let (request, format) = match self {
            Self::StartTimer(args) => (Request::Start, OutputFormat::from_simple(&args)),
            Self::NextInterval(args) => {
                (Request::NextInterval, OutputFormat::from_simple(&args))
            },
            Self::Pause(args) => (Request::Pause, OutputFormat::from_simple(&args)),
            Self::Resume(args) => (Request::Resume, OutputFormat::from_simple(&args)),
            Self::Toggle(args) => (Request::Toggle, OutputFormat::from_simple(&args)),
            Self::Stop(args) => (Request::Stop, OutputFormat::from_simple(&args)),
            Self::Status(args) => (Request::Status, OutputFormat::from_ticks(args)),
            Self::Listen(args) => {
                let (mode, format) = if args.events {
                    (ListenMode::Events, OutputFormat::Json)
                } else {
                    (ListenMode::Tick, OutputFormat::from_ticks(args.status_args))
                };
                (Request::Listen(mode), format)
            },
            Self::ConfigInfo => (Request::GetConfig, OutputFormat::Json),
            Self::StartServer { .. } => return None,
        };
        Some((request, format))
    }
}

enum OutputFormat {
    Json,
    Silent,
    TickData(Vec<ResponseField>),
}

impl OutputFormat {
    const fn from_simple(args: &SimpleCommandArgs) -> Self {
        if args.json { Self::Json } else { Self::Silent }
    }

    fn from_ticks(args: StatusCommandArgs) -> Self {
        if args.json {
            Self::Json
        } else if args.data.is_empty() {
            Self::TickData(Self::default_fields())
        } else {
            Self::TickData(args.data)
        }
    }

    fn default_fields() -> Vec<ResponseField> {
        vec![
            ResponseField::IntervalType,
            ResponseField::State,
            ResponseField::IsOvertime,
            ResponseField::Overtime,
            ResponseField::TimeLeft,
            ResponseField::TimeElapsed,
            ResponseField::TotalIntervalDuration,
        ]
    }

    fn print_line(&self, line: &str) {
        let unexpected = || eprintln!("Unexpected server response: {line}");

        match self {
            Self::Json => print!("{line}"),
            Self::Silent => {
                match serde_json::from_str::<pomidoro::ConfirmationResponse>(line) {
                    Ok(resp) if !resp.success => eprintln!("Error: {}", resp.error_msg),
                    Ok(_) => {},
                    Err(_) => unexpected(),
                }
            },
            Self::TickData(fields) => match serde_json::from_str::<ServerStatus>(line) {
                Ok(resp) => Self::print_fields(fields, &resp),
                Err(_) => unexpected(),
            },
        }
    }

    fn print_fields(fields: &[ResponseField], resp: &ServerStatus) {
        let parts: Vec<String> = fields.iter().map(|f| f.format_value(resp)).collect();
        println!("{}", parts.join("\t"));
    }
}

#[derive(Debug, Args)]
struct SimpleCommandArgs {
    /// Print the underlying JSON message
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Debug, Args)]
struct StatusCommandArgs {
    /// Comma seperated fields of response JSON to print.
    /// The values are printed seperated by tabs.
    #[arg(short, long, value_delimiter = ',')]
    pub data: Vec<ResponseField>,

    /// Print the underlying JSON message
    #[arg(short, long, conflicts_with = "data")]
    pub json: bool,
}

#[derive(Debug, Args)]
struct ListenCommandArgs {
    /// Listen for discrete events instead of tick updates
    #[arg(short, long, conflicts_with = "tick")]
    pub events: bool,

    /// Listen for tick updates every second and when server state updates (default)
    #[arg(short, long, conflicts_with = "events")]
    pub tick: bool,

    #[command(flatten)]
    pub status_args: StatusCommandArgs,
}

#[derive(Debug, Clone, clap::ValueEnum)]
#[value(rename_all = "kebab-case")]
enum ResponseField {
    #[value(alias = "i", alias = "int", alias = "interval")]
    IntervalType,
    #[value(alias = "s")]
    State,
    #[value(alias = "io", alias = "is-over")]
    IsOvertime,
    #[value(alias = "o")]
    Overtime,
    #[value(alias = "tl")]
    TimeLeft,
    #[value(alias = "te")]
    TimeElapsed,
    #[value(alias = "tid", alias = "tot-int-dur")]
    TotalIntervalDuration,
}

impl ResponseField {
    fn format_value(&self, resp: &pomidoro::ServerStatus) -> String {
        match self {
            Self::IntervalType => resp.interval_type.clone(),
            Self::State => format!("{:?}", resp.state).to_lowercase(),
            Self::IsOvertime => resp.is_overtime.to_string(),
            Self::Overtime => resp.overtime.to_string(),
            Self::TimeLeft => resp.time_left.to_string(),
            Self::TimeElapsed => resp.time_elapsed.to_string(),
            Self::TotalIntervalDuration => resp.total_interval_duration.to_string(),
        }
    }
}

fn enter_tokio_runtime<F: Future>(future: F) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("unable to create tokio runtime");

    rt.block_on(future);
}

async fn start_server(config: pomidoro::Config) {
    async fn validate_socket_path(path: &Path) {
        if path.exists() {
            if UnixStream::connect(path).await.is_ok() {
                log::error!(
                    "Server is already running on this socket: {}",
                    path.display()
                );
                std::process::exit(1);
            }
            if let Err(e) = tokio::fs::remove_file(path).await {
                log::error!("Failed to remove stale socket file {}: {e}", path.display());
                std::process::exit(1);
            }
        } else if let Some(parent) = path.parent()
            && let Err(e) = tokio::fs::create_dir_all(parent).await
        {
            log::error!(
                "Failed to create parent directories for socket file {:?}: {e}",
                path.display()
            );
            std::process::exit(1);
        }
    }

    let listener = match &config.socket {
        config::Socket::Normal(path) => {
            validate_socket_path(path).await;
            UnixListener::bind(path)
                .map_err(|e| StartupError::NormalSocketBind(path.clone(), e))
        },
        config::Socket::Abstract(addr) => UnixListener::bind(addr)
            .map_err(|e| StartupError::AbstractSocketBind(addr.clone(), e)),
    }
    .unwrap_or_else(|e| {
        log::error!("{e}");
        std::process::exit(1);
    });

    pomidoro::run_server(listener, config).await;
}

async fn send_request(config: pomidoro::Config, request: Request, format: OutputFormat) {
    let client = pomidoro::Client::new(config);

    let mut reader = match client.send_request(request).await {
        Ok(r) => r,
        Err(e) => {
            log::error!("{e}");
            std::process::exit(1);
        },
    };

    let mut line = String::new();
    while let Ok(n) = reader.read_line(&mut line).await {
        if n == 0 {
            break;
        }
        format.print_line(&line);
        line.clear();
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    let cli = Cli::parse();

    let config = pomidoro::Config::load(cli.config_path).unwrap_or_else(|e| {
        log::error!("{e}");
        std::process::exit(1);
    });

    match cli.command {
        Command::StartServer { daemon } => {
            if daemon {
                log::warn!("Daemonizing the server is not implemented"); // TODO
            }
            enter_tokio_runtime(start_server(config));
        },
        cmd => {
            let (request, format) = cmd.into_request_and_format().unwrap();
            enter_tokio_runtime(send_request(config, request, format));
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }
}
