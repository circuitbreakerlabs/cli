pub mod ollama;
mod types;

use crate::protocol_types;
use async_trait::async_trait;

#[async_trait]
pub trait ResponseProvider: Send + Sync + Clone {
    async fn generate_response(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<protocol_types::Message, Box<dyn std::error::Error>>;
}
