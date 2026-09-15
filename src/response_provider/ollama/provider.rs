use super::config::OllamaProviderConfig;
use crate::protocol_types;
use crate::response_provider::{ProviderCompletion, ProviderError, ResponseProvider};
use async_trait::async_trait;
use ollama_rs::Ollama;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::chat::{ChatMessage as OllamaMessage, MessageRole as OllamaMessageRole};
use reqwest::header::HeaderMap;

impl TryFrom<&OllamaMessageRole> for protocol_types::Role {
    type Error = ProviderError;

    fn try_from(role: &OllamaMessageRole) -> Result<Self, Self::Error> {
        match role {
            OllamaMessageRole::User => Ok(protocol_types::Role::User),
            OllamaMessageRole::Assistant => Ok(protocol_types::Role::Assistant),
            OllamaMessageRole::System => Ok(protocol_types::Role::System),
            OllamaMessageRole::Tool => Err(ProviderError::Parsing(
                "Tool messages from Ollama cannot be converted to protocol_types::Role".to_string(),
            )),
        }
    }
}

#[derive(Clone)]
pub struct OllamaProvider {
    client: Ollama,
    config: OllamaProviderConfig,
}

impl OllamaProvider {
    pub fn new(config: OllamaProviderConfig, headers: &HeaderMap) -> Result<Self, ProviderError> {
        let mut client = Ollama::try_new(&config.optional.base_url)
            .map_err(|e| ProviderError::Config(e.to_string()))?;
        client.set_headers(Some(headers.clone()));
        Ok(Self { client, config })
    }

    fn convert_message(msg: &protocol_types::Message) -> OllamaMessage {
        let role = match msg.role {
            protocol_types::Role::User => OllamaMessageRole::User,
            protocol_types::Role::Assistant => OllamaMessageRole::Assistant,
            protocol_types::Role::System => OllamaMessageRole::System,
        };

        OllamaMessage::new(role, msg.content.clone())
    }

    fn normalize_response(
        response: ollama_rs::generation::chat::ChatMessageResponse,
    ) -> Result<ProviderCompletion, ProviderError> {
        let tokens_used = response
            .final_data
            .map(|data| {
                data.prompt_eval_count
                    .checked_add(data.eval_count)
                    .ok_or_else(|| {
                        ProviderError::Parsing("Ollama token count overflowed".to_string())
                    })
            })
            .transpose()?;
        let role = protocol_types::Role::try_from(&response.message.role)?;

        Ok(ProviderCompletion {
            message: protocol_types::Message {
                role,
                content: response.message.content,
            },
            model_id: Some(response.model),
            tokens_used,
            finish_reason: None,
            provider_response_id: None,
        })
    }
}

#[async_trait]
impl ResponseProvider for OllamaProvider {
    async fn generate_response(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<ProviderCompletion, ProviderError> {
        let messages: Vec<OllamaMessage> = conversation_history
            .iter()
            .map(Self::convert_message)
            .collect();

        let mut request = ChatMessageRequest::new(self.config.required.model.clone(), messages);

        if let Some(options) = self.config.build_model_options() {
            request = request.options(options);
        }

        if let Some(logprobs) = self.config.optional.logprobs {
            request = request.logprobs(logprobs);
        }

        let response = self
            .client
            .send_chat_messages(request)
            .await
            .map_err(|e| ProviderError::Api(e.to_string()))?;

        Self::normalize_response(response)
    }
}

#[cfg(test)]
mod tests {
    use super::OllamaProvider;
    use crate::protocol_types::{Message, Role};
    use crate::response_provider::ProviderError;
    use ollama_rs::generation::chat::{
        ChatMessageFinalResponseData, ChatMessageResponse, MessageRole as OllamaMessageRole,
    };

    #[test]
    fn normalizes_completion_metadata() {
        let response = ChatMessageResponse {
            model: "llama3.2:latest".to_string(),
            created_at: "2026-09-07T12:00:00Z".to_string(),
            message: ollama_rs::generation::chat::ChatMessage::new(
                OllamaMessageRole::Assistant,
                "safe reply".to_string(),
            ),
            logprobs: None,
            done: true,
            final_data: Some(ChatMessageFinalResponseData {
                total_duration: 1,
                load_duration: 1,
                prompt_eval_count: 30,
                prompt_eval_duration: 1,
                eval_count: 12,
                eval_duration: 1,
            }),
        };

        let completion =
            OllamaProvider::normalize_response(response).expect("response should normalize");

        assert!(matches!(completion.message.role, Role::Assistant));
        assert_eq!(completion.message.content, "safe reply");
        assert_eq!(completion.model_id.as_deref(), Some("llama3.2:latest"));
        assert_eq!(completion.tokens_used, Some(42));
        assert_eq!(completion.finish_reason, None);
        assert_eq!(completion.provider_response_id, None);
    }

    #[test]
    fn converts_protocol_message_to_ollama_shape() {
        let message = Message {
            role: Role::User,
            content: "hello".to_string(),
        };

        let converted = OllamaProvider::convert_message(&message);

        assert!(matches!(converted.role, OllamaMessageRole::User));
        assert_eq!(converted.content, "hello");
    }

    #[test]
    fn converts_ollama_roles_back_to_protocol_roles() {
        let user = Role::try_from(&OllamaMessageRole::User).expect("user role should convert");
        let assistant =
            Role::try_from(&OllamaMessageRole::Assistant).expect("assistant should convert");
        let system = Role::try_from(&OllamaMessageRole::System).expect("system should convert");

        assert!(matches!(user, Role::User));
        assert!(matches!(assistant, Role::Assistant));
        assert!(matches!(system, Role::System));
    }

    #[test]
    fn rejects_ollama_tool_role() {
        let err = Role::try_from(&OllamaMessageRole::Tool)
            .expect_err("tool role should not convert to protocol role");

        assert!(matches!(err, ProviderError::Parsing(_)));
        assert!(err.to_string().contains("Tool messages from Ollama"));
    }
}
