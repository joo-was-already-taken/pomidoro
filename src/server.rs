use super::config::Config;
use super::protocol::{ConfirmationResponse, Request, StatusResponse, send_json};
use super::timer_state::TimerState;

use rustix::process::getuid;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::MissedTickBehavior;

use std::time::{Duration, SystemTime};

struct Command {
    pub reply: oneshot::Sender<ConfirmationResponse>,
    pub request: Request,
}

struct ServerState {
    pub timer: TimerState,
    cur_interval_idx: usize,
    config: Config,
}

impl ServerState {
    pub const fn new(config: Config) -> Self {
        Self {
            timer: TimerState::Stopped,
            cur_interval_idx: 0,
            config,
        }
    }

    pub fn to_status_response(&self, now: SystemTime) -> StatusResponse {
        let interval_type = &self.config.cycle[self.cur_interval_idx];
        let interval = &self.config.intervals[interval_type];
        let overtime = self.timer.overtime(now);
        let time_left = self.timer.time_left(now);

        let round_to_secs = |d: Duration| {
            if d.subsec_nanos() >= 500_000_000 {
                d.as_secs() + 1
            } else {
                d.as_secs()
            }
        };

        let total_interval_duration = round_to_secs(interval.duration);
        let time_left_secs = round_to_secs(time_left);
        let overtime_secs = round_to_secs(overtime);

        let time_elapsed = (total_interval_duration - time_left_secs) + overtime_secs;
        debug_assert_eq!(
            time_elapsed + time_left_secs,
            total_interval_duration + overtime_secs,
            "Invariant violated: elapsed ({time_elapsed}) + left ({time_left_secs}) != total ({total_interval_duration}) + overtime ({overtime_secs})",
        );

        StatusResponse {
            interval_type: interval_type.clone(),
            state: self.timer.to_timer_state(),
            is_overtime: self.timer.is_overtime(now),
            overtime: overtime_secs,
            time_left: time_left_secs,
            time_elapsed,
            total_interval_duration,
        }
    }

    pub fn start(&mut self, now: SystemTime) {
        let interval_type = &self.config.cycle[self.cur_interval_idx];
        let duration = self.config.intervals[interval_type].duration;
        self.timer.start(now + duration);
    }

    pub fn next_interval(&mut self, now: SystemTime) {
        self.cur_interval_idx = (self.cur_interval_idx + 1) % self.config.cycle.len();
        let interval_type = &self.config.cycle[self.cur_interval_idx];
        let duration = self.config.intervals[interval_type].duration;
        self.timer.start(now + duration);
    }
}

pub async fn run_server(listener: UnixListener, config: Config) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(32);

    let initial_state = ServerState::new(config);
    let initial_status = initial_state.to_status_response(SystemTime::now());

    let (state_tx, state_rx) = watch::channel(initial_status);
    tokio::spawn(run_timer_actor(cmd_rx, state_tx, initial_state));

    log::info!("Pomidoro server ready and listening for connections");

    let server_uid = getuid().as_raw();

    let mut backoff = 1;

    loop {
        let (stream, _addr) = match listener.accept().await {
            Ok(accepted) => {
                backoff = 1;
                accepted
            },
            Err(e) => {
                log::error!("Failed to accept incoming connection: {e}");
                tokio::time::sleep(Duration::from_millis(backoff)).await;
                backoff = (backoff * 2).min(1000);
                continue;
            },
        };

        let cred = match stream.peer_cred() {
            Ok(cred) => cred,
            Err(e) => {
                log::error!("Failed to get peer credentials: {e}");
                continue;
            },
        };

        if !is_authorized(cred.uid(), server_uid) {
            log::warn!(
                "Dropping connection from unauthorized UID {} (server UID is {server_uid})",
                cred.uid(),
            );
            continue;
        }

        tokio::spawn(handle_client(stream, cmd_tx.clone(), state_rx.clone()));
    }
}

const fn is_authorized(client_uid: u32, server_uid: u32) -> bool {
    client_uid == server_uid
}

async fn run_timer_actor(
    mut cmd_rx: mpsc::Receiver<Command>,
    state_tx: watch::Sender<StatusResponse>,
    mut state: ServerState,
) {
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            Some(Command { request, reply }) = cmd_rx.recv() => {
                let now = SystemTime::now();
                let time_to_sec = |timer: &TimerState| timer.time_left_to_whole_second(now);

                let result = match request {
                    Request::Start => {
                        state.start(now);
                        ticker.reset_after(time_to_sec(&state.timer));
                        Ok(())
                    },
                    Request::NextInterval => {
                        state.next_interval(now);
                        ticker.reset_after(time_to_sec(&state.timer));
                        Ok(())
                    },
                    Request::Pause => {
                        state.timer.pause(now).map_err(|e| e.to_string())
                    },
                    Request::Resume => {
                        state.timer.resume(now)
                            .inspect(|()| ticker.reset_after(time_to_sec(&state.timer)))
                            .map_err(|e| e.to_string())
                    },
                    Request::Toggle => {
                        state.timer.toggle(now)
                            .inspect(|()| {
                                if state.timer.is_running() {
                                    ticker.reset_after(time_to_sec(&state.timer));
                                }
                            })
                            .map_err(|e| e.to_string())
                    },
                    Request::Stop => {
                        state.timer.stop();
                        state.cur_interval_idx = 0;
                        Ok(())
                    },
                    _ => unreachable!(),
                };

                let _ = state_tx.send(state.to_status_response(now));

                let (success, error_msg) = match result {
                    Ok(()) => (true, String::new()),
                    Err(e) => (false, e),
                };

                let response = ConfirmationResponse {
                    request,
                    success,
                    error_msg,
                };
                let _ = reply.send(response);
            },
            _ = ticker.tick(), if state.timer.is_running() => {
                let _ = state_tx.send(state.to_status_response(SystemTime::now()));
            },
        }
    }
}

async fn handle_client(
    stream: UnixStream,
    cmd_tx: mpsc::Sender<Command>,
    watch_rx: watch::Receiver<StatusResponse>,
) {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    if buf_reader.read_line(&mut line).await.unwrap_or(0) == 0 {
        return;
    }

    let request = match serde_json::from_str(&line) {
        Ok(req) => req,
        Err(e) => {
            let err_res = ConfirmationResponse {
                request: Request::Status,
                success: false,
                error_msg: format!("Invalid JSON request: {e}"),
            };
            let _ = send_json(&mut writer, &err_res).await;
            return;
        },
    };

    route_request(watch_rx, cmd_tx, &mut writer, request).await;
}

async fn route_request(
    mut watch_rx: watch::Receiver<StatusResponse>,
    cmd_tx: mpsc::Sender<Command>,
    writer: &mut tokio::io::WriteHalf<UnixStream>,
    request: Request,
) {
    match request {
        Request::Listen => {
            let cur_status = watch_rx.borrow().clone();
            if send_json(writer, &cur_status).await.is_err() {
                return;
            }
            while watch_rx.changed().await.is_ok() {
                let cur_status = watch_rx.borrow().clone();
                if send_json(writer, &cur_status).await.is_err() {
                    return;
                }
            }
        },
        Request::Status => {
            let cur_status = watch_rx.borrow().clone();
            let _ = send_json(writer, &cur_status).await;
        },
        req => {
            let (reply_tx, reply_rx) = oneshot::channel();
            let cmd = Command {
                request: req,
                reply: reply_tx,
            };
            if cmd_tx.send(cmd).await.is_ok()
                && let Ok(confirmation) = reply_rx.await
            {
                let _ = send_json(writer, &confirmation).await;
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{self, Config, IntervalConfig};
    use crate::protocol;

    use tokio::io::AsyncWriteExt;

    use std::collections::BTreeMap;

    fn dummy_config() -> Config {
        let mut intervals = BTreeMap::new();
        intervals.insert(
            "focus".to_string(),
            IntervalConfig {
                duration: Duration::from_secs(100),
            },
        );
        Config {
            cycle: vec!["focus".to_string()],
            intervals,
            socket: config::Socket::Abstract("\0hello".into()),
        }
    }

    #[test]
    fn next_interval() {
        let mut intervals = BTreeMap::new();
        intervals.insert(
            "focus".to_string(),
            IntervalConfig {
                duration: Duration::from_secs(100),
            },
        );
        intervals.insert(
            "break".to_string(),
            IntervalConfig {
                duration: Duration::from_secs(50),
            },
        );
        let config = Config {
            cycle: vec!["focus".to_string(), "break".to_string()],
            intervals,
            ..dummy_config()
        };
        let mut state = ServerState::new(config);
        let t0 = SystemTime::UNIX_EPOCH;

        // first interval
        let duration = state.config.intervals[&state.config.cycle[0]].duration;
        state.timer.start(t0 + duration);
        assert_eq!(state.cur_interval_idx, 0);
        assert_eq!(state.timer.time_left(t0), Duration::from_secs(100));

        // move to next interval
        state.next_interval(t0);
        assert_eq!(state.cur_interval_idx, 1);
        assert_eq!(state.timer.time_left(t0), Duration::from_secs(50));
        assert!(state.timer.is_running());

        // move to next (wraps around)
        state.next_interval(t0);
        assert_eq!(state.cur_interval_idx, 0);
        assert_eq!(state.timer.time_left(t0), Duration::from_secs(100));
        assert!(state.timer.is_running());

        // switch to next when currently in overtime
        let t_overtime = t0 + Duration::from_secs(150);
        assert_eq!(state.timer.time_left(t_overtime), Duration::ZERO);
        assert!(state.timer.is_overtime(t_overtime));

        state.next_interval(t_overtime);
        assert_eq!(state.cur_interval_idx, 1);
        assert_eq!(state.timer.time_left(t_overtime), Duration::from_secs(50));
        assert!(state.timer.is_running());
        assert!(!state.timer.is_overtime(t_overtime));

        // switch to next when currently paused
        let t_pause = t_overtime + Duration::from_secs(10);
        state.timer.pause(t_pause).unwrap();
        assert!(!state.timer.is_running());

        state.next_interval(t_pause);
        assert_eq!(state.cur_interval_idx, 0);
        assert_eq!(state.timer.time_left(t_pause), Duration::from_secs(100));
        assert!(state.timer.is_running());

        // switch to next when currently stopped
        state.timer.stop();
        assert!(!state.timer.is_running());

        let t_stopped = t_pause + Duration::from_secs(10);
        state.next_interval(t_stopped);
        assert_eq!(state.cur_interval_idx, 1);
        assert_eq!(state.timer.time_left(t_stopped), Duration::from_secs(50));
        assert!(state.timer.is_running());
    }

    #[test]
    fn test_is_authorized() {
        assert!(is_authorized(1000, 1000));
        assert!(!is_authorized(1001, 1000));
        assert!(!is_authorized(0, 1000));
    }

    #[tokio::test]
    async fn timer_actor() {
        let config = dummy_config();
        let initial_state = ServerState::new(config);

        let (cmd_tx, cmd_rx) = mpsc::channel(32);
        let (state_tx, mut state_rx) =
            watch::channel(initial_state.to_status_response(SystemTime::now()));

        tokio::spawn(run_timer_actor(cmd_rx, state_tx, initial_state));

        let status = state_rx.borrow().clone();
        assert!(matches!(status.state, protocol::TimerState::Stopped));

        let (reply_tx, reply_rx) = oneshot::channel();
        cmd_tx
            .send(Command {
                request: Request::Start,
                reply: reply_tx,
            })
            .await
            .unwrap();

        // wait for confirmation
        let conf = reply_rx.await.unwrap();
        assert!(conf.success);

        // wait for the state broadcast to update to Running
        state_rx.changed().await.unwrap();
        let status = state_rx.borrow().clone();
        assert!(matches!(status.state, protocol::TimerState::Running));
        assert_eq!(status.total_interval_duration, 100);
    }

    #[tokio::test]
    async fn invalid_json_request() {
        let (cmd_tx, _cmd_rx) = mpsc::channel(32);
        let config = dummy_config();
        let initial_state = ServerState::new(config);
        let (_, watch_rx) =
            watch::channel(initial_state.to_status_response(SystemTime::now()));

        let (client_stream, server_stream) = UnixStream::pair().unwrap();

        tokio::spawn(handle_client(server_stream, cmd_tx, watch_rx));

        let (reader, mut writer) = tokio::io::split(client_stream);

        writer.write_all(b"not valid json\n").await.unwrap();

        let mut buf_reader = BufReader::new(reader);
        let mut line = String::new();
        buf_reader.read_line(&mut line).await.unwrap();

        let expected_json = r#"{"request":"Status","success":false,"error_msg":"Invalid JSON request: expected value at line 1 column 1"}"#;
        assert_eq!(line.trim(), expected_json);
    }

    #[test]
    fn rounding_logic() {
        let config = dummy_config();
        let mut state = ServerState::new(config);
        let t0 = SystemTime::UNIX_EPOCH;

        // 14.499 -> 14
        state.timer.start(t0 + Duration::from_millis(14_499));
        let status = state.to_status_response(t0);
        assert_eq!(status.time_left, 14);

        // 14.500 -> 15
        state.timer.start(t0 + Duration::from_millis(14_500));
        let status = state.to_status_response(t0);
        assert_eq!(status.time_left, 15);

        // 5.499s overtime -> time_left: 0, overtime: 5
        state.timer.start(t0 - Duration::from_millis(5_499));
        let status = state.to_status_response(t0);
        assert_eq!(status.time_left, 0);
        assert_eq!(status.overtime, 5);
        assert!(status.is_overtime);

        // 5.500s overtime -> time_left: 0, overtime: 6
        state.timer.start(t0 - Duration::from_millis(5_500));
        let status = state.to_status_response(t0);
        assert_eq!(status.time_left, 0);
        assert_eq!(status.overtime, 6);
        assert!(status.is_overtime);
    }

    #[test]
    fn elapsed_time_and_restarts() {
        let config = dummy_config();
        let mut state = ServerState::new(config);
        let t0 = SystemTime::UNIX_EPOCH;

        state.start(t0);
        let status = state.to_status_response(t0);
        assert_eq!(status.time_elapsed, 0);
        assert_eq!(status.time_left, 100);
        assert_eq!(status.total_interval_duration, 100);

        // advance 40 seconds
        let t1 = t0 + Duration::from_secs(40);
        let status = state.to_status_response(t1);
        assert_eq!(status.time_elapsed, 40);
        assert_eq!(status.time_left, 60);
        assert_eq!(
            status.time_elapsed + status.time_left,
            status.total_interval_duration
        );

        let t2 = t0 + Duration::from_secs(115);
        let status = state.to_status_response(t2);
        assert_eq!(status.time_left, 0);
        assert_eq!(status.overtime, 15);
        assert_eq!(status.time_elapsed, 115);
        assert_eq!(
            status.time_elapsed,
            status.total_interval_duration + status.overtime
        );

        // already running
        state.start(t2);
        let status = state.to_status_response(t2);

        assert!(matches!(status.state, protocol::TimerState::Running));
        assert_eq!(status.time_elapsed, 0);
        assert_eq!(status.time_left, 100);
        assert_eq!(status.overtime, 0);
        assert!(!status.is_overtime);
    }
}
