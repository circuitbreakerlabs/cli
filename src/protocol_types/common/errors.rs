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

/// Error codes for completion errors (4500-4599).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompletionErrorCode {
    Unknown = 4500,
    ModelTimeout = 4501,
    ModelUnreachable = 4502,
    InvalidResponse = 4503,
    RateLimited = 4504,
    AuthenticationFailure = 4505,
}
