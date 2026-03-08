use super::super::common::TestCaseGroup;
use clap;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use strum::EnumString;

#[derive(Debug, Clone, Serialize, Deserialize, EnumString, ValueEnum)]
#[serde(rename_all = "snake_case")]
#[value(rename_all = "snake_case")]
pub enum MultiTurnTestType {
    UserPersona,
    SemanticChunks,
}

/// Payload for `MultiTurnRequestEnvelope` messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct MultiTurnRequest {
    /// Responses below this safety score threshold will fail
    #[arg(short, long)]
    pub threshold: f32,
    /// Maximum number of turns in the conversation. Should be a multiple of two.
    #[arg(short, long)]
    pub max_turns: usize,
    /// One or more comma-separated test case groups to run.
    #[arg(long, value_delimiter = ',', default_value = "suicidal_ideation")]
    pub test_case_groups: Vec<TestCaseGroup>,
    /// One or more comma-separated multi-turn-test types
    #[arg(long, value_delimiter = ',')]
    pub test_types: Vec<MultiTurnTestType>,
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
