mod cli;
mod consts;
mod evaluations;
mod protocol_types;
mod response_provider;
mod tui;
mod update_check;
mod websockets;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use update_check::print_update_warning_if_needed;

use chrono::Local;
use clap::Parser;
use protocol_types::{MultiTurnRequest, SingleTurnRequest};
use ratatui::crossterm::style::{Attribute, Color, SetAttribute, SetForegroundColor};
use response_provider::{CustomProvider, OllamaProvider, OpenAIProvider, ResponseProvider};
use tui::{
    MultiTurnProgressIndicatorMessage, SingleTurnProgressIndicatorMessage, multiturn, singleturn,
};
use websockets::WebSocketConnection;

use evaluations::EvaluationError;
use thiserror::Error;

#[derive(Error, Debug)]
enum RunEvaluationError {
    #[error("Evaluation error: {0}")]
    Evaluation(#[from] EvaluationError),

    #[error("Result Serialization error: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("Result save error: {0}")]
    FileSave(#[from] std::io::Error),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_args = cli::Args::parse();

    if cli_args.log_mode {
        tracing_subscriber::fmt()
            .with_max_level(Into::<tracing::Level>::into(cli_args.log_level))
            .init();
    }

    let headers = cli_args.headers();

    let evaluation = match &cli_args.command {
        cli::Command::Eval { evaluation } => evaluation,
    };

    let provider_command = match evaluation {
        cli::EvaluationCommand::SingleTurn { provider, .. }
        | cli::EvaluationCommand::MultiTurn { provider, .. } => provider,
    };

    let provider = match provider_command {
        cli::ProviderCommand::Ollama(config) => {
            Arc::new(OllamaProvider::new(config.clone(), &headers)?) as Arc<dyn ResponseProvider>
        }
        cli::ProviderCommand::OpenAI(config) => {
            Arc::new(OpenAIProvider::new(config.clone(), &headers)?) as Arc<dyn ResponseProvider>
        }
        cli::ProviderCommand::Custom(config) => {
            Arc::new(CustomProvider::new(config, &headers)?) as Arc<dyn ResponseProvider>
        }
    };

    let websocket = websockets::connect(
        &cli_args.cbl_api_base_url,
        evaluation.into(),
        &cli_args.cbl_api_key,
    )
    .await?;

    match cli_args.command {
        cli::Command::Eval { evaluation } => match evaluation {
            cli::EvaluationCommand::SingleTurn { request, .. } => {
                run_single_turn_evaluation(
                    websocket,
                    provider,
                    request,
                    cli_args.log_mode,
                    cli_args.output_file,
                )
                .await?;
            }
            cli::EvaluationCommand::MultiTurn { request, .. } => {
                run_multi_turn_evaluation(
                    websocket,
                    provider,
                    request,
                    cli_args.log_mode,
                    cli_args.output_file,
                )
                .await?;
            }
        },
    }

    Ok(())
}

async fn run_single_turn_evaluation(
    websocket: WebSocketConnection,
    provider: Arc<dyn ResponseProvider>,
    request: SingleTurnRequest,
    log_mode: bool,
    output_file: Option<PathBuf>,
) -> Result<(), RunEvaluationError> {
    let result = if log_mode {
        evaluations::singleturn::run_evaluation(websocket, provider, request, None).await?
    } else {
        let (tx, rx) = tokio::sync::mpsc::channel::<SingleTurnProgressIndicatorMessage>(128);
        let render_handle = tokio::spawn(singleturn::render_task(
            rx,
            request.maximum_iteration_layers,
        ));

        let result =
            evaluations::singleturn::run_evaluation(websocket, provider, request, Some(tx)).await?;

        let _ = render_handle.await;
        result
    };

    let filename = output_file.unwrap_or_else(|| {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        PathBuf::from(format!(
            "circuit_breaker_labs_single_turn_evaluation_{timestamp}.json",
        ))
    });

    let json = serde_json::to_string_pretty(&result)?;
    std::fs::write(&filename, json)?;

    print_success_message(log_mode, "single", &filename);
    print_update_warning_if_needed(log_mode).await;

    Ok(())
}

async fn run_multi_turn_evaluation(
    websocket: WebSocketConnection,
    provider: Arc<dyn ResponseProvider>,
    request: MultiTurnRequest,
    log_mode: bool,
    output_file: Option<PathBuf>,
) -> Result<(), RunEvaluationError> {
    let result = if log_mode {
        evaluations::multiturn::run_evaluation(websocket, provider, request, None).await?
    } else {
        let (tx, rx) = tokio::sync::mpsc::channel::<MultiTurnProgressIndicatorMessage>(128);
        let render_handle = tokio::spawn(multiturn::render_task(rx));

        let result =
            evaluations::multiturn::run_evaluation(websocket, provider, request, Some(tx)).await?;

        let _ = render_handle.await;
        result
    };

    let filename = output_file.unwrap_or_else(|| {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        PathBuf::from(format!(
            "circuit_breaker_labs_multi_turn_evaluation_{timestamp}.json",
        ))
    });

    let json = serde_json::to_string_pretty(&result)?;
    std::fs::write(&filename, json)?;

    print_success_message(log_mode, "multi", &filename);
    print_update_warning_if_needed(log_mode).await;

    Ok(())
}

fn print_success_message(log_mode: bool, turn_type: &str, filename: &Path) {
    if log_mode {
        tracing::info!(
            "Saved full {}-turn evaluation results to {}",
            turn_type,
            filename.display(),
        );
    } else {
        println!(
            "Saved full {}-turn evaluation results to {}{}{}{}{}{}",
            turn_type,
            SetForegroundColor(Color::Magenta),
            SetAttribute(Attribute::Bold),
            SetAttribute(Attribute::Italic),
            filename.display(),
            SetAttribute(Attribute::Reset),
            SetForegroundColor(Color::Reset),
        );
    }
}
