use super::common::WaitingFor;
use crate::protocol_types::ConversationId;
use crate::protocol_types::common::{ConversationComplete, ConversationError};
use crate::protocol_types::single_turn::{IterationComplete, IterationStart};

pub enum SingleTurnProgressIndicatorMessage {
    IterationStart(IterationStart),
    IterationComplete(IterationComplete),
    ConversationComplete(ConversationComplete),
    ConversationError(ConversationError),
    WaitingFor {
        conversation_id: ConversationId,
        waiting_for: WaitingFor,
    },
}

pub async fn render_task(
    mut progress_rx: tokio::sync::mpsc::Receiver<SingleTurnProgressIndicatorMessage>,
) {
    while let Some(msg) = progress_rx.recv().await {
        match msg {
            SingleTurnProgressIndicatorMessage::IterationStart(iteration_start) => {
                println!(
                    "Iteration {} started with {} conversations",
                    iteration_start.iteration_number,
                    iteration_start.conversation_ids.len()
                );
            }
            SingleTurnProgressIndicatorMessage::IterationComplete(iteration_complete) => {
                println!(
                    "Iteration {} complete: {} passed, {} failed",
                    iteration_complete.iteration_number,
                    iteration_complete.passed_conversation_ids.len(),
                    iteration_complete.failed_conversation_ids.len()
                );
            }
            SingleTurnProgressIndicatorMessage::ConversationComplete(conversation_complete) => {
                println!(
                    "Conversation {} completed: {}",
                    conversation_complete.conversation_id,
                    if conversation_complete.passed {
                        "passed"
                    } else {
                        "failed"
                    }
                );
            }
            SingleTurnProgressIndicatorMessage::ConversationError(conversation_error) => {
                eprintln!(
                    "Conversation {} error: {}",
                    conversation_error.conversation_id, conversation_error.error_message
                );
            }
            SingleTurnProgressIndicatorMessage::WaitingFor {
                conversation_id,
                waiting_for,
            } => {
                println!(
                    "Conversation {} waiting for {:?}",
                    conversation_id, waiting_for
                );
            }
        }
    }
}
