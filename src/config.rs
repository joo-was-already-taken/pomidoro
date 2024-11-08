use serde::Deserialize;

use std::time::Duration;
use std::path::PathBuf;


#[derive(Debug)]
pub struct Config {
    pub paused_state_text: String,
    pub running_state_text: String,
    pub time_format: String,
    pub socket_dir: PathBuf,
    pub sessions: Vec<Session>,
}

impl Config {
    pub fn server_path(&self, server_id: u32) -> PathBuf {
        self.socket_dir.join(format!("server{server_id}.sock"))
    }
}

impl From<TomlConfig> for Config {
    fn from(toml_config: TomlConfig) -> Self {
        let TomlConfig {
            paused_state_text,
            running_state_text,
            time_format,
            socket_dir,
            sessions,
        } = toml_config;
        Self {
            paused_state_text: paused_state_text.unwrap_or("paused".into()),
            running_state_text: running_state_text.unwrap_or("running".into()),
            time_format: time_format.unwrap_or("%M:%S".into()),
            socket_dir: socket_dir.unwrap_or_else(|| {
                std::env::temp_dir().join("pomidoro")
            }),
            sessions,
        }
    }
}


#[derive(Debug, Deserialize)]
pub struct TomlConfig {
    pub paused_state_text: Option<String>,
    pub running_state_text: Option<String>,
    pub time_format: Option<String>,
    pub socket_dir: Option<PathBuf>,
    pub sessions: Vec<Session>,
}

impl Default for TomlConfig {
    fn default() -> Self {
        Self {
            paused_state_text: None,
            running_state_text: None,
            time_format: None,
            socket_dir: None,
            sessions: vec![
                Session {
                    name: "work".into(),
                    duration: Duration::from_secs(60 * 25),
                    time_format: None,
                },
                Session {
                    name: "rest".into(),
                    duration: Duration::from_secs(60 * 5),
                    time_format: None,
                },
            ]
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Session {
    pub name: String,
    pub duration: Duration,
    pub time_format: Option<String>,
}
