//! Transport-independent voice session contracts.
use super::{
    Error, Result,
    multiparty_protocol::{
        Input as MultipartyInput, LogicalParticipant, Output as MultipartyOutputMessage,
    },
    protocol::{Input, Output as OutputMessage},
};
use async_trait::async_trait;
use tokio::sync::{mpsc, watch};

pub type Output = mpsc::Sender<OutputMessage>;
pub type MultipartyOutput = mpsc::Sender<MultipartyOutputMessage>;

#[async_trait]
pub trait VoiceSessionProvider: Send + Sync {
    async fn connect(&self, session_id: i32, max_turns: u32) -> Result<Box<dyn VoiceSession>>;

    async fn connect_multiparty(
        &self,
        _spec: MultipartySessionSpec,
    ) -> Result<Box<dyn MultipartyVoiceSession>> {
        Err(Error::Configuration(
            "target does not support multiparty voice",
        ))
    }
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

#[derive(Clone, Debug)]
pub struct MultipartySessionSpec {
    pub session_id: i32,
    pub conversation_id: i32,
    pub participants: Vec<LogicalParticipant>,
    pub max_exchanges: u32,
}

#[async_trait]
pub trait MultipartyVoiceSession: Send {
    async fn run(
        self: Box<Self>,
        input: mpsc::Receiver<MultipartyInput>,
        output: MultipartyOutput,
        stop: watch::Receiver<bool>,
    ) -> Result<()>;
}
