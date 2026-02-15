use super::ResponseProvider;
use super::types::CompletionResponse;
use crate::protocol_types;
use async_trait::async_trait;
use ollama_rs;
use reqwest::header;

#[derive(Clone, Debug, clap::Args)]
pub struct OllamaProviderConfig {
    /// Ollama Base URL
    #[arg(
        long,
        env = "OLLAMA_BASE_URL",
        default_value = "http://localhost:11434/v1"
    )]
    base_url: String,

    /// Ollama model name
    #[arg(long)]
    model: String,
}

#[derive(Clone)]
pub struct OllamaProvider {
    client: reqwest::Client,
    config: OllamaProviderConfig,
}

impl OllamaProvider {
    pub fn new(config: OllamaProviderConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let headers = header::HeaderMap::from_iter([(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        )]);

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self { client, config })
    }
}

#[async_trait]
impl ResponseProvider for OllamaProvider {
    async fn generate_response(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<protocol_types::Message, Box<dyn std::error::Error>> {
        let request_body = serde_json::json!({
            "model": self.config.model,
            "messages": conversation_history,
        });

        let response = self
            .client
            .post(format!("{}/chat/completions", &self.config.base_url))
            .json(&request_body)
            .send()
            .await?
            .json::<CompletionResponse>()
            .await?;

        if let Some(choice) = response.choices.first() {
            Ok(choice.message.clone())
        } else {
            Err("No completion choices returned".into())
        }
    }
}
