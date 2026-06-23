use crate::config::{Config, Hook, Hooks};
use crate::protocol::{Request, StatusResponse, TimerState};

use tokio::process::Command;

use std::env;

pub struct HookContext<'a> {
    pub config: &'a Config,
    pub status: &'a StatusResponse,
}

impl HookContext<'_> {
    pub fn try_execute<F>(&self, extract: F)
    where
        F: Fn(&Hooks) -> Option<&Hook>,
    {
        let interval_hooks = self
            .config
            .intervals
            .get(&self.status.interval_type)
            .map(|i| &i.hooks);

        let hook = interval_hooks
            .and_then(&extract)
            .or_else(|| extract(&self.config.hooks));

        if let Some(hook) = hook {
            HookManager::execute(hook, self.status);
        }
    }
}

pub struct HookManager {
    last_was_overtime: bool,
    last_overtime_secs: u64,
}

impl HookManager {
    #[must_use]
    pub const fn new(status: &StatusResponse) -> Self {
        Self {
            last_was_overtime: status.is_overtime,
            last_overtime_secs: status.overtime,
        }
    }

    pub const fn sync_state(&mut self, status: &StatusResponse) {
        self.last_was_overtime = status.is_overtime;
        self.last_overtime_secs = status.overtime;
    }

    fn set_env(cmd: &mut Command, status: &StatusResponse) {
        cmd.env("POMIDORO_INTERVAL_TYPE", &status.interval_type);
        cmd.env(
            "POMIDORO_STATE",
            format!("{:?}", status.state).to_uppercase(),
        );
        cmd.env(
            "POMIDORO_IS_OVERTIME",
            if status.is_overtime { "1" } else { "0" },
        );
        cmd.env("POMIDORO_OVERTIME", status.overtime.to_string());
        cmd.env("POMIDORO_TIME_LEFT", status.time_left.to_string());
        cmd.env("POMIDORO_TIME_ELAPSED", status.time_elapsed.to_string());
        cmd.env(
            "POMIDORO_TOTAL_INTERVAL_DURATION",
            status.total_interval_duration.to_string(),
        );
    }

    fn execute(hook: &Hook, status: &StatusResponse) {
        let mut cmd = match hook {
            Hook::Script(script) => {
                let shell = env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
                let mut cmd = Command::new(shell);
                cmd.arg("-c").arg(script);
                cmd
            },
            Hook::Command(command) if !command.is_empty() => {
                let mut cmd = Command::new(&command[0]);
                cmd.args(&command[1..]);
                cmd
            },
            Hook::Command(_) => return,
        };

        Self::set_env(&mut cmd, status);

        tokio::spawn(async move {
            match cmd.spawn() {
                Ok(mut child) => {
                    if let Err(e) = child.wait().await {
                        log::error!("Hook process failed: {e}");
                    }
                },
                Err(e) => {
                    log::error!("Failed to execute hook: {e}");
                },
            }
        });
    }

    pub fn handle_request(ctx: &HookContext, request: Request) {
        match request {
            Request::Start | Request::NextInterval => {
                ctx.try_execute(|h| h.on_start.as_ref());
            },
            Request::Pause => {
                ctx.try_execute(|h| h.on_pause.as_ref());
            },
            Request::Resume => {
                ctx.try_execute(|h| h.on_resume.as_ref());
            },
            Request::Toggle => {
                if matches!(ctx.status.state, TimerState::Paused) {
                    ctx.try_execute(|h| h.on_pause.as_ref());
                } else if matches!(ctx.status.state, TimerState::Running) {
                    ctx.try_execute(|h| h.on_resume.as_ref());
                }
            },
            _ => {},
        }
    }

    pub fn handle_tick(&mut self, ctx: &HookContext) {
        if ctx.status.is_overtime && !self.last_was_overtime {
            ctx.try_execute(|h| h.on_completion.as_ref());
        }

        if ctx.status.is_overtime {
            let interval_hooks = ctx
                .config
                .intervals
                .get(&ctx.status.interval_type)
                .map(|i| &i.hooks);
            let overtime_reminder = interval_hooks
                .and_then(|h| h.overtime.as_ref())
                .or(ctx.config.hooks.overtime.as_ref());

            if let Some(reminder) = overtime_reminder {
                let every_secs = reminder.every.as_secs();
                #[allow(clippy::collapsible_if)]
                if let (Some(last_count), Some(current_count)) = (
                    self.last_overtime_secs.checked_div(every_secs),
                    ctx.status.overtime.checked_div(every_secs),
                ) && current_count > last_count
                    && current_count > 0
                {
                    Self::execute(&reminder.execute, ctx.status);
                }
            }
        }

        self.sync_state(ctx.status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_manager_sync_state() {
        let mut status = StatusResponse {
            interval_type: "work".to_string(),
            state: TimerState::Running,
            is_overtime: false,
            overtime: 0,
            time_left: 60,
            time_elapsed: 0,
            total_interval_duration: 60,
        };

        let mut manager = HookManager::new(&status);
        assert!(!manager.last_was_overtime);
        assert_eq!(manager.last_overtime_secs, 0);

        status.is_overtime = true;
        status.overtime = 15;

        manager.sync_state(&status);
        assert!(manager.last_was_overtime);
        assert_eq!(manager.last_overtime_secs, 15);
    }

    #[test]
    fn hook_context_resolution() {
        let toml_str = r#"
            [hooks]
            on_start = "global start"
            
            [intervals.work]
            duration = "25m"
            [intervals.work.hooks]
            on_start = "work start"
            on_pause = "work pause"
        "#;
        let config = Config::parse(toml_str).unwrap();

        let status = StatusResponse {
            interval_type: "work".to_string(),
            state: TimerState::Running,
            is_overtime: false,
            overtime: 0,
            time_left: 60,
            time_elapsed: 0,
            total_interval_duration: 60,
        };

        // interval hook taking precedence
        let interval_hooks = config
            .intervals
            .get(&status.interval_type)
            .map(|i| &i.hooks);
        let start_hook = interval_hooks
            .and_then(|h| h.on_start.as_ref())
            .or(config.hooks.on_start.as_ref());
        assert_eq!(start_hook, Some(&Hook::Script("work start".to_string())));

        // fallback to global hook
        let status_other = StatusResponse {
            interval_type: "short break".to_string(),
            ..status.clone()
        };
        let interval_hooks_other = config
            .intervals
            .get(&status_other.interval_type)
            .map(|i| &i.hooks);
        let start_hook_other = interval_hooks_other
            .and_then(|h| h.on_start.as_ref())
            .or(config.hooks.on_start.as_ref());
        assert_eq!(
            start_hook_other,
            Some(&Hook::Script("global start".to_string()))
        );
    }
}
