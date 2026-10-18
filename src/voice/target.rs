//! Adapter selection and local preparation before contacting the evaluation API.
use super::session::VoiceSessionProvider;
use std::sync::Arc;
#[cfg(feature = "livekit")]
use {
    super::{Error, Result},
    std::path::Path,
};

pub struct TargetMetadata {
    pub label: String,
    pub transport: &'static str,
}

pub struct PreparedTarget {
    pub provider: Arc<dyn VoiceSessionProvider>,
    pub metadata: TargetMetadata,
}

#[cfg(feature = "livekit")]
pub fn prepare_livekit(path: &Path, max_turns: usize) -> Result<PreparedTarget> {
    use super::{
        config::{Config, read_secret},
        hooks::{Hooks, Script},
        livekit::Provider,
    };
    let config = Config::load(path)?;
    config.context(0, u32::try_from(max_turns).map_err(|_| Error::Protocol)?)?;
    Hooks::new(&config)?;
    if let Some(path) = &config.bootstrap_script {
        Script::load(path)?;
    } else {
        read_secret(
            config
                .token_env
                .as_deref()
                .ok_or(Error::Configuration("missing token_env"))?,
        )?;
    }
    Ok(PreparedTarget {
        metadata: TargetMetadata {
            label: config.label.clone(),
            transport: "livekit",
        },
        provider: Arc::new(Provider(config)),
    })
}
