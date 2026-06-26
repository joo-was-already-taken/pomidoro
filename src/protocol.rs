use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum ListenMode {
    Tick,
    Events,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum Request {
    Start,
    NextInterval,
    Pause,
    Resume,
    Toggle,
    Stop,
    Status,
    Listen(ListenMode),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ConfirmationResponse {
    pub request: Request,
    pub success: bool,
    pub error_msg: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerStatus {
    pub interval_type: String,
    pub state: TimerState,
    pub is_overtime: bool,
    pub overtime: u64,
    pub time_left: u64,
    pub time_elapsed: u64,
    pub total_interval_duration: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum TimerState {
    Stopped,
    Paused,
    Running,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct IntervalInfo {
    pub name: String,
    pub productive: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerEvent {
    Start {
        interval: IntervalInfo,
        duration: u64,
    },
    Pause {
        interval: IntervalInfo,
        elapsed: u64,
    },
    Resume {
        interval: IntervalInfo,
        elapsed: u64,
    },
    Stop {
        interval: IntervalInfo,
        elapsed: u64,
    },
    Next {
        finished_interval: IntervalInfo,
        started_interval: IntervalInfo,
        duration: u64,
    },
    IntervalCompleted {
        interval: IntervalInfo,
        duration: u64,
    },
}

pub async fn send_json<T: Serialize + Sync, W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    data: &T,
) -> std::io::Result<()> {
    let mut json_str = serde_json::to_string(data)?;
    json_str.push('\n');
    writer.write_all(json_str.as_bytes()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_timer_state() {
        let state = TimerState::Running;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"Running\"");

        let deserialized: TimerState = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, TimerState::Running));
    }

    #[test]
    fn serialize_request() {
        let req = Request::NextInterval;
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, "\"NextInterval\"");

        let deserialized: Request = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, Request::NextInterval));
    }

    #[test]
    fn serialize_status_response() {
        let status = ServerStatus {
            interval_type: "focus".to_string(),
            state: TimerState::Paused,
            is_overtime: false,
            overtime: 0,
            time_left: 300,
            time_elapsed: 1200,
            total_interval_duration: 1500,
        };

        let json = serde_json::to_string(&status).unwrap();

        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["interval_type"], "focus");
        assert_eq!(value["state"], "Paused");
        assert_eq!(value["is_overtime"], false);
        assert_eq!(value["time_left"], 300);
        assert_eq!(value["time_elapsed"], 1200);
    }

    #[test]
    fn status_response_roundtrip() {
        let original = ServerStatus {
            interval_type: "short break".to_string(),
            state: TimerState::Running,
            is_overtime: true,
            overtime: 15,
            time_left: 0,
            time_elapsed: 315,
            total_interval_duration: 300,
        };

        let json = serde_json::to_string(&original).unwrap();

        let expected_json = r#"{"interval_type":"short break","state":"Running","is_overtime":true,"overtime":15,"time_left":0,"time_elapsed":315,"total_interval_duration":300}"#;
        assert_eq!(json, expected_json);

        let deserialized: ServerStatus = serde_json::from_str(&json).unwrap();

        assert_eq!(original.interval_type, deserialized.interval_type);
        assert!(matches!(deserialized.state, TimerState::Running));
        assert_eq!(original.is_overtime, deserialized.is_overtime);
        assert_eq!(original.overtime, deserialized.overtime);
        assert_eq!(original.time_left, deserialized.time_left);
        assert_eq!(original.time_elapsed, deserialized.time_elapsed);
        assert_eq!(
            original.total_interval_duration,
            deserialized.total_interval_duration
        );
    }

    #[test]
    fn serialize_confirmation_response() {
        let conf_ok = ConfirmationResponse {
            request: Request::Start,
            success: true,
            error_msg: String::new(),
        };
        let json_ok = serde_json::to_string(&conf_ok).unwrap();
        let val_ok: serde_json::Value = serde_json::from_str(&json_ok).unwrap();
        assert_eq!(val_ok["request"], "Start");
        assert_eq!(val_ok["success"], true);
        assert_eq!(val_ok["error_msg"], "");

        let conf_err = ConfirmationResponse {
            request: Request::Pause,
            success: false,
            error_msg: "Timer is not running".to_string(),
        };
        let json_err = serde_json::to_string(&conf_err).unwrap();
        let val_err: serde_json::Value = serde_json::from_str(&json_err).unwrap();
        assert_eq!(val_err["request"], "Pause");
        assert_eq!(val_err["success"], false);
        assert_eq!(val_err["error_msg"], "Timer is not running");
    }
}
