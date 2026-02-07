use serde::{Deserialize, Serialize};

/// Test case group identifier (e.g., "suicidal_ideation")
pub type TestCaseGroup = String;

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

/// Payload for completion_request messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequestObject {
    /// Unique identifier for this completion request (UUID recommended)
    pub request_id: String,
    /// Identifier for the conversation thread this request belongs to
    pub conversation_id: i32,
    /// Conversation history in standard role/content format
    pub messages: Vec<Message>,
}

/// Server requests client to obtain a completion from the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Request payload
    pub object: CompletionRequestObject,
}

/// Payload for completion_response messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponseObject {
    /// Must match the ID from the corresponding CompletionRequest
    pub request_id: String,
    /// The model's generated response
    pub model_response: String,
}

/// Client returns the model's completion for a requested conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Response payload
    pub object: CompletionResponseObject,
}
