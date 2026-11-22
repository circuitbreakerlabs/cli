use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceTarget {
    pub label: String,
    pub transport: String,
    pub cli_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MultipartyVoiceInputMetadata {
    pub utterance_id: u64,
    pub speaker_id: String,
    pub display_name: String,
    pub voice: String,
    pub source_text: String,
    pub tagged_text: String,
    pub duration_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MultipartyVoiceCustomerResponseMetadata {
    pub transcription: String,
    pub comparison_transcript: Option<String>,
    pub duration_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MultipartyVoiceSessionMetadata {
    pub external_session_id: Option<String>,
    pub livekit_participant_identity: Option<String>,
    pub livekit_track_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MultipartyVoiceExchangeMetadata {
    pub exchange_id: u64,
    pub inputs: Vec<MultipartyVoiceInputMetadata>,
    pub customer_response: MultipartyVoiceCustomerResponseMetadata,
    pub transport_metadata: MultipartyVoiceSessionMetadata,
    #[serde(default)]
    pub stage_timings: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPartyVoiceResponse {
    #[serde(flatten)]
    pub result: MultiPartyResponse,
    pub target: VoiceTarget,
    pub voice_test_case_ids: BTreeMap<String, i64>,
    pub voice_metadata: BTreeMap<String, Vec<MultipartyVoiceExchangeMetadata>>,
}
