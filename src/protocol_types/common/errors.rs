use serde::{Deserialize, Serialize};

/// Error codes for completion errors (4500-4599).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompletionErrorCode {
    UNKNOWN = 4500,
    MODEL_TIMEOUT = 4501,
    MODEL_UNREACHABLE = 4502,
    INVALID_RESPONSE = 4503,
    RATE_LIMITED = 4504,
    AUTHENTICATION_FAILED = 4505,
}

/// Error codes for server errors (4000-4499).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerErrorCode {
    Unknown = 4000,
}
