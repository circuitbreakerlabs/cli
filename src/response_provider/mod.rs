pub mod custom;
pub mod ollama;
pub mod openai;

use crate::protocol_types;
use async_trait::async_trait;
pub use custom::{CustomProvider, CustomProviderConfig};
pub use ollama::{OllamaProvider, OllamaProviderConfig};
pub use openai::{OpenAIProvider, OpenAIProviderConfig};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("API error: {0}")]
    Api(String),

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

#[async_trait]
pub trait ResponseProvider: Send + Sync {
    async fn generate_response(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<protocol_types::Message, ProviderError>;
}
