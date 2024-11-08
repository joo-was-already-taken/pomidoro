mod cli;
mod config;
mod socket;
mod pomodoro_clock;

use cli::{Cli, Command, Request};
use config::{Config, TomlConfig};
use pomodoro_clock::{PomodoroClock, Response};

use clap::Parser;
use rand::Rng;
use serde::Serialize;

use std::fs;
use std::path::{Path, PathBuf};


#[derive(Debug, Serialize)]
struct TemplateSource {
    /// Server id
    id: u32,
    /// "running" | "paused"
    clock_state: String,
    /// Session name
    session: String,
    /// Whole session duration
    duration: String,
    /// `0..=100`
    percent: u32,
    /// Time left
    time: String,
}


fn get_config(config_path: Option<&Path>) -> Config {
    let config_path = match config_path {
        Some(path) => Some(PathBuf::from(path)),
        None => {
            let config_dir: PathBuf = std::env::var("XDG_CONFIG_HOME")
                .unwrap_or_else(|_| {
                    let home = std::env::var("HOME")
                        .expect("Could not find the 'HOME' variable");
                    format!("{home}/.config/")
                })
                .into();
            let path = config_dir.join("pomidoro/config.toml");
            path.exists().then_some(path)
        },
    };
    let config_file = config_path.map(|config_path| {
        fs::read_to_string(&config_path)
            .expect(format!(
                "Could not open the config file '{}'",
                config_path.display(),
            ).as_str())
    });
    config_file
        .map(|config_file| {
            toml::from_str::<TomlConfig>(&config_file)
                .expect("Error in config file")
        })
        .unwrap_or_default()
        .into()
}


fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    let config = get_config(cli.config_path.as_deref());

    match cli.command {
        Command::Start { server_id } => {
            let server_path = config.server_path(server_id);
            if server_path.exists() {
                fs::remove_file(&server_path)?;
            }

            let sessions = config.sessions.iter();
            let pomodoro_clock = PomodoroClock::paused(sessions, &config.time_format);
            socket::start_server(&server_path, pomodoro_clock)?; 

            fs::remove_file(&server_path)?;
        },
        Command::Send { request, server_id } => {
            let random_digits = |len: usize| -> String {
                let mut rng = rand::thread_rng();
                (0..len)
                    .map(|_| rng.gen_range('0'..='9'))
                    .collect()
            };

            // generate an unexisting client socket filename
            let client_path = loop {
                let file_name = format!("client{}.sock", random_digits(6));
                let path = config.socket_dir.join(file_name);
                if !path.exists() {
                    break path;
                }
            };

            let response: Response = socket::send_and_receive(
                &client_path,
                &config.server_path(server_id),
                &pomodoro_clock::Request::from(&request),
            )?;
            match request {
                Request::Fetch { template } => {
                    let Response::State(state) = response else { unreachable!(); };

                    let template_src = TemplateSource {
                        id: server_id,
                        clock_state: if state.is_paused {
                            config.paused_state_text
                        } else {
                            config.running_state_text
                        },
                        session: state.session_name,
                        duration: state.session_duration,
                        time: state.time,
                        percent: state.percent,
                    };
                    let output = template.render_to_string(&template_src)
                        .expect("Couldn't populate mustache template");
                    println!("{}", output);
                },
                _ => (),
            }

            fs::remove_file(&client_path)?;
        },
    }

    Ok(())
}

