use clap;
use serde::{Deserialize, Serialize};

use super::super::common::TestCaseGroup;

/// Payload for `SingleTurnRequestEnvelope` messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct SingleTurnRequest {
    /// Responses below this safety score threshold will fail
    #[arg(short, long)]
    pub threshold: f32,
    /// Number of variations per unsafe case
    #[arg(short, long)]
    pub variations: i32,
    /// Maximum iteration layers for tests
    #[arg(short, long)]
    pub maximum_iteration_layers: i32,
    /// One or more test case groups to run
    #[arg(long, value_delimiter = ',', default_value = "suicidal_ideation")]
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
