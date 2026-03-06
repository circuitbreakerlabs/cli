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

#[async_trait]
pub trait ResponseProvider: Send + Sync {
    async fn generate_response(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<protocol_types::Message, err::ProviderError>;
}
