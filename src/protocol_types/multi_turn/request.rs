use super::super::common::{TestCaseGroup, parse_test_case_group};
use serde::{Deserialize, Serialize};

fn parse_threshold(value: &str) -> Result<f32, String> {
    let threshold: f32 = value
        .parse()
        .map_err(|_| format!("invalid threshold '{value}': expected a number between 0 and 1"))?;
    if (0.0..=1.0).contains(&threshold) {
        Ok(threshold)
    } else {
        Err(format!(
            "invalid threshold '{value}': expected a number between 0 and 1",
        ))
    }
}

fn parse_even_turn_count(value: &str) -> Result<usize, String> {
    let turns: usize = value.parse().map_err(|_| {
        format!("invalid max_turns '{value}': expected an even integer between 2 and 20")
    })?;
    if (2..=20).contains(&turns) && turns.is_multiple_of(2) {
        Ok(turns)
    } else {
        Err(format!(
            "invalid max_turns '{value}': expected an even integer between 2 and 20",
        ))
    }
}

/// Payload for `MultiTurnRequestEnvelope` messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct MultiTurnRequest {
    /// Responses below this safety score threshold will fail
    #[arg(short, long, value_parser = parse_threshold, allow_hyphen_values = true)]
    pub threshold: f32,
    /// Maximum number of turns in the conversation. Should be a multiple of two.
    #[arg(
        short,
        long,
        value_parser = parse_even_turn_count,
        allow_hyphen_values = true
    )]
    pub max_turns: usize,
    /// One or more comma-separated test case groups to run.
    #[arg(
        long,
        value_delimiter = ',',
        value_parser = parse_test_case_group,
        required = true
    )]
    pub test_case_groups: Vec<TestCaseGroup>,
}

/// Client initiates a multi-turn conversational evaluation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnRequestEnvelope {
    /// Protocol version
    pub version: String,
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Request payload
    pub data: MultiTurnRequest,
}

impl From<MultiTurnRequest> for MultiTurnRequestEnvelope {
    fn from(data: MultiTurnRequest) -> Self {
        MultiTurnRequestEnvelope {
            version: crate::consts::version::PROTOCOL_VERSION.to_string(),
            message_type: "multi_turn_request".to_string(),
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MultiTurnRequest, MultiTurnRequestEnvelope};
    use serde_json::json;

    #[test]
    fn multi_turn_request_envelope_serializes_to_protocol_shape() {
        let envelope = MultiTurnRequestEnvelope::from(MultiTurnRequest {
            threshold: 0.5,
            max_turns: 6,
            test_case_groups: vec!["suicidal_ideation".to_string(), "custom_group".to_string()],
        });

        let value = serde_json::to_value(envelope).expect("envelope should serialize");

        assert_eq!(
            value,
            json!({
                "version": crate::consts::version::PROTOCOL_VERSION,
                "type": "multi_turn_request",
                "data": {
                    "threshold": 0.5,
                    "max_turns": 6,
                    "test_case_groups": [
                        "suicidal_ideation",
                        "custom_group"
                    ]
                }
            })
        );
    }
}
