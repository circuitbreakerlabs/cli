use super::TuiError;
use super::common::{
    ConversationStatus, WaitingFor, get_progress_bar_spans, get_status_indicator_spans,
};
use crate::protocol_types::ConversationId;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    crossterm::style::{Attribute, SetAttribute, SetForegroundColor},
    layout::Constraint,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Row, Table},
};
use serde::Deserialize;
use std::collections::BTreeMap;
use tokio::{
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationOutcome {
    Passed,
    Failed,
    Error,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum MultiPartyProgressIndicatorMessage {
    #[serde(rename = "multiparty_evaluation_start")]
    EvaluationStart {
        conversation_ids: Vec<ConversationId>,
        max_turns: usize,
    },
    #[serde(rename = "multiparty_conversation_start")]
    ConversationStart { conversation_id: ConversationId },
    #[serde(rename = "multiparty_turn")]
    ConversationTurn {
        conversation_id: ConversationId,
        turns: usize,
    },
    #[serde(rename = "multiparty_conversation_complete")]
    ConversationComplete {
        conversation_id: ConversationId,
        status: ConversationOutcome,
    },
    #[serde(skip)]
    WaitingFor {
        conversation_id: ConversationId,
        waiting_for: WaitingFor,
    },
    #[serde(skip)]
    EvaluationComplete,
    #[serde(skip)]
    Ignored,
}

#[derive(Debug)]
struct ConversationState {
    turns: usize,
    status: ConversationStatus,
}

#[derive(Debug)]
struct AppState {
    conversations: BTreeMap<ConversationId, ConversationState>,
    max_turns: usize,
}

impl AppState {
    fn new(ids: Vec<ConversationId>, max_turns: usize) -> Self {
        Self {
            conversations: ids
                .into_iter()
                .map(|id| {
                    (
                        id,
                        ConversationState {
                            turns: 0,
                            status: ConversationStatus::Queued,
                        },
                    )
                })
                .collect(),
            max_turns,
        }
    }

    fn update(&mut self, message: MultiPartyProgressIndicatorMessage) {
        use MultiPartyProgressIndicatorMessage as Message;
        let (Message::ConversationStart { conversation_id }
        | Message::ConversationTurn {
            conversation_id, ..
        }
        | Message::ConversationComplete {
            conversation_id, ..
        }
        | Message::WaitingFor {
            conversation_id, ..
        }) = &message
        else {
            return;
        };
        let Some(conversation) = self.conversations.get_mut(conversation_id) else {
            return;
        };
        if !matches!(
            conversation.status,
            ConversationStatus::Queued | ConversationStatus::Waiting(_)
        ) {
            return;
        }
        match message {
            Message::ConversationStart { .. } => {
                conversation.status = ConversationStatus::Waiting(WaitingFor::API);
            }
            Message::ConversationTurn { turns, .. } => {
                // Only accepted utterances advance progress; retries do not.
                conversation.turns = conversation.turns.max(turns).min(self.max_turns);
                conversation.status = ConversationStatus::Waiting(WaitingFor::API);
            }
            Message::ConversationComplete { status, .. } => {
                conversation.status = match status {
                    ConversationOutcome::Passed => ConversationStatus::Passed,
                    ConversationOutcome::Failed => ConversationStatus::Failed,
                    ConversationOutcome::Error => ConversationStatus::Warning,
                };
            }
            Message::WaitingFor { waiting_for, .. } => {
                conversation.status = ConversationStatus::Waiting(waiting_for);
            }
            _ => {}
        }
    }

    fn totals(&self) -> [usize; 3] {
        let mut totals = [0; 3];
        for conversation in self.conversations.values() {
            match conversation.status {
                ConversationStatus::Passed => totals[0] += 1,
                ConversationStatus::Failed => totals[1] += 1,
                ConversationStatus::Warning => totals[2] += 1,
                _ => {}
            }
        }
        totals
    }
}

/// Render accepted utterances, rather than inferring turns from customer calls.
pub async fn render_task(
    mut rx: mpsc::Receiver<MultiPartyProgressIndicatorMessage>,
) -> Result<(), TuiError> {
    use MultiPartyProgressIndicatorMessage as Message;
    let mut state = loop {
        match rx.recv().await {
            Some(Message::EvaluationStart {
                conversation_ids,
                max_turns,
            }) => break AppState::new(conversation_ids, max_turns),
            Some(_) => {}
            None => return Err(TuiError::ChannelClosed),
        }
    };
    let height = u16::try_from(state.conversations.len().saturating_add(3)).unwrap_or(u16::MAX);
    let mut terminal = Terminal::with_options(
        CrosstermBackend::new(std::io::stdout()),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Inline(height),
        },
    )?;
    let start = Instant::now();
    let mut interval = tokio::time::interval(Duration::from_millis(50));
    let result = loop {
        let complete = tokio::select! {
            message = rx.recv() => match message {
                Some(Message::EvaluationComplete) => true,
                Some(message) => {
                    state.update(message);
                    false
                }
                None => break Err(TuiError::ChannelClosed),
            },
            _ = interval.tick() => false,
        };
        let spinner_frame = usize::try_from(start.elapsed().as_millis() / 100).unwrap_or(0);
        if let Err(error) = terminal.draw(|frame| render(frame, &state, spinner_frame)) {
            break Err(TuiError::Render(error.to_string()));
        }
        if complete {
            break Ok(());
        }
    };
    terminal.clear()?;
    terminal.show_cursor()?;
    drop(terminal);
    result?;
    print_footer(state.totals());
    Ok(())
}

fn render(frame: &mut Frame<'_>, state: &AppState, spinner_frame: usize) {
    let header = [
        Line::raw(""),
        Line::styled(
            "Multi-party evaluation",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
    ];
    let rows = state.conversations.iter().map(|(id, conversation)| {
        let spans = get_status_indicator_spans(&conversation.status, *id, spinner_frame)
            .into_iter()
            .chain([Span::raw(format!(" Conversation {id} "))])
            .chain(get_progress_bar_spans(conversation.turns, state.max_turns))
            .chain([Span::raw(format!(
                " [{}/{}]",
                conversation.turns, state.max_turns,
            ))])
            .collect::<Vec<_>>();
        Row::new(vec![Cell::from(Line::from(spans))])
    });
    let table = Table::new(
        header
            .into_iter()
            .map(|line| Row::new(vec![Cell::from(line)]))
            .chain(rows),
        [Constraint::Fill(1)],
    );
    frame.render_widget(table, frame.area());
}

fn print_footer(totals: [usize; 3]) {
    use ratatui::crossterm::style::Color;
    println!(
        "\n{}Multi-party run summary{}",
        SetAttribute(Attribute::Bold),
        SetAttribute(Attribute::Reset)
    );
    for ((label, color), count) in [
        ("Passed", Color::Green),
        ("Failed", Color::Red),
        ("Errors", Color::Yellow),
    ]
    .into_iter()
    .zip(totals)
    {
        println!(
            "  {label}: {}{}{count}{}{}",
            SetAttribute(Attribute::Bold),
            SetForegroundColor(color),
            SetForegroundColor(ratatui::crossterm::style::Color::Reset),
            SetAttribute(Attribute::Reset),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{MultiPartyProgressIndicatorMessage as Message, *};
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    fn draw(state: &AppState, spinner_frame: usize, width: u16) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, 10)).unwrap();
        terminal
            .draw(|frame| render(frame, state, spinner_frame))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn text(buffer: &ratatui::buffer::Buffer) -> String {
        buffer
            .content
            .chunks(usize::from(buffer.area.width))
            .map(|row| {
                row.iter()
                    .map(ratatui::buffer::Cell::symbol)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn only_accepted_utterances_advance_progress() {
        let mut state = AppState::new(vec![1, 2], 11);
        state.update(Message::ConversationStart { conversation_id: 1 });
        state.update(Message::ConversationTurn {
            conversation_id: 1,
            turns: 5,
        });
        for waiting_for in [WaitingFor::Provider, WaitingFor::API, WaitingFor::Provider] {
            state.update(Message::WaitingFor {
                conversation_id: 1,
                waiting_for,
            });
            assert_eq!(state.conversations[&1].turns, 5);
        }
        state.update(Message::ConversationTurn {
            conversation_id: 1,
            turns: 3,
        });
        assert_eq!(state.conversations[&1].turns, 5);
        assert!(matches!(
            state.conversations[&2].status,
            ConversationStatus::Queued
        ));
        state.update(Message::ConversationTurn {
            conversation_id: 1,
            turns: 99,
        });
        assert_eq!(state.conversations[&1].turns, 11);
    }

    #[test]
    fn completion_is_idempotent_and_preserves_partial_progress() {
        let mut state = AppState::new(vec![1, 2, 3], 11);
        for (id, status) in [
            (1, ConversationOutcome::Passed),
            (2, ConversationOutcome::Failed),
            (3, ConversationOutcome::Error),
        ] {
            state.update(Message::ConversationTurn {
                conversation_id: id,
                turns: 5,
            });
            state.update(Message::ConversationComplete {
                conversation_id: id,
                status,
            });
            state.update(Message::ConversationComplete {
                conversation_id: id,
                status: ConversationOutcome::Passed,
            });
            state.update(Message::WaitingFor {
                conversation_id: id,
                waiting_for: WaitingFor::Provider,
            });
            state.update(Message::ConversationTurn {
                conversation_id: id,
                turns: 11,
            });
            assert_eq!(state.conversations[&id].turns, 5);
        }
        assert_eq!(state.totals(), [1, 1, 1]);
        let output = text(&draw(&state, 0, 110));
        assert!(output.contains("[5/11]"));
        assert!(!output.contains("[11/11]"));
    }

    #[test]
    fn renders_shared_bars_animated_spinners_and_distinct_outcomes() {
        let mut state = AppState::new(vec![1, 2, 3, 4, 5], 11);
        state.update(Message::ConversationTurn {
            conversation_id: 2,
            turns: 5,
        });
        state.update(Message::WaitingFor {
            conversation_id: 2,
            waiting_for: WaitingFor::Provider,
        });
        for (id, status) in [
            (3, ConversationOutcome::Passed),
            (4, ConversationOutcome::Failed),
            (5, ConversationOutcome::Error),
        ] {
            state.update(Message::ConversationTurn {
                conversation_id: id,
                turns: 11,
            });
            state.update(Message::ConversationComplete {
                conversation_id: id,
                status,
            });
        }
        let buffer = draw(&state, 0, 110);
        let output = text(&buffer);
        assert!(output.contains("Multi-party evaluation"));
        assert!(output.contains("[·] Conversation 1 [                                ] [0/11]"));
        assert!(output.contains("Conversation 2 [==============                  ] [5/11]"));
        assert!(output.contains("[✓] Conversation 3 [================================] [11/11]"));
        assert!(output.contains("[✗] Conversation 4"));
        assert!(output.contains("[▲] Conversation 5"));
        assert_eq!(buffer[(1, 3)].fg, Color::DarkGray);
        assert_eq!(buffer[(1, 4)].fg, Color::Magenta);
        assert_eq!(buffer[(1, 5)].fg, Color::Green);
        assert_eq!(buffer[(1, 6)].fg, Color::Red);
        assert_eq!(buffer[(1, 7)].fg, Color::Yellow);
        assert_ne!(
            buffer[(1, 4)].symbol(),
            draw(&state, 1, 110)[(1, 4)].symbol()
        );
    }

    #[test]
    fn ignores_unknown_conversations_and_duplicate_start_events() {
        let mut state = AppState::new(vec![1], 11);
        state.update(Message::ConversationTurn {
            conversation_id: 1,
            turns: 5,
        });
        state.update(Message::ConversationStart {
            conversation_id: 99,
        });
        state.update(Message::EvaluationStart {
            conversation_ids: vec![99],
            max_turns: 100,
        });
        assert_eq!(state.conversations.len(), 1);
        assert_eq!(state.conversations[&1].turns, 5);
        assert_eq!(state.max_turns, 11);
    }

    #[test]
    fn renders_narrow_terminals_and_empty_turn_budgets() {
        let state = AppState::new(vec![1], 0);
        let buffer = draw(&state, 0, 24);
        assert!(text(&buffer).contains("Multi-party evaluation"));
    }

    #[tokio::test]
    async fn exits_if_the_channel_closes_before_evaluation_start() {
        let (tx, rx) = mpsc::channel(1);
        drop(tx);
        assert!(matches!(
            render_task(rx).await,
            Err(TuiError::ChannelClosed)
        ));
    }
}
