use super::protocol;

use thiserror::Error;

use std::time::{Duration, SystemTime};

#[derive(Debug, Error)]
pub enum TimerError {
    #[error("Timer is not running")]
    NotRunning,
    #[error("Timer is not paused")]
    NotPaused,
}

pub enum TimerState {
    Stopped,
    Paused {
        time_left: Duration,
        overtime: Duration,
    },
    Running {
        end_time: SystemTime,
    },
}

impl TimerState {
    pub const fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    pub fn is_overtime(&self, now: SystemTime) -> bool {
        self.overtime(now) > Duration::ZERO
    }

    pub fn overtime(&self, now: SystemTime) -> Duration {
        match self {
            Self::Stopped => Duration::ZERO,
            Self::Paused { overtime, .. } => *overtime,
            Self::Running { end_time } => {
                now.duration_since(*end_time).unwrap_or(Duration::ZERO)
            },
        }
    }

    pub fn time_left(&self, now: SystemTime) -> Duration {
        match self {
            Self::Stopped => Duration::ZERO,
            Self::Paused { time_left, .. } => *time_left,
            Self::Running { end_time } => {
                end_time.duration_since(now).unwrap_or(Duration::ZERO)
            },
        }
    }

    pub fn time_left_to_whole_second(&self, now: SystemTime) -> Duration {
        let subsec = |d: Duration| Duration::from_nanos(u64::from(d.subsec_nanos()));
        let calc_overtime = |overtime: Duration| {
            let rem = subsec(overtime);
            if rem == Duration::ZERO {
                Duration::ZERO
            } else {
                Duration::from_secs(1).checked_sub(rem).unwrap()
            }
        };

        match self {
            Self::Stopped => Duration::ZERO,
            Self::Paused {
                time_left,
                overtime,
            } => {
                if *time_left > Duration::ZERO {
                    subsec(*time_left)
                } else {
                    calc_overtime(*overtime)
                }
            },
            Self::Running { end_time } => end_time
                .duration_since(now)
                .map_or_else(|e| calc_overtime(e.duration()), subsec),
        }
    }

    pub fn toggle(&mut self, now: SystemTime) -> Result<(), TimerError> {
        match self {
            Self::Stopped => Err(TimerError::NotRunning),
            Self::Paused { .. } => self.resume(now),
            Self::Running { .. } => self.pause(now),
        }
    }

    pub fn pause(&mut self, now: SystemTime) -> Result<(), TimerError> {
        let Self::Running { end_time } = *self else {
            return Err(TimerError::NotRunning);
        };
        *self = end_time.duration_since(now).map_or_else(
            |_| Self::Paused {
                time_left: Duration::ZERO,
                overtime: now.duration_since(end_time).unwrap_or(Duration::ZERO),
            },
            |time_left| Self::Paused {
                time_left,
                overtime: Duration::ZERO,
            },
        );
        Ok(())
    }

    pub fn resume(&mut self, now: SystemTime) -> Result<(), TimerError> {
        let Self::Paused {
            time_left,
            overtime,
        } = *self
        else {
            return Err(TimerError::NotPaused);
        };

        *self = Self::Running {
            end_time: now
                .checked_add(time_left)
                .and_then(|t| t.checked_sub(overtime))
                .unwrap_or(SystemTime::UNIX_EPOCH),
        };
        Ok(())
    }

    pub const fn stop(&mut self) {
        *self = Self::Stopped;
    }

    pub const fn start(&mut self, end_time: SystemTime) {
        *self = Self::Running { end_time };
    }

    pub const fn to_timer_state(&self) -> protocol::TimerState {
        match self {
            Self::Stopped => protocol::TimerState::Stopped,
            Self::Paused { .. } => protocol::TimerState::Paused,
            Self::Running { .. } => protocol::TimerState::Running,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correctness() {
        let t0 = SystemTime::UNIX_EPOCH;
        let mut timer = TimerState::Stopped;

        assert_eq!(timer.time_left(t0), Duration::ZERO);
        assert!(!timer.is_running());

        timer.start(t0 + Duration::from_secs(100));
        assert!(timer.is_running());
        assert_eq!(timer.time_left(t0), Duration::from_secs(100));

        let t1 = t0 + Duration::from_secs(10);
        assert_eq!(timer.time_left(t1), Duration::from_secs(90));

        timer.pause(t1).unwrap();
        assert!(!timer.is_running());
        assert_eq!(timer.time_left(t1), Duration::from_secs(90));

        let t2 = t1 + Duration::from_secs(50);
        timer.resume(t2).unwrap();
        assert!(timer.is_running());
        // time left should still be 90s immediately after resume
        assert_eq!(timer.time_left(t2), Duration::from_secs(90));

        // advance into overtime
        let t3 = t2 + Duration::from_secs(100);
        assert_eq!(timer.time_left(t3), Duration::ZERO);
        assert_eq!(timer.overtime(t3), Duration::from_secs(10));
        assert!(timer.is_overtime(t3));

        // pause in overtime
        timer.pause(t3).unwrap();
        assert_eq!(timer.time_left(t3), Duration::ZERO);
        assert_eq!(timer.overtime(t3), Duration::from_secs(10));

        // resume from overtime 20s later
        let t4 = t3 + Duration::from_secs(20);
        timer.resume(t4).unwrap();
        assert_eq!(timer.time_left(t4), Duration::ZERO);
        assert_eq!(timer.overtime(t4), Duration::from_secs(10));

        // toggle to pause
        let t5 = t4 + Duration::from_secs(10);
        timer.toggle(t5).unwrap();
        assert!(!timer.is_running());
        assert_eq!(timer.overtime(t5), Duration::from_secs(20));

        // toggle to resume
        let t6 = t5 + Duration::from_secs(5);
        timer.toggle(t6).unwrap();
        assert!(timer.is_running());
        assert_eq!(timer.overtime(t6), Duration::from_secs(20));
    }

    #[test]
    fn edge_cases() {
        let t0 = SystemTime::UNIX_EPOCH;
        let mut timer = TimerState::Stopped;

        assert!(matches!(timer.pause(t0), Err(TimerError::NotRunning)));
        assert!(matches!(timer, TimerState::Stopped));

        assert!(matches!(timer.resume(t0), Err(TimerError::NotPaused)));
        assert!(matches!(timer, TimerState::Stopped));

        assert!(matches!(timer.toggle(t0), Err(TimerError::NotRunning)));
        assert!(matches!(timer, TimerState::Stopped));

        timer.stop();
        assert!(matches!(timer, TimerState::Stopped));

        timer.start(t0 + Duration::from_secs(10));

        assert!(matches!(timer.resume(t0), Err(TimerError::NotPaused)));
        assert!(timer.is_running());
        assert_eq!(timer.time_left(t0), Duration::from_secs(10));

        timer.pause(t0).unwrap();

        assert!(matches!(
            timer.pause(t0 + Duration::from_secs(5)),
            Err(TimerError::NotRunning)
        ));
        assert!(!timer.is_running());
        assert_eq!(timer.time_left(t0), Duration::from_secs(10));

        // overtime calculations
        timer.resume(t0).unwrap();
        let t_huge = t0 + Duration::from_secs(1_000_000);
        assert_eq!(timer.time_left(t_huge), Duration::ZERO);
        assert_eq!(timer.overtime(t_huge), Duration::from_secs(1_000_000 - 10));
    }

    #[test]
    fn time_left_to_whole_second() {
        let t0 = SystemTime::UNIX_EPOCH;
        let mut timer = TimerState::Stopped;

        assert_eq!(timer.time_left_to_whole_second(t0), Duration::ZERO);

        timer.start(t0 + Duration::from_secs(10));
        assert_eq!(timer.time_left_to_whole_second(t0), Duration::ZERO);

        timer.start(t0 + Duration::from_millis(10_250));
        assert_eq!(
            timer.time_left_to_whole_second(t0),
            Duration::from_millis(250)
        );

        timer.start(t0 - Duration::from_secs(5));
        assert_eq!(timer.time_left_to_whole_second(t0), Duration::ZERO);

        timer.start(t0 - Duration::from_millis(5_750));
        assert_eq!(
            timer.time_left_to_whole_second(t0),
            Duration::from_millis(250)
        );

        timer.start(t0 + Duration::from_millis(3_100));
        timer.pause(t0).unwrap();
        assert_eq!(
            timer.time_left_to_whole_second(t0),
            Duration::from_millis(100)
        );

        timer.start(t0 + Duration::from_secs(3));
        timer.pause(t0).unwrap();
        assert_eq!(timer.time_left_to_whole_second(t0), Duration::ZERO);

        timer.start(t0 - Duration::from_millis(4_800));
        timer.pause(t0).unwrap();
        assert_eq!(
            timer.time_left_to_whole_second(t0),
            Duration::from_millis(200)
        );

        timer.start(t0 - Duration::from_secs(4));
        timer.pause(t0).unwrap();
        assert_eq!(timer.time_left_to_whole_second(t0), Duration::ZERO);
    }
}
