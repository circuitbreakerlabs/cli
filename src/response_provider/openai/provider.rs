use super::config::OpenAIProviderConfig;
use crate::protocol_types;
use crate::response_provider::{ProviderError, ResponseProvider};
use async_openai::types::chat::{
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent,
    ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
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
}

#[async_trait]
impl ResponseProvider for OpenAIProvider {
    async fn generate_response(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<protocol_types::Message, ProviderError> {
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

        let err_no_resp = "No response received from OpenAI";
        let content = response
            .choices
            .into_iter()
            .next()
            .ok_or(ProviderError::Api(err_no_resp.to_string()))?
            .message
            .content
            .ok_or(ProviderError::Api(err_no_resp.to_string()))?;

        Ok(protocol_types::Message {
            role: protocol_types::Role::Assistant,
            content,
        })
    }
}
