//! Types shared by the versioned voice wire protocols.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolVersion {
    #[serde(rename = "1")]
    V1,
    #[serde(rename = "2")]
    V2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceTransport {
    Livekit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceErrorCode {
    Authentication,
    Configuration,
    Transport,
    Timeout,
    SynthesisFailed,
    TranscriptionFailed,
    InvalidResponse,
    InvalidRequest,
    NotFound,
    EvaluationFailed,
    InternalError,
}

impl std::fmt::Display for VoiceErrorCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = serde_json::to_value(self).map_err(|_| std::fmt::Error)?;
        formatter.write_str(value.as_str().ok_or(std::fmt::Error)?)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionErrorCode {
    Authentication,
    Configuration,
    Transport,
    Timeout,
    InvalidResponse,
    Cancelled,
    Overflow,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceTarget {
    pub label: String,
    pub transport: VoiceTransport,
    pub cli_version: String,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub enum BootstrapMethod {
    #[serde(rename = "GET")]
    Get,
    #[serde(rename = "POST")]
    Post,
}

impl From<BootstrapMethod> for reqwest::Method {
    fn from(method: BootstrapMethod) -> Self {
        match method {
            BootstrapMethod::Get => Self::GET,
            BootstrapMethod::Post => Self::POST,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_wire_values_are_closed_and_stable() {
        assert_eq!(serde_json::to_value(ProtocolVersion::V1).unwrap(), "1");
        assert_eq!(serde_json::to_value(ProtocolVersion::V2).unwrap(), "2");
        assert_eq!(
            serde_json::to_value(VoiceTransport::Livekit).unwrap(),
            "livekit"
        );
        assert_eq!(
            serde_json::to_value(SessionErrorCode::InvalidResponse).unwrap(),
            "invalid_response"
        );
        assert!(serde_json::from_str::<ProtocolVersion>(r#""3""#).is_err());
        assert!(serde_json::from_str::<VoiceTransport>(r#""unknown""#).is_err());
    }
}
