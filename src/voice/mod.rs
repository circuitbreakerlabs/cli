#![cfg_attr(not(feature = "voice"), allow(dead_code))]
//! Customer-configured voice sessions. Customer configuration never leaves this process.
#[cfg(any(all(feature = "voice", not(target_env = "musl")), test))]
mod config;
#[cfg(any(all(feature = "voice", not(target_env = "musl")), test))]
mod hooks;
#[cfg(all(feature = "voice", not(target_env = "musl")))]
mod livekit;
#[cfg(any(all(feature = "voice", not(target_env = "musl")), test))]
pub mod protocol;
#[cfg(all(feature = "voice", not(target_env = "musl")))]
mod runner;

use std::path::PathBuf;

#[derive(Clone, Debug, clap::Subcommand)]
pub enum ProviderCommand {
    /// Connect to a customer `LiveKit` endpoint
    Livekit {
        /// Customer-owned TOML connection and control configuration
        #[arg(long)]
        config: PathBuf,
    },
}

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
    TransportStage(&'static str),
    #[error("Invalid voice protocol message")]
    Protocol,
    #[error("Voice operation timed out")]
    Timeout,
    #[error("Voice evaluation cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(all(feature = "voice", not(target_env = "musl")))]
pub use runner::run;
