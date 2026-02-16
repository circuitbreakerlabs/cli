use super::ResponseProvider;
use crate::protocol_types;
use async_trait::async_trait;

#[derive(Clone, Debug, clap::Args)]
pub struct OpenAIProviderConfig {
    /// Completion endpoint URL
    #[arg(long, env = "COMPLETION_ENDPOINT")]
    endpoint: String,

    /// OpenAI organization ID
    #[arg(long, env = "OPENAI_ORG_ID")]
    org_id: Option<String>,

    /// OpenAI parameter 1
    #[arg(long)]
    parameter_1: String,
}

#[derive(Clone, Debug)]
pub struct OpenAIProvider {
    config: OpenAIProviderConfig,
}

impl OpenAIProvider {
    pub fn new(config: OpenAIProviderConfig) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self { config })
    }
}
#[async_trait]
impl ResponseProvider for OpenAIProvider {
    async fn generate_response(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<protocol_types::Message, Box<dyn std::error::Error>> {
        unimplemented!("OpenAI response generation is not implemented yet");
    }
}
