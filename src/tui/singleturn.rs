use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::style::{Attribute, SetAttribute, SetForegroundColor};

use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Row, Table};
use tokio::sync::RwLock;

use super::common::{ConversationStatus, WaitingFor, get_status_indicator_spans};
use crate::protocol_types::ConversationId;
use crate::protocol_types::common::{ConversationComplete, ConversationError};
use crate::protocol_types::single_turn::{IterationComplete, IterationStart};

const GRID_COLUMNS: usize = 5;

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

#[derive(Clone, Debug)]
struct ConversationState {
    id: ConversationId,
    status: ConversationStatus,
}

#[derive(Clone, Debug)]
struct IterationState {
    iteration_number: i32,
    conversation_ids: Vec<ConversationId>,
    conversations: BTreeMap<ConversationId, ConversationState>,
    completed: bool,
    passed_count: usize,
    failed_count: usize,
    warning_count: usize,
}

#[derive(Clone, Debug, Default)]
struct CompletedIteration {
    iteration_number: i32,
    passed_count: usize,
    failed_count: usize,
    warning_count: usize,
}

impl From<IterationState> for CompletedIteration {
    fn from(iteration: IterationState) -> Self {
        CompletedIteration {
            iteration_number: iteration.iteration_number,
            passed_count: iteration.passed_count,
            failed_count: iteration.failed_count,
            warning_count: iteration.warning_count,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct AppState {
    current_iteration: Option<IterationState>,
    completed_iterations: Vec<CompletedIteration>,
}

fn get_header_line(iteration_number: i32, num_cases: usize) -> Line<'static> {
    Line::from(vec![Span::styled(
        format!("Iteration {iteration_number} ({num_cases} cases)"),
        Style::default().add_modifier(ratatui::style::Modifier::BOLD),
    )])
}

fn get_summary_line(
    iteration_number: i32,
    passed: usize,
    failed: usize,
    warning: usize,
) -> Line<'static> {
    Line::from(vec![
        Span::raw(format!("Iteration {iteration_number} complete. ")),
        Span::styled(
            format!("{passed}"),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" passed, "),
        Span::styled(
            format!("{failed}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" failed, "),
        Span::styled(
            format!("{warning}"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" errors"),
    ])
}

fn print_summary_history(completed_iterations: &[CompletedIteration]) {
    for iteration in completed_iterations {
        println!(
            "Iteration {} complete. {}{}{}{}{} passed, {}{}{}{}{} failed, {}{}{}{}{} errors",
            iteration.iteration_number,
            SetAttribute(Attribute::Bold),
            SetForegroundColor(ratatui::crossterm::style::Color::Green),
            iteration.passed_count,
            SetForegroundColor(ratatui::crossterm::style::Color::Reset),
            SetAttribute(Attribute::Reset),
            SetAttribute(Attribute::Bold),
            SetForegroundColor(ratatui::crossterm::style::Color::Red),
            iteration.failed_count,
            SetForegroundColor(ratatui::crossterm::style::Color::Reset),
            SetAttribute(Attribute::Reset),
            SetAttribute(Attribute::Bold),
            SetForegroundColor(ratatui::crossterm::style::Color::Yellow),
            iteration.warning_count,
            SetForegroundColor(ratatui::crossterm::style::Color::Reset),
            SetAttribute(Attribute::Reset),
        );
    }
}

fn print_final_summary(passed: usize, failed: usize, errors: usize) {
    println!(
        "{}Single-turn run summary{}",
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
    mut progress_rx: tokio::sync::mpsc::Receiver<SingleTurnProgressIndicatorMessage>,
    maximum_iteration_layers: i32,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = Arc::new(RwLock::new(AppState::default()));

    let stdout = std::io::stdout();
    let backend = CrosstermBackend::new(stdout);

    /*  this reserves the maximum needed viewport height for all iterations
     *  this will break if there are more than <terminal_width> * 3 cases in the final iteration
     */
    let viewport_height = maximum_iteration_layers // summary line per layer
    + 1 // header
    + 1 // blank
    + 3; // grid
    let options = ratatui::TerminalOptions {
        viewport: ratatui::Viewport::Inline(u16::try_from(viewport_height)?),
    };
    let mut terminal = Terminal::with_options(backend, options)?;

    let start = Instant::now();
    let mut render_interval = tokio::time::interval(Duration::from_millis(50));

    loop {
        tokio::select! {
            msg = progress_rx.recv() => {
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
    drop(terminal);

    print_summary_history(&state_guard.completed_iterations);

    let total_passed: usize = state_guard
        .completed_iterations
        .iter()
        .map(|i| i.passed_count)
        .sum();
    let total_failed: usize = state_guard
        .completed_iterations
        .iter()
        .map(|i| i.failed_count)
        .sum();
    let total_warnings: usize = state_guard
        .completed_iterations
        .iter()
        .map(|i| i.warning_count)
        .sum();

    print_final_summary(total_passed, total_failed, total_warnings);

    Ok(())
}

async fn handle_message(
    state: &Arc<RwLock<AppState>>,
    msg: SingleTurnProgressIndicatorMessage,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let mut state = state.write().await;

    match msg {
        SingleTurnProgressIndicatorMessage::IterationStart(iteration_start) => {
            let conversations = iteration_start
                .conversation_ids
                .iter()
                .map(|id| {
                    (
                        *id,
                        ConversationState {
                            id: *id,
                            status: ConversationStatus::Waiting(WaitingFor::Provider),
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>();

            state.current_iteration = Some(IterationState {
                iteration_number: iteration_start.iteration_number,
                conversation_ids: iteration_start.conversation_ids,
                conversations,
                completed: false,
                passed_count: 0,
                failed_count: 0,
                warning_count: 0,
            });
        }
        SingleTurnProgressIndicatorMessage::IterationComplete(iteration_complete) => {
            if let Some(iteration) = &mut state.current_iteration {
                iteration.completed = true;
                iteration.passed_count = iteration_complete.passed_conversation_ids.len();
                iteration.failed_count = iteration_complete.failed_conversation_ids.len();
                iteration.warning_count = iteration
                    .conversations
                    .values()
                    .filter(|c| matches!(c.status, ConversationStatus::Warning))
                    .count();

                let complete = CompletedIteration::from(iteration.clone());
                state.completed_iterations.push(complete);
            }
        }
        SingleTurnProgressIndicatorMessage::ConversationComplete(conversation_complete) => {
            if let Some(iteration) = &mut state.current_iteration
                && let Some(conv) = iteration
                    .conversations
                    .get_mut(&conversation_complete.conversation_id)
            {
                if conversation_complete.passed {
                    conv.status = ConversationStatus::Passed;
                } else {
                    conv.status = ConversationStatus::Failed;
                }
            }
        }
        SingleTurnProgressIndicatorMessage::ConversationError(conversation_error) => {
            if let Some(iteration) = &mut state.current_iteration
                && let Some(conv) = iteration
                    .conversations
                    .get_mut(&conversation_error.conversation_id)
            {
                conv.status = ConversationStatus::Warning;
            }
        }
        SingleTurnProgressIndicatorMessage::WaitingFor {
            conversation_id,
            waiting_for,
        } => {
            if let Some(iteration) = &mut state.current_iteration
                && let Some(conv) = iteration.conversations.get_mut(&conversation_id)
            {
                conv.status = ConversationStatus::Waiting(waiting_for);
            }
        }
    }

    Ok(false)
}

fn get_current_iteration_rows(
    iteration: &IterationState,
    elapsed_spinner_frames: usize,
) -> Vec<Row<'_>> {
    let mut rows: Vec<Row<'_>> = Vec::new();

    // header
    let header = get_header_line(iteration.iteration_number, iteration.conversation_ids.len());
    rows.push(Row::new(vec![Cell::from(header)]));
    rows.push(Row::new(vec![Cell::from(Line::from(""))]));

    // grid
    let conversation_values: Vec<&ConversationState> = iteration.conversations.values().collect();
    let num_rows = conversation_values.len().div_ceil(GRID_COLUMNS);

    rows.extend((0..num_rows).filter_map(|row_idx| {
        let row_spans = (0..GRID_COLUMNS)
            .filter_map(|col_idx| {
                let idx = row_idx * GRID_COLUMNS + col_idx;
                conversation_values.get(idx)
            })
            .flat_map(|conv| {
                get_status_indicator_spans(&conv.status, conv.id, elapsed_spinner_frames)
            })
            .collect::<Vec<_>>();

        if row_spans.is_empty() {
            None
        } else {
            Some(Row::new(vec![Cell::from(Line::from(row_spans))]))
        }
    }));

    rows.push(Row::new(vec![Cell::from(Line::from(""))]));

    rows
}

fn get_previous_iterations_summary_rows(
    completed_iterations: &[CompletedIteration],
) -> Vec<Row<'_>> {
    completed_iterations
        .iter()
        .map(|iteration| {
            Row::new(vec![Cell::from(get_summary_line(
                iteration.iteration_number,
                iteration.passed_count,
                iteration.failed_count,
                iteration.warning_count,
            ))])
        })
        .collect()
}

fn render(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &AppState,
    start: Instant,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let elapsed_spinner_frames = (start.elapsed().as_millis() / 100) as usize;

    terminal.draw(|frame| {
        let mut rows: Vec<Row<'_>> = Vec::new();

        rows.extend(get_previous_iterations_summary_rows(
            &state.completed_iterations,
        ));

        if let Some(iteration) = &state.current_iteration
            && !iteration.completed
        {
            rows.extend(get_current_iteration_rows(
                iteration,
                elapsed_spinner_frames,
            ));
        }

        let table = Table::new(rows, &[Constraint::Fill(1)]);
        frame.render_widget(table, frame.area());
    })?;

    Ok(())
}
