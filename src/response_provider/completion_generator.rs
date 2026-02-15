use super::types::CompletionResponse;
use crate::protocol_types::Message;
use reqwest::header;

#[derive(Clone)]
pub struct CompletionGenerator {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

impl CompletionGenerator {
    pub fn new(
        base_url: impl Into<String>,
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
            base_url: base_url.into(),
            model: model.into(),
        })
    }

    pub async fn generate_completions(
        &self,
        conversation_history: &[Message],
    ) -> Result<Message, Box<dyn std::error::Error>> {
        let request_body = serde_json::json!({
            "model": self.model,
            "messages": conversation_history,
        });

        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
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
