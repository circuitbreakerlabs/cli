use serde::{Deserialize, Serialize};

pub type ConversationId = i32;

/// Test case group identifier
pub type TestCaseGroup = String;

pub fn parse_test_case_group(value: &str) -> Result<TestCaseGroup, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(
            "invalid test_case_groups: expected a non-empty test case group name".to_string(),
        );
    }

    Ok(value.to_string())
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
        parse_test_case_group,
    };
    use serde_json::json;

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
    fn test_case_group_is_a_string_identifier() {
        let group: TestCaseGroup = "suicidal_ideation".to_string();

        assert_eq!(group, "suicidal_ideation");
    }

    #[test]
    fn parse_test_case_group_trims_and_rejects_empty_values() {
        let known = parse_test_case_group(" suicidal_ideation ")
            .expect("known group with whitespace should parse");
        let custom = parse_test_case_group(" custom_group ")
            .expect("custom group with whitespace should parse");

        assert_eq!(known, "suicidal_ideation");
        assert_eq!(custom, "custom_group");
        assert!(parse_test_case_group("  ").is_err());
    }
}
