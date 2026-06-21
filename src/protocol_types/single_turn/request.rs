use serde::{Deserialize, Serialize};

use super::super::common::{TestCaseGroup, parse_test_case_group};

pub(crate) fn parse_threshold(value: &str) -> Result<f32, String> {
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

pub(crate) fn parse_variations(value: &str) -> Result<i32, String> {
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

pub(crate) fn parse_maximum_iteration_layers(value: &str) -> Result<i32, String> {
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

fn parse_test_result_id(value: &str) -> Result<i64, String> {
    let parsed: i64 = value
        .parse()
        .map_err(|_| format!("invalid test_result_id '{value}': expected an integer >= 1"))?;
    if parsed >= 1 {
        Ok(parsed)
    } else {
        Err(format!(
            "invalid test_result_id '{value}': expected an integer >= 1",
        ))
    }
}

#[derive(Debug, Clone, clap::Args)]
#[command(group(
    clap::ArgGroup::new("single_turn_source")
        .args(["test_case_groups", "test_result_id"])
        .required(true)
        .multiple(false)
))]
pub struct SingleTurnEvalRequest {
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
    #[arg(long, value_delimiter = ',', value_parser = parse_test_case_group)]
    pub test_case_groups: Option<Vec<TestCaseGroup>>,
    /// Historic test result ID to re-run
    #[arg(long, value_parser = parse_test_result_id)]
    pub test_result_id: Option<i64>,
}

/// Payload for `SingleTurnRequestEnvelope` messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTurnRequest {
    /// Responses below this safety score threshold will fail
    pub threshold: f32,
    /// Number of variations per unsafe case
    pub variations: i32,
    /// Maximum iteration layers for tests
    pub maximum_iteration_layers: i32,
    /// One or more comma-separated test case groups to run
    pub test_case_groups: Vec<TestCaseGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTurnRerunRequest {
    pub test_result_id: i64,
    pub threshold: f32,
    pub variations: i32,
    pub maximum_iteration_layers: i32,
}

#[derive(Debug, Clone)]
pub enum SingleTurnEvaluationRequest {
    Standard(SingleTurnRequest),
    Rerun(SingleTurnRerunRequest),
}

impl SingleTurnEvaluationRequest {
    pub fn maximum_iteration_layers(&self) -> i32 {
        match self {
            SingleTurnEvaluationRequest::Standard(request) => request.maximum_iteration_layers,
            SingleTurnEvaluationRequest::Rerun(request) => request.maximum_iteration_layers,
        }
    }

    pub fn test_case_groups(&self) -> Option<&[TestCaseGroup]> {
        match self {
            SingleTurnEvaluationRequest::Standard(request) => Some(&request.test_case_groups),
            SingleTurnEvaluationRequest::Rerun(_) => None,
        }
    }

    pub fn test_result_id(&self) -> Option<i64> {
        match self {
            SingleTurnEvaluationRequest::Standard(_) => None,
            SingleTurnEvaluationRequest::Rerun(request) => Some(request.test_result_id),
        }
    }
}

impl From<SingleTurnEvalRequest> for SingleTurnEvaluationRequest {
    fn from(request: SingleTurnEvalRequest) -> Self {
        if let Some(test_result_id) = request.test_result_id {
            return SingleTurnEvaluationRequest::Rerun(SingleTurnRerunRequest {
                test_result_id,
                threshold: request.threshold,
                variations: request.variations,
                maximum_iteration_layers: request.maximum_iteration_layers,
            });
        }

        SingleTurnEvaluationRequest::Standard(SingleTurnRequest {
            threshold: request.threshold,
            variations: request.variations,
            maximum_iteration_layers: request.maximum_iteration_layers,
            test_case_groups: request
                .test_case_groups
                .expect("clap requires single-turn source"),
        })
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTurnRerunRequestEnvelope {
    pub version: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub data: SingleTurnRerunRequest,
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

impl From<SingleTurnRerunRequest> for SingleTurnRerunRequestEnvelope {
    fn from(data: SingleTurnRerunRequest) -> Self {
        SingleTurnRerunRequestEnvelope {
            version: crate::consts::version::PROTOCOL_VERSION.to_string(),
            message_type: "single_turn_rerun_request".to_string(),
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SingleTurnRequest, SingleTurnRequestEnvelope, SingleTurnRerunRequest,
        SingleTurnRerunRequestEnvelope,
    };
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

    #[test]
    fn single_turn_rerun_request_envelope_serializes_to_protocol_shape() {
        let envelope = SingleTurnRerunRequestEnvelope::from(SingleTurnRerunRequest {
            test_result_id: 42,
            threshold: 0.5,
            variations: 3,
            maximum_iteration_layers: 2,
        });

        let value = serde_json::to_value(envelope).expect("envelope should serialize");

        assert_eq!(
            value,
            json!({
                "version": crate::consts::version::PROTOCOL_VERSION,
                "type": "single_turn_rerun_request",
                "data": {
                    "test_result_id": 42,
                    "threshold": 0.5,
                    "variations": 3,
                    "maximum_iteration_layers": 2
                }
            })
        );
    }
}
