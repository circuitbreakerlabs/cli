use super::config::OpenAIProviderConfig;
use crate::protocol_types;
use crate::response_provider::ResponseProvider;
use async_openai::Client;
use async_openai::types::chat::{
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent,
    ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
};
use async_trait::async_trait;

#[derive(Clone)]
pub struct OpenAIProvider {
    client: Client<async_openai::config::OpenAIConfig>,
    config: OpenAIProviderConfig,
}

impl OpenAIProvider {
    pub fn new(config: OpenAIProviderConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let openai_config = config.build_openai_config();
        let client = Client::with_config(openai_config);

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
    ) -> Result<protocol_types::Message, Box<dyn std::error::Error>> {
        let messages: Vec<ChatCompletionRequestMessage> = conversation_history
            .iter()
            .map(Self::convert_message)
            .collect();

        let request = self.config.build_request(messages);

        let response = self.client.chat().create(request).await?;

        let choice = response
            .choices
            .first()
            .ok_or("No response received from OpenAI")?;

        let content = choice.message.content.clone().unwrap_or_default();

        Ok(protocol_types::Message {
            role: protocol_types::Role::Assistant,
            content,
        })
    }
}
