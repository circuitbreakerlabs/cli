use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

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

const SPINNER_PHASE_SPREAD: usize = 4;
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
    id: ConversationId,
    current_turn: usize,
    max_turns: usize,
    status: ConversationStatus,
}

#[derive(Clone, Debug, Default)]
struct AppState {
    conversations: BTreeMap<ConversationId, ConversationState>,
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
                    state_guard.conversations.insert(
                        id,
                        ConversationState {
                            id,
                            current_turn: 0,
                            max_turns,
                            status: ConversationStatus::Waiting(WaitingFor::Provider),
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
        viewport: ratatui::Viewport::Inline(u16::try_from(get_header_lines().len() + num_cases)?),
    };
    let mut terminal = Terminal::with_options(backend, options)?;

    let start = Instant::now();
    let mut render_interval = tokio::time::interval(Duration::from_millis(50));

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(msg) => {
                        if handle_message(&state, msg).await? {
                            break;
                        }
                    }
                    None => break,
                }
            }
            _ = render_interval.tick() => {}
        }

        let state_guard = state.read().await;
        render(&mut terminal, &state_guard, start)?;
    }

    let state_guard = state.read().await;
    render(&mut terminal, &state_guard, start)?;
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
                conv.current_turn = usize::try_from(complete.turns)?;
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

fn get_status_indicator_spans(
    conv: &ConversationState,
    elapsed_spinner_frames: usize,
) -> Vec<Span<'_>> {
    let (status_char, status_color) = match &conv.status {
        ConversationStatus::Waiting(_) => {
            let phase_offset = usize::try_from(conv.id).unwrap_or(0) * SPINNER_PHASE_SPREAD
                % DOTS_SPINNER_FRAMES.len();
            let frame_idx = (elapsed_spinner_frames + phase_offset) % DOTS_SPINNER_FRAMES.len();
            (DOTS_SPINNER_FRAMES[frame_idx], Color::Blue)
        }
        ConversationStatus::Passed => ('✓', Color::Green),
        ConversationStatus::Failed => ('✗', Color::Red),
        ConversationStatus::Warning => ('▲', Color::Yellow),
    };
    vec![
        Span::raw("["),
        Span::styled(status_char.to_string(), Style::default().fg(status_color)),
        Span::raw("]"),
    ]
}

fn get_progress_bar_spans(conv: &ConversationState) -> Vec<Span<'_>> {
    let progress_len = if conv.max_turns > 0 {
        (conv.current_turn * PROGRESS_BAR_WIDTH) / conv.max_turns
    } else {
        0
    };
    let progress_bar_filled = "=".repeat(progress_len);
    let progress_bar_empty = " ".repeat(PROGRESS_BAR_WIDTH - progress_len);
    vec![
        Span::raw("["),
        Span::styled(
            progress_bar_filled,
            Style::default().add_modifier(ratatui::style::Modifier::BOLD),
        ),
        Span::raw(progress_bar_empty),
        Span::raw("]"),
    ]
}

fn render(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &AppState,
    start: Instant,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 100ms per spinner tick
    let elapsed_spinner_frames = (start.elapsed().as_millis() / 100) as usize;

    terminal.draw(|frame| {
        let progress_rows = state.conversations.values().map(|conv| {
            let spans = get_status_indicator_spans(conv, elapsed_spinner_frames)
                .into_iter()
                .chain(get_progress_bar_spans(conv));
            let line = Line::from(spans.collect::<Vec<_>>());
            Row::new(vec![Cell::from(line)])
        });

        let all_rows = get_header_lines()
            .into_iter()
            .map(|line| Row::new(vec![Cell::from(line)]))
            .chain(progress_rows);

        let table = Table::new(all_rows, &[Constraint::Fill(1)]);

        frame.render_widget(table, frame.area());
    })?;

    Ok(())
}
