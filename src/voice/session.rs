//! Transport-independent voice session contracts.
use super::{Error, Result, protocol::Input};
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;

pub type Output = mpsc::Sender<Message>;

#[async_trait]
pub trait VoiceSessionProvider: Send + Sync {
    async fn connect(&self, session_id: i32, max_turns: u32) -> Result<Box<dyn VoiceSession>>;
}
#[async_trait]
pub trait VoiceSession: Send {
    async fn run(
        self: Box<Self>,
        input: mpsc::Receiver<Input>,
        output: Output,
        stop: watch::Receiver<bool>,
    ) -> Result<()>;
}

pub async fn emit(output: &Output, event: Value) -> Result<()> {
    output
        .send(Message::Text(event.to_string().into()))
        .await
        .map_err(|_| Error::Transport)
}
