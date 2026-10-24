#[cfg(feature = "custom")]
pub mod custom;
pub mod err;
mod http;
#[cfg(any(text_targets, test))]
pub use http::HttpResponseProvider;
#[cfg(feature = "ollama")]
pub mod ollama;
#[cfg(feature = "openai")]
pub mod openai;
mod retry;

use crate::protocol_types;
use async_trait::async_trait;
#[cfg(feature = "custom")]
pub use custom::{CustomProvider, CustomProviderConfig};
pub use err::ProviderError;
#[cfg(feature = "ollama")]
pub use ollama::{OllamaProvider, OllamaProviderConfig};
#[cfg(feature = "openai")]
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

/// A complete model interaction, including any provider retry policy.
/// Evaluation code only depends on this interface, not on HTTP details.
#[async_trait]
pub trait ResponseProvider: Send + Sync {
    async fn generate_response(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<ProviderCompletion, err::ProviderError>;
}

#[cfg(test)]
pub(crate) mod test_support;

/// Provider-specific translation, without network execution or retry policy.
#[async_trait]
pub trait ProviderApi: Send + Sync {
    /// Translate the same history afresh for each attempt; do not send the request.
    async fn build_request(
        &self,
        messages: &[protocol_types::Message],
    ) -> Result<ProviderRequest, ProviderError>;

    /// Interpret both successful and unsuccessful responses, including retry hints.
    async fn parse_response(
        &self,
        response: ProviderResponse,
    ) -> Result<ProviderCompletion, ProviderError>;
}

/// A fully translated request. Explicit headers override client defaults.
pub struct ProviderRequest {
    pub method: reqwest::Method,
    pub url: String,
    pub headers: reqwest::header::HeaderMap,
    pub body: Vec<u8>,
}

impl ProviderRequest {
    pub fn json(url: String, body: &impl serde::Serialize) -> Result<Self, ProviderError> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().expect("valid content type"),
        );
        Ok(Self {
            method: reqwest::Method::POST,
            url,
            headers,
            body: serde_json::to_vec(body)?,
        })
    }
}

/// The buffered response for one attempt, before provider-specific interpretation.
#[cfg_attr(not(feature = "custom"), allow(dead_code))]
pub struct ProviderResponse {
    pub status: reqwest::StatusCode,
    pub headers: reqwest::header::HeaderMap,
    pub body: Vec<u8>,
    pub url: String,
    pub elapsed: std::time::Duration,
}

impl ProviderResponse {
    /// Standard HTTP error interpretation used by the built-in JSON APIs.
    #[cfg(any(feature = "openai", feature = "ollama", test))]
    pub fn decode_json<T: serde::de::DeserializeOwned>(&self) -> Result<T, ProviderError> {
        if self.status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited {
                message: String::from_utf8_lossy(&self.body).into_owned(),
                retry_after: None,
            });
        }
        if !self.status.is_success() {
            return Err(ProviderError::Api(
                String::from_utf8_lossy(&self.body).into_owned(),
            ));
        }
        Ok(serde_json::from_slice(&self.body)?)
    }
}
