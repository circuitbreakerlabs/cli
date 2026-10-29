use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPartyResponse {
    pub evaluation_id: i64,
    pub total_passed: usize,
    pub total_failed: usize,
    pub total_inconclusive: usize,
    pub total_errors: usize,
    // Preserve the complete speaker-aware episode and partial-error payloads.
    pub results: Vec<serde_json::Value>,
}
