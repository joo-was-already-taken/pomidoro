use pomidoro::{Request, config};

use clap::{Parser, Subcommand};
use thiserror::Error;
use tokio::io::AsyncBufReadExt;
use tokio::net::{UnixListener, UnixStream};

use std::future::Future;
use std::path::PathBuf;

#[derive(Error, Debug)]
enum StartupError {
    #[error("Could not read explicitly provided config file {0:?}: {1}")]
    ExplicitConfigRead(PathBuf, #[source] std::io::Error),
    #[error("Configuration error in {0:?}: {1}")]
    Config(PathBuf, #[source] config::Error),
    #[error("Failed to bind socket {0:?}: {1}")]
    #[allow(unused)] // TODO: remove
    SocketBind(PathBuf, #[source] std::io::Error),
    #[error("Failed to bind abstract socket {0:?}: {1}")]
    AbstractSocketBind(String, #[source] std::io::Error),
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
pub enum Command {
    /// Start the server
    StartServer {
        /// Move the server to the background (Daemonize)
        #[arg(short, long)]
        daemon: bool,
    },

    /// Start the timer
    #[command(name = "start", visible_alias = "s")]
    StartTimer,

    /// Skip to the next interval in the cycle
    #[command(visible_alias = "next", visible_alias = "n")]
    NextInterval,

    /// Pause a running timer
    #[command(visible_alias = "p")]
    Pause,

    /// Resume a paused timer
    #[command(visible_alias = "r")]
    Resume,

    /// Toggle between pause and resume
    #[command(visible_alias = "t")]
    Toggle,

    /// Stop the timer and reset the cycle
    Stop,

    /// Get the current status of the timer
    Status,

    /// Listen to status updates
    Listen,
}

impl Command {
    #[must_use]
    pub const fn as_request(&self) -> Option<Request> {
        let request = match self {
            Self::StartTimer => Request::Start,
            Self::NextInterval => Request::NextInterval,
            Self::Pause => Request::Pause,
            Self::Resume => Request::Resume,
            Self::Toggle => Request::Toggle,
            Self::Stop => Request::Stop,
            Self::Status => Request::Status,
            Self::Listen => Request::Listen,
            Self::StartServer { .. } => return None,
        };
        Some(request)
    }
}

fn read_config_file(
    cli_config: Option<PathBuf>,
) -> Result<Option<(PathBuf, String)>, StartupError> {
    if let Some(path) = cli_config {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| StartupError::ExplicitConfigRead(path.clone(), e))?;
        return Ok(Some((path, content)));
    }

    let config_home = dirs::config_dir();

    let config_file = "pomidoro/config.toml";

    if let Some(mut path) = config_home {
        path.push(config_file);
        if let Ok(content) = std::fs::read_to_string(&path) {
            return Ok(Some((path, content)));
        }
        log::debug!(
            "Could not read '{}', falling back to global config in '/etc'",
            path.display(),
        );
    } else {
        log::warn!(
            "Could not determine user configuration directory (neither XDG_CONFIG_HOME nor HOME are set)"
        );
    }

    let global_path = PathBuf::from("/etc").join(config_file);
    if let Ok(content) = std::fs::read_to_string(&global_path) {
        return Ok(Some((global_path, content)));
    }

    Ok(None)
}

fn enter_tokio_runtime<F: Future>(future: F) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("unable to create tokio runtime");

    rt.block_on(future);
}

async fn start_server(config: pomidoro::Config) {
    let listener = match &config.socket {
        config::Socket::Normal(_path) => {
            log::error!("Only abstract sockets are currently supported");
            std::process::exit(1);
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

async fn send_request(config: pomidoro::Config, request: Request) {
    let sock_addr = config.socket.as_str();
    let stream = match UnixStream::connect(sock_addr).await {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to connect to server at {sock_addr:?}: {e}");
            std::process::exit(1);
        },
    };

    let (reader, mut writer) = tokio::io::split(stream);

    if let Err(e) = pomidoro::send_json(&mut writer, &request).await {
        log::error!("Failed to send JSON request: {e}");
        std::process::exit(1);
    }

    let mut buf_reader = tokio::io::BufReader::new(reader);
    let mut line = String::new();

    while let Ok(n) = buf_reader.read_line(&mut line).await {
        if n == 0 {
            break;
        }
        print!("{line}");
        line.clear();
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    let cli = Cli::parse();

    let config = match read_config_file(cli.config_path) {
        Ok(Some((path, content))) => pomidoro::Config::parse(&content)
            .inspect(|_| {
                log::info!("Successfully loaded configuration from {}", path.display());
            })
            .map_err(|e| StartupError::Config(path, e)),
        Ok(None) => {
            log::warn!("No configuration file found, using defaults");
            Ok(pomidoro::Config::parse("").expect("Default config is always valid"))
        },
        Err(e) => Err(e),
    }
    .unwrap_or_else(|e| {
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
            let request = cmd.as_request().unwrap();
            enter_tokio_runtime(send_request(config, request));
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
