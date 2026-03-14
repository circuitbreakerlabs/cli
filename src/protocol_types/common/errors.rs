use crate::response_provider::ProviderError;
use serde::{Deserialize, Serialize};

/// Error codes for server errors (4000-4499).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum ServerErrorCode {
    Unknown = 4000,
    Unauthorized = 4001,
    QuotaExceeded = 4002,
    NotFound = 4003,
    VersionMismatch = 4004,
    InvalidMessageType = 4005,
    InvalidRequestFormat = 4006,
    Timeout = 4007,
}

impl TryFrom<u16> for ServerErrorCode {
    type Error = u16;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            4000 => Ok(Self::Unknown),
            4001 => Ok(Self::Unauthorized),
            4002 => Ok(Self::QuotaExceeded),
            4003 => Ok(Self::NotFound),
            4004 => Ok(Self::VersionMismatch),
            4005 => Ok(Self::InvalidMessageType),
            4006 => Ok(Self::InvalidRequestFormat),
            4007 => Ok(Self::Timeout),
            _ => Err(value),
        }
    }
}

/// Error reasons for completion errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionErrorReason {
    ModelTimeout,
    ModelUnreachable,
    InvalidResponse,
    RateLimited,
    AuthenticationFailed,
    Unknown,
}

/// Payload for `completion_error` messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionError {
    pub request_id: String,
    pub error_reason: CompletionErrorReason,
}

/// Client returns an error that occurred while processing a completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionErrorEnvelope {
    #[serde(rename = "type")]
    pub message_type: String,
    pub data: CompletionError,
}

impl From<&ProviderError> for CompletionErrorReason {
    fn from(error: &ProviderError) -> Self {
        match error {
            ProviderError::Network(_) => CompletionErrorReason::ModelUnreachable,
            ProviderError::Api(msg) => {
                if msg.contains("rate") || msg.contains("Rate") {
                    CompletionErrorReason::RateLimited
                } else if msg.contains("auth") || msg.contains("Auth") || msg.contains("key") {
                    CompletionErrorReason::AuthenticationFailed
                } else {
                    CompletionErrorReason::Unknown
                }
            }
            ProviderError::Parsing(_) => CompletionErrorReason::InvalidResponse,
            _ => CompletionErrorReason::Unknown,
        }
    }
}

impl From<CompletionError> for CompletionErrorEnvelope {
    fn from(error: CompletionError) -> Self {
        CompletionErrorEnvelope {
            message_type: "completion_error".to_string(),
            data: error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CompletionError, CompletionErrorEnvelope, CompletionErrorReason, ServerErrorCode};
    use crate::response_provider::ProviderError;
    use serde_json::json;

    #[test]
    fn parses_known_server_error_codes() {
        assert_eq!(
            ServerErrorCode::try_from(4000),
            Ok(ServerErrorCode::Unknown)
        );
        assert_eq!(
            ServerErrorCode::try_from(4001),
            Ok(ServerErrorCode::Unauthorized)
        );
        assert_eq!(
            ServerErrorCode::try_from(4002),
            Ok(ServerErrorCode::QuotaExceeded)
        );
        assert_eq!(
            ServerErrorCode::try_from(4003),
            Ok(ServerErrorCode::NotFound)
        );
        assert_eq!(
            ServerErrorCode::try_from(4004),
            Ok(ServerErrorCode::VersionMismatch)
        );
        assert_eq!(
            ServerErrorCode::try_from(4005),
            Ok(ServerErrorCode::InvalidMessageType)
        );
        assert_eq!(
            ServerErrorCode::try_from(4006),
            Ok(ServerErrorCode::InvalidRequestFormat)
        );
        assert_eq!(
            ServerErrorCode::try_from(4007),
            Ok(ServerErrorCode::Timeout)
        );
    }

    #[test]
    fn rejects_unknown_server_error_codes() {
        assert_eq!(ServerErrorCode::try_from(3999), Err(3999));
        assert_eq!(ServerErrorCode::try_from(4008), Err(4008));
    }

    #[test]
    fn maps_network_errors_to_model_unreachable() {
        let reason = CompletionErrorReason::from(&ProviderError::Network("boom".to_string()));

        assert!(matches!(reason, CompletionErrorReason::ModelUnreachable));
    }

    #[test]
    fn maps_rate_limit_api_errors() {
        let reason =
            CompletionErrorReason::from(&ProviderError::Api("Rate limit exceeded".to_string()));

        assert!(matches!(reason, CompletionErrorReason::RateLimited));
    }

    #[test]
    fn maps_auth_api_errors() {
        let reason = CompletionErrorReason::from(&ProviderError::Api(
            "Invalid API key provided".to_string(),
        ));

        assert!(matches!(
            reason,
            CompletionErrorReason::AuthenticationFailed
        ));
    }

    #[test]
    fn maps_parsing_errors_to_invalid_response() {
        let reason = CompletionErrorReason::from(&ProviderError::Parsing("bad json".to_string()));

        assert!(matches!(reason, CompletionErrorReason::InvalidResponse));
    }

    #[test]
    fn leaves_other_errors_as_unknown() {
        let reason =
            CompletionErrorReason::from(&ProviderError::Script("script failed".to_string()));

        assert!(matches!(reason, CompletionErrorReason::Unknown));
    }

    #[test]
    fn completion_error_envelope_serializes_to_protocol_shape() {
        let envelope = CompletionErrorEnvelope::from(CompletionError {
            request_id: "req-456".to_string(),
            error_reason: CompletionErrorReason::RateLimited,
        });

        let value = serde_json::to_value(envelope).expect("envelope should serialize");

        assert_eq!(
            value,
            json!({
                "type": "completion_error",
                "data": {
                    "request_id": "req-456",
                    "error_reason": "rate_limited"
                }
            })
        );
    }
}
