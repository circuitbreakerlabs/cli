use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::oneshot;
use tokio::task::JoinSet;

use crate::protocol_types::{CompletionRequest, ConversationId};
use crate::response_provider::{ConversationContext, ResponseProvider, SESSION_ENDED};

#[derive(Default)]
pub(super) struct Sessions {
    active: HashMap<ConversationId, Session>,
    ended: HashSet<ConversationId>,
    cleanup_tasks: JoinSet<()>,
}

struct Session {
    context: ConversationContext,
    attempted: bool,
    ended_by_provider: bool,
    turn: Option<Turn>,
}

enum Turn {
    Queued,
    Running {
        stop: oneshot::Sender<()>,
        finished: oneshot::Receiver<()>,
    },
}

#[derive(thiserror::Error, Debug)]
pub(super) enum SessionUnavailable {
    #[error("Conversation has ended")]
    Closed,
    #[error("Conversation already has a pending turn")]
    Busy,
}

impl Sessions {
    pub(super) fn queue_turn(
        &mut self,
        request: &CompletionRequest,
    ) -> Result<(), SessionUnavailable> {
        if self.ended.contains(&request.conversation_id) {
            return Err(SessionUnavailable::Closed);
        }
        let session = self
            .active
            .entry(request.conversation_id)
            .or_insert_with(|| Session {
                context: ConversationContext {
                    first_request_id: request.request_id.clone(),
                    assistant_replies: Vec::new(),
                },
                attempted: false,
                ended_by_provider: false,
                turn: None,
            });
        if session.ended_by_provider {
            return Err(SessionUnavailable::Closed);
        }
        if session.turn.is_some() {
            return Err(SessionUnavailable::Busy);
        }
        session.turn = Some(Turn::Queued);
        Ok(())
    }

    pub(super) fn context(&self, id: ConversationId) -> Option<&ConversationContext> {
        self.active.get(&id).map(|session| &session.context)
    }

    pub(super) fn started(
        &mut self,
        id: ConversationId,
        stop: oneshot::Sender<()>,
        finished: oneshot::Receiver<()>,
    ) {
        if let Some(session) = self.active.get_mut(&id) {
            session.attempted = true;
            session.turn = Some(Turn::Running { stop, finished });
        }
    }

    /// Returns false for a late result after the API ended the case.
    pub(super) fn completed(
        &mut self,
        id: ConversationId,
        reply: Option<(&str, Option<&str>)>,
    ) -> bool {
        let Some(session) = self.active.get_mut(&id) else {
            return false;
        };
        session.turn = None;
        if let Some((content, finish_reason)) = reply {
            session.ended_by_provider = finish_reason == Some(SESSION_ENDED);
            session.context.assistant_replies.push(content.to_owned());
        }
        true
    }

    pub(super) fn end(&mut self, id: ConversationId, provider: &Arc<dyn ResponseProvider>) {
        self.ended.insert(id);
        let Some(session) = self.active.remove(&id) else {
            return;
        };
        let pending_attempt = match session.turn {
            Some(Turn::Running { stop, finished }) => {
                let _ = stop.send(());
                Some(finished)
            }
            _ => None,
        };
        if session.attempted && !session.ended_by_provider {
            while self.cleanup_tasks.try_join_next().is_some() {}
            let provider = provider.clone();
            self.cleanup_tasks.spawn(async move {
                if let Some(finished) = pending_attempt {
                    // An attempt already in progress settles before the worker exits.
                    let _ = finished.await;
                }
                if let Err(error) = provider.close_conversation(&session.context).await {
                    tracing::warn!(conversation_id = id, %error, "Could not close provider conversation");
                }
            });
        }
    }

    pub(super) async fn finish(mut self, provider: &Arc<dyn ResponseProvider>) {
        for id in self.active.keys().copied().collect::<Vec<_>>() {
            self.end(id, provider);
        }
        while self.cleanup_tasks.join_next().await.is_some() {}
    }
}
