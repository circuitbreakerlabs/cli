pub mod custom;
pub mod err;
pub mod ollama;
pub mod openai;

use crate::protocol_types;
use async_trait::async_trait;
pub use custom::{CustomProvider, CustomProviderConfig};
pub use err::ProviderError;
pub use ollama::{OllamaProvider, OllamaProviderConfig};
pub use openai::{OpenAIProvider, OpenAIProviderConfig};

#[derive(Debug, Clone)]
pub struct ProviderCompletion {
    pub message: protocol_types::Message,
    pub model_id: Option<String>,
    pub tokens_used: Option<u64>,
    pub finish_reason: Option<String>,
    pub provider_response_id: Option<String>,
}

impl From<protocol_types::Message> for ProviderCompletion {
    fn from(message: protocol_types::Message) -> Self {
        Self {
            message,
            model_id: None,
            tokens_used: None,
            finish_reason: None,
            provider_response_id: None,
        }
    }
}

#[async_trait]
pub trait ResponseProvider: Send + Sync {
    async fn generate_response(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<ProviderCompletion, err::ProviderError>;
}
