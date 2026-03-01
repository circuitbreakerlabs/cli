use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub type ConversationId = i32;

/// Test case group identifier
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestCaseGroup {
    SuicidalIdeation,
    #[serde(untagged)]
    CustomGroup(String),
}

impl FromStr for TestCaseGroup {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "suicidal_ideation" => Ok(TestCaseGroup::SuicidalIdeation),
            custom => Ok(TestCaseGroup::CustomGroup(custom.to_string())),
        }
    }
}

/// Role of a message participant in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

/// A chat message with role and content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// Payload for `CompletionRequestEnvelope` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Unique identifier for this completion request (UUID recommended)
    pub request_id: String,
    /// Identifier for the conversation thread this request belongs to
    pub conversation_id: ConversationId,
    /// Conversation history in standard role/content format
    pub messages: Vec<Message>,
}

/// Server requests client to obtain a completion from the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequestEnvelope {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Request payload
    pub data: CompletionRequest,
}

/// Payload for `CompletionResponseEnvelope` messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Must match the ID from the corresponding `CompletionRequest`
    pub request_id: String,
    /// The model's generated response
    pub model_response: String,
}

/// Client returns the model's completion for a requested conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponseEnvelope {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Response payload
    pub data: CompletionResponse,
}

impl From<CompletionResponse> for CompletionResponseEnvelope {
    fn from(response: CompletionResponse) -> Self {
        CompletionResponseEnvelope {
            message_type: "completion_response".to_string(),
            data: response,
        }
    }
}
