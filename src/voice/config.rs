use super::{Error, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_label")]
    pub label: String,
    pub url: Option<String>,
    pub token_env: Option<String>,
    pub bootstrap_script: Option<PathBuf>,
    pub control_script: PathBuf,
    pub participant: Option<String>,
    pub track: Option<String>,
    #[serde(default)]
    pub playback_ticks: bool,
    #[serde(default)]
    pub parameters: BTreeMap<String, Value>,
    #[serde(default)]
    pub credentials: BTreeMap<String, String>,
}

fn default_label() -> String {
    "livekit".into()
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .map_err(|_| Error::Configuration("cannot read target file"))?;
        let mut config: Self =
            toml::from_str(&contents).map_err(|_| Error::Configuration("invalid target TOML"))?;
        if config.label.trim().is_empty() || config.label.len() > 128 {
            return Err(Error::Configuration("label must contain 1–128 bytes"));
        }
        let direct = config.url.is_some() && config.token_env.is_some();
        if direct == config.bootstrap_script.is_some()
            || (!direct && (config.url.is_some() || config.token_env.is_some()))
        {
            return Err(Error::Configuration(
                "choose url/token_env or bootstrap_script",
            ));
        }
        let root = path.parent().unwrap_or(Path::new("."));
        config.control_script = root.join(&config.control_script);
        config.bootstrap_script = config.bootstrap_script.map(|p| root.join(p));
        if let Some(url) = &config.url {
            validate_url(url, &["wss", "ws"])?;
        }
        Ok(config)
    }

    pub fn context(&self, session_id: i32, max_turns: u32) -> Result<Value> {
        let mut credentials = BTreeMap::new();
        for (key, name) in &self.credentials {
            credentials.insert(key.clone(), read_secret(name)?);
        }
        Ok(json!({"session_id": session_id, "max_turns": max_turns,
            "parameters": self.parameters, "credentials": credentials}))
    }
}

pub fn read_secret(name: &str) -> Result<String> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .ok_or(Error::Configuration(
            "a required credential environment variable is missing",
        ))
}

pub fn validate_url(value: &str, schemes: &[&str]) -> Result<()> {
    let url = url::Url::parse(value).map_err(|_| Error::Configuration("invalid endpoint URL"))?;
    if !schemes.contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Error::Configuration("unsupported endpoint URL"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_non_network_and_embedded_credentials() {
        for url in [
            "file:///tmp/token",
            "wss://user:secret@example.com",
            "invalid",
        ] {
            assert!(validate_url(url, &["wss", "ws"]).is_err());
        }
        assert!(validate_url("ws://localhost:7880", &["wss", "ws"]).is_ok());
    }
}
