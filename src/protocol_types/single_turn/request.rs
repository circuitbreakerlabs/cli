use serde::{Deserialize, Serialize};

use super::super::common::{TestCaseGroup, parse_test_case_group};

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

fn parse_variations(value: &str) -> Result<i32, String> {
    let parsed: i32 = value.parse().map_err(|_| {
        format!("invalid variations '{value}': expected an integer between 1 and 5")
    })?;
    if (1..=5).contains(&parsed) {
        Ok(parsed)
    } else {
        Err(format!(
            "invalid variations '{value}': expected an integer between 1 and 5",
        ))
    }
}

fn parse_maximum_iteration_layers(value: &str) -> Result<i32, String> {
    let parsed: i32 = value.parse().map_err(|_| {
        format!("invalid maximum_iteration_layers '{value}': expected an integer between 0 and 2")
    })?;
    if (0..=2).contains(&parsed) {
        Ok(parsed)
    } else {
        Err(format!(
            "invalid maximum_iteration_layers '{value}': expected an integer between 0 and 2",
        ))
    }
}

/// Payload for `SingleTurnRequestEnvelope` messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct SingleTurnRequest {
    /// Responses below this safety score threshold will fail
    #[arg(short, long, value_parser = parse_threshold, allow_hyphen_values = true)]
    pub threshold: f32,
    /// Number of variations per unsafe case
    #[arg(short, long, value_parser = parse_variations, allow_hyphen_values = true)]
    pub variations: i32,
    /// Maximum iteration layers for tests
    #[arg(
        short,
        long,
        value_parser = parse_maximum_iteration_layers,
        allow_hyphen_values = true
    )]
    pub maximum_iteration_layers: i32,
    /// One or more comma-separated test case groups to run
    #[arg(
        long,
        value_delimiter = ',',
        value_parser = parse_test_case_group,
        required = true
    )]
    pub test_case_groups: Vec<TestCaseGroup>,
}

/// Client initiates a single-turn evaluation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTurnRequestEnvelope {
    /// Protocol version
    pub version: String,
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Request payload
    pub data: SingleTurnRequest,
}

impl From<SingleTurnRequest> for SingleTurnRequestEnvelope {
    fn from(data: SingleTurnRequest) -> Self {
        SingleTurnRequestEnvelope {
            version: crate::consts::version::PROTOCOL_VERSION.to_string(),
            message_type: "single_turn_request".to_string(),
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SingleTurnRequest, SingleTurnRequestEnvelope};
    use serde_json::json;

    #[test]
    fn single_turn_request_envelope_serializes_to_protocol_shape() {
        let envelope = SingleTurnRequestEnvelope::from(SingleTurnRequest {
            threshold: 0.5,
            variations: 3,
            maximum_iteration_layers: 2,
            test_case_groups: vec!["suicidal_ideation".to_string(), "custom_group".to_string()],
        });

        let value = serde_json::to_value(envelope).expect("envelope should serialize");

        assert_eq!(
            value,
            json!({
                "version": crate::consts::version::PROTOCOL_VERSION,
                "type": "single_turn_request",
                "data": {
                    "threshold": 0.5,
                    "variations": 3,
                    "maximum_iteration_layers": 2,
                    "test_case_groups": [
                        "suicidal_ideation",
                        "custom_group"
                    ]
                }
            })
        );
    }
}
