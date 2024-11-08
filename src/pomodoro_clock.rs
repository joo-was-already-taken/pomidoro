use super::cli;
use super::config::Session;
use super::socket::{ServerState, ServerAction};

use serde::{Serialize, Deserialize};
use chrono::NaiveTime;

use std::fmt;
use std::time::{Duration, Instant};
use std::error::Error;
use std::ops::Range;


pub fn duration_fmt(duration: Duration, fmt: &str) -> String {
    let seconds = (duration.as_secs() % 60) as u32;
    let minutes = ((duration.as_secs() % 3600) / 60) as u32;
    let hours = (duration.as_secs() / 3600) as u32;

    let time = NaiveTime::from_hms_opt(hours, minutes, seconds).unwrap();
    time.format(fmt).to_string()
}


/// Provided instant is older than `Clock`'s resumed time
#[derive(Debug)]
pub struct ClockError;

impl Error for ClockError {}

impl fmt::Display for ClockError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Provided `Instant` is older than the resumed time")
    }
}

enum Clock {
    Running { resumed: Instant, offset: Duration },
    Paused { elapsed: Duration },
}

impl Clock {
    pub fn duration_until(&self, until: Instant) -> Result<Duration, ClockError> {
        match *self {
            Self::Running { resumed, offset } => (until >= resumed)
                .then_some(until.duration_since(resumed) + offset)
                .ok_or(ClockError),
            Self::Paused { elapsed } => Ok(elapsed),
        }
    }

    pub fn toggle(&self, now: Instant) -> Result<Self, ClockError> {
        match *self {
            Self::Running { resumed, offset } => (now >= resumed)
                .then_some(Self::Paused { elapsed: offset + (now - resumed) })
                .ok_or(ClockError),
            Self::Paused { elapsed } => Ok(Self::Running {
                resumed: now,
                offset: elapsed,
            }),
        }
    }

    pub fn skip_by(&self, time: Duration) -> Self {
        match *self {
            Self::Running { resumed, offset } => Self::Running {
                resumed,
                offset: offset + time,
            },
            Self::Paused { elapsed } => Self::Paused {
                elapsed: elapsed + time,
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct PomodoroState {
    pub is_paused: bool,
    pub time: String,
    pub session_name: String,
    pub session_duration: String,
    pub percent: u32,
}

pub struct PomodoroClock<'a> {
    clock: Clock,
    default_time_format: &'a str,
    sessions: Vec<&'a Session>,
}

impl<'a> PomodoroClock<'a> {
    const NO_SESSIONS_MSG: &'static str = "There should be at least one session defined";

    pub fn paused(sessions: impl Iterator<Item = &'a Session>, default_time_format: &'a str) -> Self {
        Self {
            clock: Clock::Paused { elapsed: Duration::ZERO },
            default_time_format,
            sessions: sessions.collect(),
        }
    }

    fn sessions_bounds(&self) -> impl Iterator<Item = Range<Duration>> + '_ {
        self.sessions
            .iter()
            .map(|session| session.duration)
            .scan(Duration::ZERO, |pref_sum, duration| {
                let bounds = *pref_sum..(*pref_sum + duration);
                *pref_sum = bounds.end;
                Some(bounds)
            })
    }

    fn elapsed_until(&self, instant: Instant) -> Result<Duration, ClockError> {
        fn duration_rem(dividend: Duration, divisor: Duration) -> Duration {
            let nanos_per_sec = 1_000_000_000;
            let nanos: u128 = dividend.as_nanos() % divisor.as_nanos();
            Duration::new((nanos / nanos_per_sec) as u64, (nanos % nanos_per_sec) as u32)
        }
        let cycle_duration: Duration = self.sessions
            .iter()
            .map(|session| session.duration)
            .sum();
        let elapsed = duration_rem(
            self.clock.duration_until(instant)?,
            cycle_duration,
        );
        Ok(elapsed)
    }

    pub fn state_at(&self, instant: Instant) -> Result<PomodoroState, ClockError> {
        let elapsed = self.elapsed_until(instant)?;

        let (session, time_left) = self.sessions
            .iter()
            .zip(self.sessions_bounds())
            .map_while(|(session, bounds)| {
                let is_current_session = bounds.contains(&elapsed);
                (elapsed >= bounds.end || is_current_session).then(|| {
                    let session_time_left = bounds.end
                        .checked_sub(elapsed)
                        .unwrap_or_default();
                    (session, session_time_left)
                })
            })
            .last()
            .expect(Self::NO_SESSIONS_MSG);
        let time_format = session.time_format
            .as_deref()
            .unwrap_or(self.default_time_format);

        let percent = {
            let elapsed = (session.duration - time_left).as_secs_f64();
            let fraction = elapsed / session.duration.as_secs_f64();
            if fraction.is_infinite() {
                0
            } else {
                (fraction * 100.0) as u32
            }
        };

        Ok(PomodoroState {
            is_paused: matches!(self.clock, Clock::Paused { .. }),
            session_name: session.name.clone(),
            session_duration: duration_fmt(session.duration, time_format),
            time: duration_fmt(time_left, time_format),
            percent,
        })
    }

    pub fn toggle(&mut self, now: Instant) -> Result<(), ClockError> {
        self.clock = self.clock.toggle(now)?;
        Ok(())
    }

    pub fn skip_session(&mut self, now: Instant) -> Result<(), ClockError> {
        let elapsed = self.elapsed_until(now)?;
        let session_bounds = self.sessions_bounds()
            .take_while(|bounds| elapsed >= bounds.end || bounds.contains(&elapsed))
            .last()
            .expect(Self::NO_SESSIONS_MSG);
        let skip_by = session_bounds.end - elapsed;
        self.clock = self.clock.skip_by(skip_by);
        Ok(())
    }

    pub fn reset(&mut self) {
        self.clock = Clock::Paused { elapsed: Duration::ZERO };
    }
}


#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    Fetch,
    Toggle,
    Skip,
    Reset,
    Stop,
}

impl From<&cli::Request> for Request {
    fn from(value: &cli::Request) -> Self {
        match value {
            cli::Request::Fetch { .. } => Self::Fetch,
            cli::Request::Toggle => Self::Toggle,
            cli::Request::Skip => Self::Skip,
            cli::Request::Reset => Self::Reset,
            cli::Request::Stop => Self::Stop,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    State(PomodoroState),
    Confirmation(Result<(), String>),
}


impl ServerState for PomodoroClock<'_> {
    type Request<'de> = Request;
    type Response = Response;

    fn update<'de>(&mut self, request: &Self::Request<'de>) -> ServerAction<Self::Response> {
        let sys_clock_err_msg = "your system clock is prbly doomed, idk 💀";
        let now = Instant::now();

        match request {
            Request::Toggle => {
                self.toggle(now).expect(sys_clock_err_msg);
                ServerAction::Respond(Response::Confirmation(Ok(())))
            },
            Request::Skip => {
                self.skip_session(now).expect(sys_clock_err_msg);
                ServerAction::Respond(Response::Confirmation(Ok(())))
            },
            Request::Reset => {
                self.reset();
                ServerAction::Respond(Response::Confirmation(Ok(())))
            }
            Request::Fetch { .. } => {
                let state = self.state_at(now).expect(sys_clock_err_msg);
                ServerAction::Respond(Response::State(state))
            },
            Request::Stop => ServerAction::StopRespond(Response::Confirmation(Ok(()))),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pomodoro_state_at() {
        let sessions = vec![
            Session {
                name: "work1".into(),
                duration: Duration::from_secs(200),
                time_format: None,
            },
            Session {
                name: "rest".into(),
                duration: Duration::from_secs(100),
                time_format: None,
            },
            Session {
                name: "work2".into(),
                duration: Duration::from_secs(200),
                time_format: None,
            },
            Session {
                name: "long rest".into(),
                duration: Duration::from_secs(150),
                time_format: None,
            },
        ];
        let pomodoro_clock = PomodoroClock {
            clock: Clock::Paused { elapsed: Duration::from_secs(950) },
            default_time_format: "%M:%S",
            sessions: sessions.iter().collect(),
        };

        assert_eq!(
            pomodoro_clock.state_at(Instant::now()).unwrap(),
            PomodoroState {
                is_paused: true,
                session_name: "work2".into(),
                session_duration: "03:20".into(),
                time: "03:20".into(),
                percent: 0,
            },
        );
    }

    #[test]
    fn pomodoro_skip() {
        let sessions = vec![
            Session {
                name: "work1".into(),
                duration: Duration::from_secs(8),
                time_format: None,
            },
        ];
        let mut pomodoro_clock = PomodoroClock {
            clock: Clock::Paused { elapsed: Duration::from_secs_f32(5.07) },
            default_time_format: "%M:%S",
            sessions: sessions.iter().collect(),
        };
        let _ = pomodoro_clock.skip_session(Instant::now());
        assert_eq!(
            pomodoro_clock.elapsed_until(Instant::now()).unwrap(),
            Duration::from_secs(0),
        );
    }
}
