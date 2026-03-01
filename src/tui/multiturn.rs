use std::collections::BTreeMap;
use std::sync::Arc;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::style::{Attribute, SetAttribute, SetForegroundColor};

use ratatui::layout::Constraint;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Row, Table};
use tokio::sync::RwLock;

use super::{ConversationStatus, WaitingFor};
use crate::protocol_types::common::{ConversationComplete, ConversationError};

type ConversationId = i32;

const PROGRESS_BAR_WIDTH: usize = 32;
const DOTS_SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub enum MultiTurnProgressIndicatorMessage {
    EvaluationStart {
        conversation_ids: Vec<i32>,
        max_turns: usize,
    },
    EvaluationComplete,
    ConversationComplete(ConversationComplete),
    ConversationError(ConversationError),
    ConversationTurn {
        conversation_id: ConversationId,
    },
    WaitingFor {
        conversation_id: ConversationId,
        waiting_for: WaitingFor,
    },
}

#[derive(Clone, Debug)]
struct ConversationState {
    current_turn: usize,
    max_turns: usize,
    status: ConversationStatus,
    spinner_offset: usize,
}

#[derive(Clone, Debug, Default)]
struct AppState {
    conversations: BTreeMap<ConversationId, ConversationState>,
    spinner_frame: usize,
    num_cases: usize,
    max_turns: usize,
    completed: bool,
    passed_count: usize,
    failed_count: usize,
    warning_count: usize,
}

fn get_header_lines() -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "Multi-turn evaluation",
            Style::default().add_modifier(ratatui::style::Modifier::BOLD),
        )]),
        Line::from(""),
    ]
}

fn print_footer(passed: usize, failed: usize, errors: usize) {
    println!();
    println!(
        "{}Multi-turn run summary{}",
        SetAttribute(Attribute::Bold),
        SetAttribute(Attribute::Reset)
    );
    println!(
        "  Passed: {}{}{} {}{}",
        SetAttribute(Attribute::Bold),
        SetForegroundColor(ratatui::crossterm::style::Color::Green),
        passed,
        SetForegroundColor(ratatui::crossterm::style::Color::Reset),
        SetAttribute(Attribute::Reset)
    );
    println!(
        "  Failed: {}{}{} {}{}",
        SetAttribute(Attribute::Bold),
        SetForegroundColor(ratatui::crossterm::style::Color::Red),
        failed,
        SetForegroundColor(ratatui::crossterm::style::Color::Reset),
        SetAttribute(Attribute::Reset)
    );
    println!(
        "  Errors: {}{}{} {}{}",
        SetAttribute(Attribute::Bold),
        SetForegroundColor(ratatui::crossterm::style::Color::Yellow),
        errors,
        SetForegroundColor(ratatui::crossterm::style::Color::Reset),
        SetAttribute(Attribute::Reset)
    );
}

pub async fn render_task(
    mut rx: tokio::sync::mpsc::Receiver<MultiTurnProgressIndicatorMessage>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = Arc::new(RwLock::new(AppState::default()));

    let (num_cases, _max_turns) = loop {
        if let Some(msg) = rx.recv().await {
            if let MultiTurnProgressIndicatorMessage::EvaluationStart {
                conversation_ids,
                max_turns,
            } = msg
            {
                let mut state_guard = state.write().await;
                state_guard.num_cases = conversation_ids.len();
                state_guard.max_turns = max_turns;
                for id in conversation_ids {
                    let spinner_offset = ((id * 4) as usize) % DOTS_SPINNER_FRAMES.len();
                    state_guard.conversations.insert(
                        id,
                        ConversationState {
                            current_turn: 0,
                            max_turns,
                            status: ConversationStatus::Waiting(WaitingFor::Provider),
                            spinner_offset,
                        },
                    );
                }
                break (state_guard.num_cases, max_turns);
            }
        } else {
            return Ok(());
        }
    };

    let stdout = std::io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let options = ratatui::TerminalOptions {
        viewport: ratatui::Viewport::Inline((get_header_lines().len() + num_cases) as u16),
    };
    let mut terminal = Terminal::with_options(backend, options)?;

    // spinner task
    let spinner_state = Arc::clone(&state);
    let _spinner_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
        loop {
            interval.tick().await;
            let mut state = spinner_state.write().await;
            state.spinner_frame = (state.spinner_frame + 1) % DOTS_SPINNER_FRAMES.len();
        }
    });

    loop {
        // check for new messages with timeout to allow spinner updates
        match tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await {
            Ok(Some(msg)) => {
                let is_done = handle_message(&state, msg).await?;
                if is_done {
                    break;
                }
            }
            Ok(None) => break,
            Err(_) => {}
        }

        let state_guard = state.read().await;
        render(&mut terminal, &state_guard)?;
    }

    let state_guard = state.read().await;
    render(&mut terminal, &state_guard)?;

    terminal.clear()?;

    print_footer(
        state_guard.passed_count,
        state_guard.failed_count,
        state_guard.warning_count,
    );

    Ok(())
}

async fn handle_message(
    state: &Arc<RwLock<AppState>>,
    msg: MultiTurnProgressIndicatorMessage,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let mut state = state.write().await;

    match msg {
        MultiTurnProgressIndicatorMessage::EvaluationStart { .. } => {
            tracing::warn!("Received duplicate EvaluationStart message, ignoring");
        }
        MultiTurnProgressIndicatorMessage::EvaluationComplete => {
            state.completed = true;
            return Ok(true);
        }
        MultiTurnProgressIndicatorMessage::ConversationComplete(complete) => {
            if let Some(conv) = state.conversations.get_mut(&complete.conversation_id) {
                conv.current_turn = complete.turns as usize;
                if complete.passed {
                    conv.status = ConversationStatus::Passed;
                    state.passed_count += 1;
                } else {
                    conv.status = ConversationStatus::Failed;
                    state.failed_count += 1;
                }
            }
        }
        MultiTurnProgressIndicatorMessage::ConversationError(error) => {
            if let Some(conv) = state.conversations.get_mut(&error.conversation_id) {
                conv.status = ConversationStatus::Warning;
                state.warning_count += 1;
            }
        }
        MultiTurnProgressIndicatorMessage::ConversationTurn { conversation_id } => {
            if let Some(conv) = state.conversations.get_mut(&conversation_id) {
                conv.current_turn += 1;
            }
        }
        MultiTurnProgressIndicatorMessage::WaitingFor {
            conversation_id,
            waiting_for,
        } => {
            if let Some(conv) = state.conversations.get_mut(&conversation_id) {
                conv.status = ConversationStatus::Waiting(waiting_for);
            }
        }
    }

    Ok(false)
}

fn render(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &AppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    terminal.draw(|frame| {
        let area = frame.area();

        let rows: Vec<Row> = state
            .conversations
            .keys()
            .map(|id| {
                let conv = state.conversations.get(id).unwrap();

                // status indicator
                let (status_char, status_color) = match &conv.status {
                    ConversationStatus::Waiting(_) => {
                        let frame =
                            (state.spinner_frame + conv.spinner_offset) % DOTS_SPINNER_FRAMES.len();
                        (DOTS_SPINNER_FRAMES[frame], Color::Blue)
                    }
                    ConversationStatus::Passed => ('✓', Color::Green),
                    ConversationStatus::Failed => ('✗', Color::Red),
                    ConversationStatus::Warning => ('▲', Color::Yellow),
                };

                // progress bar
                let progress_len = if conv.max_turns > 0 {
                    (conv.current_turn * PROGRESS_BAR_WIDTH) / conv.max_turns
                } else {
                    0
                };
                let progress_bar_filled = "=".repeat(progress_len);
                let progress_bar_empty = " ".repeat(PROGRESS_BAR_WIDTH - progress_len);

                let line = Line::from(vec![
                    Span::raw("["),
                    Span::styled(status_char.to_string(), Style::default().fg(status_color)),
                    Span::raw("]"),
                    Span::raw("["),
                    Span::styled(
                        progress_bar_filled,
                        Style::default().add_modifier(ratatui::style::Modifier::BOLD),
                    ),
                    Span::raw(progress_bar_empty),
                    Span::raw("]"),
                ]);

                Row::new(vec![Cell::from(line)])
            })
            .collect();

        let all_rows: Vec<Row> = get_header_lines()
            .into_iter()
            .map(|line| Row::new(vec![Cell::from(line)]))
            .chain(rows)
            .collect();

        let table = Table::new(all_rows, &[Constraint::Fill(1)]);

        frame.render_widget(table, area);
    })?;

    Ok(())
}
