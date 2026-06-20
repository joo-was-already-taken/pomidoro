use rustix::process::getuid;
use serde::Deserialize;
use shellexpand::LookupError;
use thiserror::Error;

use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to parse TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Configuration validation failed: {0}")]
    Validation(String),
    #[error("Could not read explicitly provided config file {0:?}: {1}")]
    ExplicitConfigRead(PathBuf, #[source] std::io::Error),
    #[error("Configuration error in {0:?}: {1}")]
    File(PathBuf, Box<Error>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_cycle")]
    pub cycle: Vec<String>,
    #[serde(default)]
    pub intervals: BTreeMap<String, IntervalConfig>,
    #[serde(default)]
    pub socket: Socket,
}

fn default_cycle() -> Vec<String> {
    [
        "work",
        "short break",
        "work",
        "short break",
        "work",
        "short break",
        "work",
        "long break",
    ]
    .map(Into::into)
    .into()
}

impl Config {
    pub fn parse(content: &str) -> Result<Self, Error> {
        let mut config: Self = toml::from_str(content)?;
        config.normalize();
        config.validate().map_err(Error::Validation)?;
        Ok(config)
    }

    pub fn load(cli_config: Option<PathBuf>) -> Result<Self, Error> {
        if let Some(path) = cli_config {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| Error::ExplicitConfigRead(path.clone(), e))?;
            log::info!("Successfully loaded configuration from {}", path.display());
            return Self::parse(&content).map_err(|e| Error::File(path, Box::new(e)));
        }

        let config_home = dirs::config_dir();
        let config_file = "pomidoro/config.toml";

        if let Some(mut path) = config_home {
            path.push(config_file);
            if let Ok(content) = std::fs::read_to_string(&path) {
                log::info!("Successfully loaded configuration from {}", path.display());
                return Self::parse(&content).map_err(|e| Error::File(path, Box::new(e)));
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
            log::info!(
                "Successfully loaded configuration from {}",
                global_path.display()
            );
            return Self::parse(&content)
                .map_err(|e| Error::File(global_path, Box::new(e)));
        }

        log::warn!("No configuration file found, using defaults");
        Ok(Self::parse("").expect("Default config is always valid"))
    }

    fn normalize(&mut self) {
        let default_intervals = [
            (
                "work",
                IntervalConfig {
                    duration: Duration::from_mins(25),
                },
            ),
            (
                "short break",
                IntervalConfig {
                    duration: Duration::from_mins(5),
                },
            ),
            (
                "long break",
                IntervalConfig {
                    duration: Duration::from_mins(15),
                },
            ),
        ];
        for (name, cfg) in default_intervals {
            self.intervals.entry(name.into()).or_insert(cfg);
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.cycle.is_empty() {
            return Err("Interval cycle cannot be empty".to_string());
        }
        for name in &self.cycle {
            if !self.intervals.contains_key(name) {
                return Err(format!(
                    "Interval '{name}' used in cycle is not defined in 'intervals' map"
                ));
            }
        }

        if let Socket::Abstract(addr) = &self.socket {
            assert_eq!(addr.as_bytes()[0], b'\0');
            if addr.len() == 1 {
                return Err("Abstract socket address cannot be empty".to_string());
            }
            if addr.as_bytes()[1..].contains(&b'\0') {
                return Err("Abstract socket address cannot contain null bytes (null byte is automatically prepended)".to_string());
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct IntervalConfig {
    #[serde(with = "humantime_serde")]
    pub duration: Duration,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(try_from = "SocketHelper")]
pub enum Socket {
    Normal(PathBuf),
    Abstract(String),
}

impl Socket {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Normal(path) => path
                .to_str()
                .expect("Socket path in config should be valid UTF-8"),
            Self::Abstract(addr) => addr.as_str(),
        }
    }
}

impl Default for Socket {
    fn default() -> Self {
        SocketHelper::default().try_into().unwrap()
    }
}

impl TryFrom<SocketHelper> for Socket {
    type Error = LookupError<env::VarError>;

    fn try_from(helper: SocketHelper) -> Result<Self, Self::Error> {
        let socket = match helper {
            SocketHelper::Simple(addr) => {
                Self::Normal(PathBuf::from(expand_string(&addr)?))
            },
            SocketHelper::Full { addr, is_abstract } => {
                let expanded = expand_string(&addr)?;
                if is_abstract {
                    Self::Abstract(format!("\0{expanded}"))
                } else {
                    Self::Normal(PathBuf::from(expanded))
                }
            },
        };
        Ok(socket)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum SocketHelper {
    Simple(String),
    Full {
        addr: String,
        #[serde(default = "default_true", rename = "abstract")]
        is_abstract: bool,
    },
}

const fn default_true() -> bool {
    true
}

impl Default for SocketHelper {
    fn default() -> Self {
        Self::Full {
            addr: "pomidoro-server-${uid}".into(),
            is_abstract: true,
        }
    }
}

fn expand_string(input: &str) -> Result<String, LookupError<env::VarError>> {
    let home_dir = || dirs::home_dir().and_then(|p| p.to_str().map(String::from));
    let vars = |var: &str| {
        if var == "uid" {
            let uid = getuid().as_raw();
            return Ok(Some(uid.to_string()));
        }
        env::var(var).map(Some)
    };
    shellexpand::full_with_context(input, home_dir, vars).map(|cow| cow.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = Config::parse("").unwrap();
        assert_eq!(config.cycle.len(), 8);
        assert_eq!(config.intervals.len(), 3);
        assert_eq!(config.intervals["work"].duration, Duration::from_mins(25));
    }

    #[test]
    fn selective_overwrite() {
        let toml_str = r#"
            [intervals.work]
            duration = "50m"
        "#;
        let config = Config::parse(toml_str).unwrap();

        assert_eq!(config.intervals["work"].duration, Duration::from_mins(50));
        assert_eq!(
            config.intervals["short break"].duration,
            Duration::from_mins(5),
        );
    }

    #[test]
    fn invalid_cycle_name() {
        let toml_str = r#"
            cycle = ["nonexistent"]
        "#;
        let res = Config::parse(toml_str);
        assert!(matches!(res, Err(Error::Validation(_))));
    }

    #[test]
    fn empty_cycle() {
        let toml_str = "
            cycle = []
        ";
        let res = Config::parse(toml_str);
        assert!(matches!(res, Err(Error::Validation(_))));
    }

    #[test]
    fn custom_intervals_in_cycle() {
        let toml_str = r#"
            cycle = ["custom"]
            [intervals.custom]
            duration = "1s"
        "#;
        let config = Config::parse(toml_str).unwrap();
        assert_eq!(config.intervals["custom"].duration, Duration::from_secs(1));
    }

    #[test]
    fn socket_simple_string() {
        let toml_str = r#"socket = "/tmp/pomidoro.sock""#;
        let config = Config::parse(toml_str).unwrap();
        assert_eq!(
            config.socket,
            Socket::Normal(PathBuf::from("/tmp/pomidoro.sock"))
        );
    }

    #[test]
    fn socket_full_abstract_default() {
        let toml_str = r#"
            [socket]
            addr = "my-abstract-socket"
        "#;
        let config = Config::parse(toml_str).unwrap();
        assert_eq!(
            config.socket,
            Socket::Abstract("\0my-abstract-socket".into()),
        );
    }

    #[test]
    fn socket_full_abstract_false() {
        let toml_str = r#"
            [socket]
            addr = "/tmp/pomidoro.sock"
            abstract = false
        "#;
        let config = Config::parse(toml_str).unwrap();
        assert_eq!(
            config.socket,
            Socket::Normal(PathBuf::from("/tmp/pomidoro.sock"))
        );
    }

    #[test]
    fn socket_full_abstract_true() {
        let toml_str = r#"
            [socket]
            addr = "explicit-abstract"
            abstract = true
        "#;
        let config = Config::parse(toml_str).unwrap();
        assert_eq!(
            config.socket,
            Socket::Abstract("\0explicit-abstract".into()),
        );
    }

    #[test]
    fn socket_expansion_uid() {
        let toml_str = r#"
            [socket]
            addr = "/run/user/${uid}/pomidoro.sock"
            abstract = false
        "#;
        let config = Config::parse(toml_str).unwrap();
        let expected_path = format!(
            "/run/user/{}/pomidoro.sock",
            rustix::process::getuid().as_raw()
        );
        assert_eq!(config.socket, Socket::Normal(PathBuf::from(expected_path)));
    }

    #[test]
    fn socket_expansion_home() {
        let toml_str = r#"socket = "~/.pomidoro.sock""#;
        let config = Config::parse(toml_str).unwrap();
        let expected_path = format!(
            "{}/.pomidoro.sock",
            dirs::home_dir().unwrap().to_str().unwrap()
        );
        assert_eq!(config.socket, Socket::Normal(PathBuf::from(expected_path)));
    }

    #[test]
    fn socket_validation_empty_abstract() {
        let toml_str = r#"
            [socket]
            addr = ""
            abstract = true
        "#;
        let res = Config::parse(toml_str);
        assert!(
            matches!(res, Err(Error::Validation(msg)) if msg.contains("cannot be empty"))
        );
    }

    #[test]
    fn socket_validation_null_byte_in_abstract() {
        let toml_str = r#"
            [socket]
            addr = "my\u0000sock"
            abstract = true
        "#;
        let res = Config::parse(toml_str);
        assert!(
            matches!(res, Err(Error::Validation(msg)) if msg.contains("cannot contain null bytes"))
        );
    }
}
