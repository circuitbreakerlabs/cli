pub mod anthropic;
pub mod ollama;
pub mod openai;
mod types;

use crate::protocol_types;
pub use anthropic::{AnthropicProvider, AnthropicProviderConfig};
use async_trait::async_trait;
pub use ollama::{OllamaProvider, OllamaProviderConfig};
pub use openai::{OpenAIProvider, OpenAIProviderConfig};

#[async_trait]
pub trait ResponseProvider: Send + Sync {
    async fn generate_response(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<protocol_types::Message, Box<dyn std::error::Error>>;
}
