use pomidoro::{Request, config};

use clap::{Args, Parser, Subcommand};
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

    /// Listen to status updates
    Listen(StatusCommandArgs),
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
            Self::Status(args) => (Request::Status, OutputFormat::from_status(args)),
            Self::Listen(args) => (Request::Listen, OutputFormat::from_status(args)),
            Self::StartServer { .. } => return None,
        };
        Some((request, format))
    }
}

enum OutputFormat {
    Json,
    Silent,
    Data(Vec<ResponseField>),
}

impl OutputFormat {
    const fn from_simple(args: &SimpleCommandArgs) -> Self {
        if args.json { Self::Json } else { Self::Silent }
    }

    fn from_status(args: StatusCommandArgs) -> Self {
        if args.json {
            Self::Json
        } else if args.data.is_empty() {
            Self::Data(vec![
                ResponseField::IntervalType,
                ResponseField::State,
                ResponseField::IsOvertime,
                ResponseField::Overtime,
                ResponseField::TimeLeft,
                ResponseField::TimeElapsed,
                ResponseField::TotalIntervalDuration,
            ])
        } else {
            Self::Data(args.data)
        }
    }

    fn print_line(&self, line: &str) {
        match self {
            Self::Json => print!("{line}"),
            Self::Silent => {
                if let Ok(resp) =
                    serde_json::from_str::<pomidoro::ConfirmationResponse>(line)
                    && !resp.success
                {
                    eprintln!("Error: {}", resp.error_msg);
                }
            },
            Self::Data(fields) => {
                if let Ok(resp) = serde_json::from_str::<pomidoro::StatusResponse>(line) {
                    let parts: Vec<String> =
                        fields.iter().map(|f| f.format_value(&resp)).collect();
                    println!("{}", parts.join(" "));
                }
            },
        }
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
    /// Comma seperated fields which values to print in the specified order
    #[arg(short, long, value_delimiter = ',')]
    pub data: Vec<ResponseField>,

    /// Print the underlying JSON message
    #[arg(short, long, conflicts_with = "data")]
    pub json: bool,
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
    fn format_value(&self, resp: &pomidoro::StatusResponse) -> String {
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

async fn send_request(config: pomidoro::Config, request: Request, format: OutputFormat) {
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

        format.print_line(&line);

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
