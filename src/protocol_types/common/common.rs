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

#[cfg(test)]
mod tests {
    use super::{
        CompletionRequestEnvelope, CompletionResponse, CompletionResponseEnvelope, TestCaseGroup,
    };
    use serde_json::json;
    use std::str::FromStr;

    #[test]
    fn completion_response_envelope_serializes_to_protocol_shape() {
        let envelope = CompletionResponseEnvelope::from(CompletionResponse {
            request_id: "req-123".to_string(),
            model_response: "safe reply".to_string(),
        });

        let value = serde_json::to_value(envelope).expect("envelope should serialize");

        assert_eq!(
            value,
            json!({
                "type": "completion_response",
                "data": {
                    "request_id": "req-123",
                    "model_response": "safe reply"
                }
            })
        );
    }

    #[test]
    fn completion_request_envelope_deserializes_from_protocol_shape() {
        let value = json!({
            "type": "completion_request",
            "data": {
                "request_id": "req-789",
                "conversation_id": 12,
                "messages": [
                    { "role": "system", "content": "be safe" },
                    { "role": "user", "content": "hello" }
                ]
            }
        });

        let envelope: CompletionRequestEnvelope =
            serde_json::from_value(value).expect("envelope should deserialize");

        assert_eq!(envelope.message_type, "completion_request");
        assert_eq!(envelope.data.request_id, "req-789");
        assert_eq!(envelope.data.conversation_id, 12);
        assert_eq!(envelope.data.messages.len(), 2);
        assert_eq!(envelope.data.messages[0].content, "be safe");
    }

    #[test]
    fn test_case_group_from_str_supports_known_and_custom_values() {
        let known = TestCaseGroup::from_str("suicidal_ideation").expect("known group should parse");
        let custom = TestCaseGroup::from_str("custom_group").expect("custom group should parse");

        assert!(matches!(known, TestCaseGroup::SuicidalIdeation));
        assert!(matches!(custom, TestCaseGroup::CustomGroup(ref value) if value == "custom_group"));
    }
}
