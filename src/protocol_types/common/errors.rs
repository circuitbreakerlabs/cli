use serde::{Deserialize, Serialize};

/// Error codes for completion errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CompletionErrorCode {
    ModelTimeout,
    ModelUnreachable,
    InvalidResponse,
    RateLimited,
    AuthenticationFailed,
    Unknown,
}

/// Payload for completion_error messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionErrorObject {
    /// Must match the ID from the corresponding CompletionRequest
    pub request_id: String,
    /// Machine-readable error code
    pub error_code: CompletionErrorCode,
    /// Human-readable error description
    pub error_message: String,
}

/// Client indicates inability to obtain a completion from the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionError {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Error payload
    pub object: CompletionErrorObject,
}

/// Error codes for server errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServerErrorCode {
    Parse,
    Unknown,
}

/// Payload for error messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorObject {
    /// Machine-readable error code
    pub error_code: ServerErrorCode,
    /// Human-readable error description
    pub error_message: String,
}

/// Server indicates an error occurred during evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerError {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Error payload
    pub object: ErrorObject,
}
