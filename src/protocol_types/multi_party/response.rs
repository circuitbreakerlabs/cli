use serde::{Deserialize, Serialize};

use crate::protocol_types::{ConversationId, Message};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultiPartyStatus {
    Passed,
    Failed,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultiPartyErrorCode {
    CasesUnavailable,
    CustomerFailure,
    UserFailure,
    Timeout,
    InternalFailure,
    EvaluationFailure,
    QuotaExceeded,
    PersistenceFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPartyParticipantResult {
    pub participant_id: String,
    pub test_case_id: i64,
    pub test_result_id: Option<i64>,
    pub status: MultiPartyStatus,
    pub safe_response_score: Option<f64>,
    pub passed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPartyEvaluationResult {
    pub conversation_id: ConversationId,
    pub multiparty_test_case_id: i64,
    pub status: MultiPartyStatus,
    pub turns: usize,
    pub min_safe_response_score: Option<f64>,
    pub passed: Option<bool>,
    pub conversation: Vec<Message>,
    pub participant_results: Vec<MultiPartyParticipantResult>,
    pub error_code: Option<MultiPartyErrorCode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPartyResponse {
    pub evaluation_id: i64,
    pub total_passed: usize,
    pub total_failed: usize,
    pub total_errors: usize,
    pub results: Vec<MultiPartyEvaluationResult>,
}
