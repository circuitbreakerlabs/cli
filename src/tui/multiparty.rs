use super::TuiError;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    widgets::{Block, Borders, Paragraph},
};
use serde_json::Value;
use std::collections::BTreeMap;
use tokio::sync::mpsc;

/// Render accepted utterances, rather than inferring turns from customer calls.
pub async fn render_task(mut rx: mpsc::Receiver<Value>) -> Result<(), TuiError> {
    let mut rows = BTreeMap::<u64, (u64, String)>::new();
    let Some(start) = rx.recv().await else {
        return Ok(());
    };
    if let Some(ids) = start["data"]["conversation_ids"].as_array() {
        for id in ids.iter().filter_map(Value::as_u64) {
            rows.insert(id, (0, "queued".into()));
        }
    }
    let height = u16::try_from(rows.len() + 2).unwrap_or(u16::MAX);
    let mut terminal = Terminal::with_options(
        CrosstermBackend::new(std::io::stdout()),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Inline(height),
        },
    )?;
    while let Some(event) = rx.recv().await {
        let data = &event["data"];
        match event["type"].as_str().unwrap_or_default() {
            "complete" => break,
            "multiparty_evaluation_start" => {
                if let Some(ids) = data["conversation_ids"].as_array() {
                    for id in ids.iter().filter_map(Value::as_u64) {
                        rows.insert(id, (0, "queued".into()));
                    }
                }
            }
            kind => {
                if let Some(id) = data["conversation_id"].as_u64() {
                    let row = rows.entry(id).or_insert((0, "queued".into()));
                    match kind {
                        "multiparty_conversation_start" => row.1 = "running".into(),
                        "multiparty_turn" => row.0 = data["turns"].as_u64().unwrap_or(row.0),
                        "multiparty_conversation_complete" => {
                            row.1 = data["status"].as_str().unwrap_or("error").into();
                        }
                        _ => {}
                    }
                }
            }
        }
        let lines = rows
            .iter()
            .map(|(id, (turns, status))| {
                format!("Conversation {id}: {turns} utterances — {status}")
            })
            .collect::<Vec<_>>()
            .join("\n");
        terminal.draw(|frame| {
            let area = frame.area();
            frame.render_widget(
                Paragraph::new(lines).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Multi-party evaluation"),
                ),
                area,
            );
        })?;
    }
    terminal.clear()?;
    Ok(())
}
