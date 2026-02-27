use super::WaitingFor;
use crate::protocol_types::common::{ConversationComplete, ConversationError};
use crate::protocol_types::multi_turn::MultiTurnEvaluationStart;

pub enum MultiTurnProgressIndicatorMessage {
    EvaluationStart(MultiTurnEvaluationStart),
    ConversationComplete(ConversationComplete),
    ConversationError(ConversationError),
    ConversationTurn {
        conversation_id: i32,
    },
    WaitingFor {
        conversation_id: i32,
        waiting_for: WaitingFor,
    },
}
