use super::ResponseProvider;
use crate::protocol_types;
use async_trait::async_trait;
use reqwest::header::HeaderMap;

#[derive(Clone, Debug, clap::Args)]
pub struct AnthropicProviderConfig {
    /// Anthropic beta features
    #[arg(long)]
    beta: Option<String>,

    /// Anthropic parameter 1
    #[arg(long)]
    parameter_1: String,
}

#[derive(Clone, Debug)]
pub struct AnthropicProvider {
    config: AnthropicProviderConfig,
}

impl AnthropicProvider {
    pub fn new(
        config: AnthropicProviderConfig,
        _headers: &HeaderMap,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self { config })
    }
}

#[async_trait]
impl ResponseProvider for AnthropicProvider {
    async fn generate_response(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<protocol_types::Message, Box<dyn std::error::Error>> {
        unimplemented!("AnthropicProvider response generation is not implemented yet");
    }
}
