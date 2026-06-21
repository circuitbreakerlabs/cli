use super::super::common::{TestCaseGroup, parse_test_case_group};
use serde::{Deserialize, Serialize};

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

pub(crate) fn parse_even_turn_count(value: &str) -> Result<usize, String> {
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
    clap::ArgGroup::new("multi_turn_source")
        .args(["test_case_groups", "test_result_id"])
        .required(true)
        .multiple(false)
))]
pub struct MultiTurnEvalRequest {
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
    #[arg(long, value_delimiter = ',', value_parser = parse_test_case_group)]
    pub test_case_groups: Option<Vec<TestCaseGroup>>,
    /// Historic test result ID to re-run
    #[arg(long, value_parser = parse_test_result_id)]
    pub test_result_id: Option<i64>,
}

/// Payload for `MultiTurnRequestEnvelope` messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnRequest {
    /// Responses below this safety score threshold will fail
    pub threshold: f32,
    /// Maximum number of turns in the conversation. Should be a multiple of two.
    pub max_turns: usize,
    /// One or more comma-separated test case groups to run.
    pub test_case_groups: Vec<TestCaseGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnRerunRequest {
    pub test_result_id: i64,
    pub threshold: f32,
    pub max_turns: usize,
}

#[derive(Debug, Clone)]
pub enum MultiTurnEvaluationRequest {
    Standard(MultiTurnRequest),
    Rerun(MultiTurnRerunRequest),
}

impl MultiTurnEvaluationRequest {
    pub fn max_turns(&self) -> usize {
        match self {
            MultiTurnEvaluationRequest::Standard(request) => request.max_turns,
            MultiTurnEvaluationRequest::Rerun(request) => request.max_turns,
        }
    }

    pub fn test_case_groups(&self) -> Option<&[TestCaseGroup]> {
        match self {
            MultiTurnEvaluationRequest::Standard(request) => Some(&request.test_case_groups),
            MultiTurnEvaluationRequest::Rerun(_) => None,
        }
    }

    pub fn test_result_id(&self) -> Option<i64> {
        match self {
            MultiTurnEvaluationRequest::Standard(_) => None,
            MultiTurnEvaluationRequest::Rerun(request) => Some(request.test_result_id),
        }
    }
}

impl From<MultiTurnEvalRequest> for MultiTurnEvaluationRequest {
    fn from(request: MultiTurnEvalRequest) -> Self {
        if let Some(test_result_id) = request.test_result_id {
            return MultiTurnEvaluationRequest::Rerun(MultiTurnRerunRequest {
                test_result_id,
                threshold: request.threshold,
                max_turns: request.max_turns,
            });
        }

        MultiTurnEvaluationRequest::Standard(MultiTurnRequest {
            threshold: request.threshold,
            max_turns: request.max_turns,
            test_case_groups: request
                .test_case_groups
                .expect("clap requires multi-turn source"),
        })
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnRerunRequestEnvelope {
    pub version: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub data: MultiTurnRerunRequest,
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

impl From<MultiTurnRerunRequest> for MultiTurnRerunRequestEnvelope {
    fn from(data: MultiTurnRerunRequest) -> Self {
        MultiTurnRerunRequestEnvelope {
            version: crate::consts::version::PROTOCOL_VERSION.to_string(),
            message_type: "multi_turn_rerun_request".to_string(),
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MultiTurnRequest, MultiTurnRequestEnvelope, MultiTurnRerunRequest,
        MultiTurnRerunRequestEnvelope,
    };
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

    #[test]
    fn multi_turn_rerun_request_envelope_serializes_to_protocol_shape() {
        let envelope = MultiTurnRerunRequestEnvelope::from(MultiTurnRerunRequest {
            test_result_id: 42,
            threshold: 0.5,
            max_turns: 6,
        });

        let value = serde_json::to_value(envelope).expect("envelope should serialize");

        assert_eq!(
            value,
            json!({
                "version": crate::consts::version::PROTOCOL_VERSION,
                "type": "multi_turn_rerun_request",
                "data": {
                    "test_result_id": 42,
                    "threshold": 0.5,
                    "max_turns": 6
                }
            })
        );
    }
}
