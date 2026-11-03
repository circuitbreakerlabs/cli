use super::config::OllamaProviderConfig;
use crate::protocol_types;
use crate::response_provider::{
    ProviderApi, ProviderCompletion, ProviderError, ProviderRequest, ProviderResponse,
};
use async_trait::async_trait;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::generation::chat::{ChatMessage as OllamaMessage, MessageRole as OllamaMessageRole};

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
    url: String,
    config: OllamaProviderConfig,
}

impl OllamaProvider {
    pub fn new(config: OllamaProviderConfig) -> Result<Self, ProviderError> {
        let endpoint = url::Url::parse(&config.optional.base_url)
            .map_err(|e| ProviderError::Config(e.to_string()))?;
        let url = format!("{}api/chat", endpoint.as_str());
        Ok(Self { url, config })
    }

    fn convert_message(msg: &protocol_types::Message) -> OllamaMessage {
        let role = match msg.role {
            protocol_types::Role::User => OllamaMessageRole::User,
            protocol_types::Role::Assistant => OllamaMessageRole::Assistant,
            protocol_types::Role::System => OllamaMessageRole::System,
        };

        // The Ollama chat schema has no speaker name field.
        let content = msg.name.as_ref().map_or_else(
            || msg.content.clone(),
            |name| format!("{name}: {}", msg.content),
        );
        OllamaMessage::new(role, content)
    }
}

#[async_trait]
impl ProviderApi for OllamaProvider {
    async fn build_request(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<ProviderRequest, ProviderError> {
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

        ProviderRequest::json(self.url.clone(), &request)
    }

    async fn parse_response(
        &self,
        response: ProviderResponse,
    ) -> Result<ProviderCompletion, ProviderError> {
        let response: ollama_rs::generation::chat::ChatMessageResponse = response.decode_json()?;
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
                name: None,
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

#[cfg(test)]
mod tests {
    use super::OllamaProvider;
    use crate::protocol_types::{Message, Role};
    use crate::response_provider::ProviderError;
    use ollama_rs::generation::chat::{
        ChatMessageFinalResponseData, ChatMessageResponse, MessageRole as OllamaMessageRole,
    };

    use crate::response_provider::{ProviderApi, ProviderResponse};
    use clap::Parser;
    use serde_json::json;

    #[derive(Parser)]
    struct Arguments {
        #[command(flatten)]
        config: super::OllamaProviderConfig,
    }

    #[tokio::test]
    async fn requests_label_named_messages_and_preserve_unnamed_content() {
        let config = Arguments::try_parse_from(["test", "--model", "model"])
            .unwrap()
            .config;
        let provider = OllamaProvider::new(config).unwrap();

        for role in ["system", "user", "assistant"] {
            for name in [None, Some("Alex")] {
                let message: Message = serde_json::from_value(json!({
                    "role": role, "content": "hello", "name": name,
                }))
                .unwrap();
                let request = provider
                    .build_request(std::slice::from_ref(&message))
                    .await
                    .unwrap();
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                let expected = if name.is_some() {
                    "Alex: hello"
                } else {
                    "hello"
                };

                assert_eq!(body["messages"][0]["role"], role);
                assert_eq!(body["messages"][0]["content"], expected);
                assert_eq!(message.content, "hello");
            }
        }
    }

    #[tokio::test]
    async fn adapter_translates_requests_and_interprets_responses() {
        for (status, body) in [
            (429, "busy"),
            (
                200,
                r#"{"model":"model","created_at":"now","message":{"role":"assistant","content":"reply"},"done":true}"#,
            ),
            (503, "unavailable"),
        ] {
            let base_url = "http://example.test:11434";
            let config = Arguments::try_parse_from([
                "test",
                "--model",
                "model",
                "--base-url",
                base_url,
                "--temperature",
                "0.7",
            ])
            .unwrap()
            .config;
            let api = OllamaProvider::new(config).unwrap();
            let request = api
                .build_request(&[Message {
                    name: None,
                    role: Role::User,
                    content: "hello".into(),
                }])
                .await
                .unwrap();
            assert_eq!(request.method, reqwest::Method::POST);
            assert_eq!(request.url, "http://example.test:11434/api/chat");
            assert_eq!(request.headers["content-type"], "application/json");
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
            assert_eq!(request["stream"], false);
            assert!((request["options"]["temperature"].as_f64().unwrap() - 0.7).abs() < 0.0001);
        }
    }

    #[tokio::test]
    async fn parses_completion_metadata() {
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

        let config = Arguments::try_parse_from(["test", "--model", "model"])
            .unwrap()
            .config;
        let api = OllamaProvider::new(config).unwrap();
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
        assert_eq!(completion.model_id.as_deref(), Some("llama3.2:latest"));
        assert_eq!(completion.tokens_used, Some(42));
        assert_eq!(completion.finish_reason, None);
        assert_eq!(completion.provider_response_id, None);
    }

    #[test]
    fn converts_protocol_message_to_ollama_shape() {
        let message = Message {
            name: None,
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
