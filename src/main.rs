mod cli;
mod consts;
mod evaluations;
mod protocol_types;
mod response_provider;
mod tui;
mod websockets;

use std::path::PathBuf;
use std::sync::Arc;

use chrono::Local;
use clap::Parser;
use protocol_types::{MultiTurnRequest, SingleTurnRequest};
use ratatui::crossterm::style::{Attribute, Color, Print, SetAttribute, SetForegroundColor};
use response_provider::{CustomProvider, OllamaProvider, OpenAIProvider, ResponseProvider};
use tui::{
    MultiTurnProgressIndicatorMessage, SingleTurnProgressIndicatorMessage, multiturn, singleturn,
};
use websockets::WebSocketConnection;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_args = cli::Args::parse();

    if cli_args.log_mode {
        tracing_subscriber::fmt()
            .with_max_level(Into::<tracing::Level>::into(cli_args.log_level))
            .init();
    }

    let headers = cli_args.headers();

    let provider_command = match &cli_args.evaluation {
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
            Arc::new(CustomProvider::new(&config, &headers)?) as Arc<dyn ResponseProvider>
        }
    };

    let websocket = websockets::connect(
        &cli_args.cbl_api_base_url,
        (&cli_args.evaluation).into(),
        &cli_args.cbl_api_key,
    )
    .await?;

    match cli_args.evaluation {
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
    }

    Ok(())
}

async fn run_single_turn_evaluation(
    websocket: WebSocketConnection,
    provider: Arc<dyn ResponseProvider>,
    request: SingleTurnRequest,
    log_mode: bool,
    output_file: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = if log_mode {
        evaluations::singleturn::run_evaluation(websocket, provider, request, None)
            .await
            .map_err(|e| e.to_string())?
    } else {
        let (tx, rx) = tokio::sync::mpsc::channel::<SingleTurnProgressIndicatorMessage>(128);
        let render_handle = tokio::spawn(singleturn::render_task(
            rx,
            request.maximum_iteration_layers,
        ));

        let result =
            evaluations::singleturn::run_evaluation(websocket, provider, request, Some(tx))
                .await
                .map_err(|e| e.to_string())?;

        let _ = render_handle.await;
        result
    };

    let filename = output_file.unwrap_or_else(|| {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        PathBuf::from(format!(
            "circuit_breaker_labs_single_turn_evaluation_{}.json",
            timestamp
        ))
    });

    let json = serde_json::to_string_pretty(&result)?;
    std::fs::write(&filename, json)?;

    if log_mode {
        tracing::info!(
            "Saved full single-turn evaluation results to {}",
            filename.display()
        );
    } else {
        print_styled_success_message("single", &filename)?;
    }

    Ok(())
}

async fn run_multi_turn_evaluation(
    websocket: WebSocketConnection,
    provider: Arc<dyn ResponseProvider>,
    request: MultiTurnRequest,
    log_mode: bool,
    output_file: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = if log_mode {
        evaluations::multiturn::run_evaluation(websocket, provider, request, None)
            .await
            .map_err(|e| e.to_string())?
    } else {
        let (tx, rx) = tokio::sync::mpsc::channel::<MultiTurnProgressIndicatorMessage>(128);
        let render_handle = tokio::spawn(multiturn::render_task(rx));

        let result = evaluations::multiturn::run_evaluation(websocket, provider, request, Some(tx))
            .await
            .map_err(|e| e.to_string())?;

        let _ = render_handle.await;
        result
    };

    let filename = output_file.unwrap_or_else(|| {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        PathBuf::from(format!(
            "circuit_breaker_labs_multi_turn_evaluation_{}.json",
            timestamp
        ))
    });

    let json = serde_json::to_string_pretty(&result)?;
    std::fs::write(&filename, json)?;

    if log_mode {
        tracing::info!(
            "Saved full multi-turn evaluation results to {}",
            filename.display()
        );
    } else {
        print_styled_success_message("multi", &filename)?;
    }

    Ok(())
}

fn print_styled_success_message(
    turn_type: &str,
    filename: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    use ratatui::crossterm::queue;
    use std::io::{self, Write};

    let mut stdout = io::stdout();

    queue!(stdout, Print("Saved full "))?;
    queue!(stdout, Print(turn_type))?;
    queue!(stdout, Print("-turn evaluation results to "))?;

    queue!(stdout, SetForegroundColor(Color::Magenta))?;
    queue!(stdout, SetAttribute(Attribute::Bold))?;
    queue!(stdout, SetAttribute(Attribute::Italic))?;
    queue!(stdout, Print(filename.display().to_string()))?;
    queue!(stdout, SetAttribute(Attribute::Reset))?;
    queue!(stdout, SetForegroundColor(Color::Reset))?;

    queue!(stdout, Print("\n"))?;

    stdout.flush()?;

    Ok(())
}
