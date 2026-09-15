use super::config::OpenAIProviderConfig;
use crate::protocol_types;
use crate::response_provider::{ProviderCompletion, ProviderError, ResponseProvider};
use async_openai::types::chat::{
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent,
    ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
    CreateChatCompletionResponse, FinishReason,
};
use async_trait::async_trait;
use reqwest::header::HeaderMap;

#[derive(Clone)]
pub struct OpenAIProvider {
    client: async_openai::Client<async_openai::config::OpenAIConfig>,
    config: OpenAIProviderConfig,
}

impl OpenAIProvider {
    pub fn new(config: OpenAIProviderConfig, headers: &HeaderMap) -> Result<Self, ProviderError> {
        let openai_config = config.build_openai_config();

        let http_client = reqwest::Client::builder()
            .default_headers(headers.clone())
            .build()?;

        let client = async_openai::Client::with_config(openai_config).with_http_client(http_client);

        Ok(Self { client, config })
    }

    fn convert_message(msg: &protocol_types::Message) -> ChatCompletionRequestMessage {
        match msg.role {
            protocol_types::Role::System => {
                ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
                    content: ChatCompletionRequestSystemMessageContent::Text(msg.content.clone()),
                    name: None,
                })
            }
            protocol_types::Role::User => {
                ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                    content: ChatCompletionRequestUserMessageContent::Text(msg.content.clone()),
                    name: None,
                })
            }
            protocol_types::Role::Assistant => {
                ChatCompletionRequestMessage::Assistant(ChatCompletionRequestAssistantMessage {
                    content: Some(async_openai::types::chat::ChatCompletionRequestAssistantMessageContent::Text(
                        msg.content.clone(),
                    )),
                    name: None,
                    tool_calls: None,
                    refusal: None,
                    audio: None,
                    ..Default::default()
                })
            }
        }
    }

    fn normalize_response(
        response: CreateChatCompletionResponse,
    ) -> Result<ProviderCompletion, ProviderError> {
        let err_no_resp = "No response received from OpenAI";
        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or(ProviderError::Api(err_no_resp.to_string()))?;
        let content = choice
            .message
            .content
            .ok_or(ProviderError::Api(err_no_resp.to_string()))?;

        Ok(ProviderCompletion {
            message: protocol_types::Message {
                role: protocol_types::Role::Assistant,
                content,
            },
            model_id: Some(response.model),
            tokens_used: response.usage.map(|usage| u64::from(usage.total_tokens)),
            finish_reason: choice.finish_reason.map(Self::finish_reason_name),
            provider_response_id: Some(response.id),
        })
    }

    fn finish_reason_name(reason: FinishReason) -> String {
        match reason {
            FinishReason::Stop => "stop",
            FinishReason::Length => "length",
            FinishReason::ToolCalls => "tool_calls",
            FinishReason::ContentFilter => "content_filter",
            FinishReason::FunctionCall => "function_call",
        }
        .to_string()
    }
}

#[async_trait]
impl ResponseProvider for OpenAIProvider {
    async fn generate_response(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<ProviderCompletion, ProviderError> {
        let messages: Vec<ChatCompletionRequestMessage> = conversation_history
            .iter()
            .map(Self::convert_message)
            .collect();

        let request = self.config.build_request(messages);

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(|e| ProviderError::Api(e.to_string()))?;

        Self::normalize_response(response)
    }
}

#[cfg(test)]
mod tests {
    use super::OpenAIProvider;
    use crate::protocol_types::{Message, Role};
    use async_openai::types::chat::{
        ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessageContent,
    };
    use serde_json::json;

    #[test]
    fn normalizes_completion_metadata() {
        let response = serde_json::from_value(json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1_720_000_000,
            "model": "gpt-4o-2024-08-06",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "safe reply",
                    "refusal": null
                },
                "finish_reason": "stop",
                "logprobs": null
            }],
            "usage": {
                "prompt_tokens": 30,
                "completion_tokens": 12,
                "total_tokens": 42
            }
        }))
        .expect("OpenAI response fixture should deserialize");

        let completion =
            OpenAIProvider::normalize_response(response).expect("response should normalize");

        assert!(matches!(completion.message.role, Role::Assistant));
        assert_eq!(completion.message.content, "safe reply");
        assert_eq!(completion.model_id.as_deref(), Some("gpt-4o-2024-08-06"));
        assert_eq!(completion.tokens_used, Some(42));
        assert_eq!(completion.finish_reason.as_deref(), Some("stop"));
        assert_eq!(
            completion.provider_response_id.as_deref(),
            Some("chatcmpl-123")
        );
    }

    #[test]
    fn converts_system_message_to_openai_shape() {
        let message = Message {
            role: Role::System,
            content: "system prompt".to_string(),
        };

        let converted = OpenAIProvider::convert_message(&message);

        match converted {
            ChatCompletionRequestMessage::System(system) => {
                assert!(matches!(
                    system.content,
                    ChatCompletionRequestSystemMessageContent::Text(ref content)
                        if content == "system prompt"
                ));
            }
            other => panic!("expected system message, got {other:?}"),
        }
    }

    #[test]
    fn converts_user_message_to_openai_shape() {
        let message = Message {
            role: Role::User,
            content: "hello".to_string(),
        };

        let converted = OpenAIProvider::convert_message(&message);

        match converted {
            ChatCompletionRequestMessage::User(user) => {
                assert!(matches!(
                    user.content,
                    ChatCompletionRequestUserMessageContent::Text(ref content)
                        if content == "hello"
                ));
            }
            other => panic!("expected user message, got {other:?}"),
        }
    }

    #[test]
    fn converts_assistant_message_to_openai_shape() {
        let message = Message {
            role: Role::Assistant,
            content: "assistant reply".to_string(),
        };

        let converted = OpenAIProvider::convert_message(&message);

        match converted {
            ChatCompletionRequestMessage::Assistant(assistant) => {
                assert!(matches!(
                    assistant.content,
                    Some(ChatCompletionRequestAssistantMessageContent::Text(ref content))
                        if content == "assistant reply"
                ));
                assert!(assistant.tool_calls.is_none());
            }
            other => panic!("expected assistant message, got {other:?}"),
        }
    }
}
