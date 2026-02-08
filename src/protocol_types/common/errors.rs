use serde::{Deserialize, Serialize};

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

/// Error codes for server errors (4000-4499).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerErrorCode {
    Unknown = 4000,
}
