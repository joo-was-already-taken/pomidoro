pub mod client;
pub mod config;
mod hooks;
mod protocol;
mod server;
mod timer_state;
pub use client::Client;
pub use config::{Config, IntervalConfig};
pub use protocol::{
    ConfirmationResponse, IntervalInfo, ListenMode, Request, ServerEvent, ServerStatus,
    TimerState, send_json,
};
pub use server::run_server;
