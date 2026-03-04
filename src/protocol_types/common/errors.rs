use serde::{Deserialize, Serialize};

/// Error codes for server errors (4000-4499).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerErrorCode {
    Unknown = 4000,
    Unauthorized = 4001,
    QuotaExceeded = 4002,
    NotFound = 4003,
    InvalidMessageType = 4005,
    InvalidRequestFormat = 4006,
    Timeout = 4007,
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
