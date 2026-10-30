use super::config::OpenAIProviderConfig;
use crate::protocol_types;
use crate::response_provider::{
    ProviderApi, ProviderCompletion, ProviderError, ProviderRequest, ProviderResponse,
};
use async_openai::config::Config;
use async_openai::types::chat::{
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent,
    ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
    CreateChatCompletionResponse, FinishReason,
};
use async_trait::async_trait;

#[derive(Clone)]
pub struct OpenAIProvider {
    api_config: async_openai::config::OpenAIConfig,
    config: OpenAIProviderConfig,
}

impl OpenAIProvider {
    pub fn new(config: OpenAIProviderConfig) -> Self {
        let openai_config = config.build_openai_config();

        Self {
            api_config: openai_config,
            config,
        }
    }

    fn convert_message(msg: &protocol_types::Message) -> ChatCompletionRequestMessage {
        match msg.role {
            protocol_types::Role::System => {
                ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
                    content: ChatCompletionRequestSystemMessageContent::Text(msg.content.clone()),
                    name: msg.name.clone(),
                })
            }
            protocol_types::Role::User => {
                ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                    content: ChatCompletionRequestUserMessageContent::Text(msg.content.clone()),
                    name: msg.name.clone(),
                })
            }
            protocol_types::Role::Assistant => {
                ChatCompletionRequestMessage::Assistant(ChatCompletionRequestAssistantMessage {
                    content: Some(async_openai::types::chat::ChatCompletionRequestAssistantMessageContent::Text(
                        msg.content.clone(),
                    )),
                    name: msg.name.clone(),
                    tool_calls: None,
                    refusal: None,
                    audio: None,
                    ..Default::default()
                })
            }
        }
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
impl ProviderApi for OpenAIProvider {
    async fn build_request(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<ProviderRequest, ProviderError> {
        let messages: Vec<ChatCompletionRequestMessage> = conversation_history
            .iter()
            .map(Self::convert_message)
            .collect();

        let request = self.config.build_request(messages);

        let mut url = reqwest::Url::parse(&self.api_config.url("/chat/completions"))
            .map_err(|error| ProviderError::Config(error.to_string()))?;
        let query = self.api_config.query();
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        let mut request = ProviderRequest::json(url.to_string(), &request)?;
        request.headers.extend(self.api_config.headers());
        Ok(request)
    }

    async fn parse_response(
        &self,
        response: ProviderResponse,
    ) -> Result<ProviderCompletion, ProviderError> {
        let response: CreateChatCompletionResponse = response.decode_json()?;
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
                name: None,
                role: protocol_types::Role::Assistant,
                content,
            },
            model_id: Some(response.model),
            tokens_used: response.usage.map(|usage| u64::from(usage.total_tokens)),
            finish_reason: choice.finish_reason.map(Self::finish_reason_name),
            provider_response_id: Some(response.id),
        })
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

    use crate::response_provider::{ProviderApi, ProviderResponse};
    use clap::Parser;

    #[derive(Parser)]
    struct Arguments {
        #[command(flatten)]
        config: super::OpenAIProviderConfig,
    }

    #[tokio::test]
    async fn requests_preserve_optional_speaker_names_and_message_roles() {
        let config =
            Arguments::try_parse_from(["test", "--model", "model", "--api-key", "test-key"])
                .unwrap()
                .config;
        let provider = OpenAIProvider::new(config);

        for role in ["system", "user", "assistant"] {
            for name in [None, Some("Alex")] {
                let message: Message = serde_json::from_value(json!({
                    "role": role, "content": "hello", "name": name,
                }))
                .unwrap();
                let request = provider.build_request(&[message]).await.unwrap();
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                let sent = &body["messages"][0];

                assert_eq!(sent["role"], role);
                assert_eq!(sent["content"], "hello");
                assert_eq!(sent.get("name"), name.map(|name| json!(name)).as_ref());
            }
        }
    }

    #[tokio::test]
    async fn adapter_translates_requests_and_interprets_responses() {
        for (status, body) in [
            (429, "busy"),
            (
                200,
                r#"{"id":"reply-id","object":"chat.completion","created":1,"model":"model","choices":[{"index":0,"message":{"role":"assistant","content":"reply"},"finish_reason":"stop"}]}"#,
            ),
            (503, "unavailable"),
        ] {
            let base_url = "https://example.test/v1";
            let config = Arguments::try_parse_from([
                "test",
                "--model",
                "model",
                "--base-url",
                base_url,
                "--api-key",
                "test-key",
                "--org-id",
                "test-org",
                "--temperature",
                "0.7",
            ])
            .unwrap()
            .config;
            let api = OpenAIProvider::new(config);
            let request = api
                .build_request(&[Message {
                    name: None,
                    role: Role::User,
                    content: "hello".into(),
                }])
                .await
                .unwrap();
            assert_eq!(request.method, reqwest::Method::POST);
            assert_eq!(request.url, "https://example.test/v1/chat/completions");
            assert_eq!(request.headers["content-type"], "application/json");
            assert_eq!(request.headers["authorization"], "Bearer test-key");
            assert_eq!(request.headers["openai-organization"], "test-org");
            let request: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            let result = api
                .parse_response(ProviderResponse {
                    status: reqwest::StatusCode::from_u16(status).unwrap(),
                    headers: reqwest::header::HeaderMap::new(),
                    body: body.as_bytes().to_vec(),
                    url: base_url.into(),
                    elapsed: std::time::Duration::ZERO,
                })
                .await;
            if status == 429 {
                let error = result.unwrap_err();
                assert!(matches!(
                    error,
                    crate::response_provider::ProviderError::RateLimited { .. }
                ));
                assert_eq!(error.retry_after(), None);
            } else if status == 200 {
                assert_eq!(result.unwrap().message.content, "reply");
            } else {
                assert!(matches!(
                    result,
                    Err(crate::response_provider::ProviderError::Api(_))
                ));
            }
            assert_eq!(request["model"], "model");
            assert_eq!(request["messages"][0]["content"], "hello");
            assert!((request["temperature"].as_f64().unwrap() - 0.7).abs() < 0.0001);
        }
    }

    #[tokio::test]
    async fn parses_completion_metadata() {
        let response = json!({
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
        });

        let config =
            Arguments::try_parse_from(["test", "--model", "model", "--api-key", "test-key"])
                .unwrap()
                .config;
        let api = OpenAIProvider::new(config);
        let completion = api
            .parse_response(ProviderResponse {
                status: reqwest::StatusCode::OK,
                headers: reqwest::header::HeaderMap::new(),
                body: serde_json::to_vec(&response).unwrap(),
                url: "https://example.test/completion".into(),
                elapsed: std::time::Duration::ZERO,
            })
            .await
            .expect("response should parse");

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
            name: None,
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
            name: None,
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
            name: None,
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
