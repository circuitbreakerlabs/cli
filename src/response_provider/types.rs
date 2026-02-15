use crate::protocol_types::Message;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct CompletionChoice {
    pub finish_reason: String,
    pub message: Message,
}

#[derive(Serialize, Deserialize)]
pub struct CompletionResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<CompletionChoice>,
}
