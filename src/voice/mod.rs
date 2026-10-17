//! Customer-configured voice sessions. Credentials stay in the local adapter.
#[cfg(feature = "livekit")]
mod config;
#[cfg(feature = "livekit")]
mod hooks;
#[cfg(feature = "livekit")]
mod livekit;
pub mod protocol;
mod runner;
mod session;
mod target;
#[cfg(feature = "livekit")]
pub use target::{PreparedTarget, prepare_livekit};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid voice configuration: {0}")]
    Configuration(&'static str),
    #[error("Voice hook failed; check the local script and its contract")]
    Hook,
    #[error("Voice endpoint authentication failed")]
    Authentication,
    #[error("Voice endpoint connection or transport failed")]
    Transport,
    #[error("Native voice transport failed during {0}")]
    #[cfg_attr(not(feature = "livekit"), allow(dead_code))]
    TransportStage(&'static str),
    #[error("Invalid voice protocol message")]
    Protocol,
    #[error("Voice evaluation failed: {0}")]
    Remote(String),
    #[error("Voice operation timed out")]
    Timeout,
    #[error("Voice evaluation cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(feature = "livekit")]
pub use runner::{run, run_single};

#[cfg(test)]
mod runner_tests;
