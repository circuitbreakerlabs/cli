use thiserror::Error;

// Providers construct different subsets of the shared error vocabulary.
#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("Provider reported {reason:?}: {message}")]
    Reported {
        reason: crate::protocol_types::common::CompletionErrorReason,
        message: String,
    },

    #[error("Rate limited: {message}")]
    RateLimited {
        message: String,
        retry_after: Option<std::time::Duration>,
    },

    #[error("Parsing error: {0}")]
    Parsing(String),

    #[error("Script execution error: {0}")]
    Script(String),
}

impl From<reqwest::Error> for ProviderError {
    fn from(e: reqwest::Error) -> Self {
        ProviderError::Network(e.to_string())
    }
}

#[cfg(feature = "custom")]
impl From<rhai::EvalAltResult> for ProviderError {
    fn from(e: rhai::EvalAltResult) -> Self {
        ProviderError::Script(e.to_string())
    }
}

impl From<serde_json::Error> for ProviderError {
    fn from(e: serde_json::Error) -> Self {
        ProviderError::Parsing(e.to_string())
    }
}

impl ProviderError {
    /// Attach a fallback delay without replacing an explicit provider delay.
    pub fn with_retry_after(self, retry_after: Option<std::time::Duration>) -> Self {
        if let Self::RateLimited {
            message,
            retry_after: existing,
        } = self
        {
            return Self::RateLimited {
                message,
                retry_after: existing.or(retry_after),
            };
        }
        if retry_after.is_some()
            && matches!(
                crate::protocol_types::common::CompletionErrorReason::from(&self),
                crate::protocol_types::common::CompletionErrorReason::RateLimited
            )
        {
            Self::RateLimited {
                message: self.to_string(),
                retry_after,
            }
        } else {
            self
        }
    }

    pub fn retry_after(&self) -> Option<std::time::Duration> {
        match self {
            Self::RateLimited { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}
