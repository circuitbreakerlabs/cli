use super::ResponseProvider;
use super::types::CompletionResponse;
use crate::protocol_types;
use async_trait::async_trait;
use reqwest::header;

#[derive(Clone)]
pub struct OllamaProvider {
    client: reqwest::Client,
    completion_endpoint: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(
        completion_endpoint: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let headers = header::HeaderMap::from_iter([(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        )]);

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self {
            client,
            completion_endpoint: completion_endpoint.into(),
            model: model.into(),
        })
    }
}

#[async_trait]
impl ResponseProvider for OllamaProvider {
    async fn generate_response(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<protocol_types::Message, Box<dyn std::error::Error>> {
        let request_body = serde_json::json!({
            "model": self.model,
            "messages": conversation_history,
        });

        let response = self
            .client
            .post(&self.completion_endpoint)
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
