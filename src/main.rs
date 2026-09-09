mod cli;
mod consts;
mod evaluation_output;
mod evaluations;
mod http_api;
mod protocol_types;
mod response_provider;
mod tui;
mod update_check;
mod voice;
mod websockets;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use update_check::print_update_warning_if_needed;

use chrono::Local;
use clap::Parser;
use evaluation_output::{serialize_evaluation_output, serialize_rerun_evaluation_output};
use protocol_types::{MultiTurnEvaluationRequest, SingleTurnEvaluationRequest};
use ratatui::crossterm::style::{Attribute, Color, SetAttribute, SetForegroundColor};
use response_provider::{CustomProvider, OllamaProvider, OpenAIProvider, ResponseProvider};
use tui::{
    MultiTurnProgressIndicatorMessage, SingleTurnProgressIndicatorMessage, multiturn, singleturn,
};
use websockets::WebSocketConnection;

use evaluations::EvaluationError;
use thiserror::Error;
use tracing_subscriber::prelude::*;

#[derive(Error, Debug)]
enum RunEvaluationError {
    #[error("Evaluation error: {0}")]
    Evaluation(#[from] EvaluationError),

    #[error("Result Serialization error: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("Result save error: {0}")]
    FileSave(#[from] std::io::Error),
}

#[allow(clippy::too_many_lines)] // Dispatch text and voice without changing existing command semantics.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_args = cli::Args::parse();
    cli_args.validate().unwrap_or_else(|e| e.exit());

    if cli_args.log_mode {
        let level: tracing::Level = cli_args.log_level.into();
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                        // Native SDK diagnostics can include customer URLs or tokens.
                        !["livekit", "libwebrtc", "webrtc"]
                            .iter()
                            .any(|prefix| metadata.target().starts_with(prefix))
                    }))
                    .with_filter(tracing_subscriber::filter::LevelFilter::from_level(level)),
            )
            .init();
    }

    let headers = cli_args.headers();
    let command = cli_args
        .command
        .expect("validated CLI args should include a subcommand");

    let evaluation = match command {
        cli::Command::Api(api_command) => {
            http_api::handle(
                api_command,
                &cli_args.cbl_api_base_url,
                &cli_args.cbl_api_key,
                cli_args.log_mode,
            )
            .await?;
            print_update_warning_if_needed(cli_args.log_mode).await;
            return Ok(());
        }
        cli::Command::Eval { evaluation } => evaluation,
    };

    match evaluation {
        cli::EvaluationCommand::Voice { provider, request } => {
            return run_voice_cli(
                &cli_args.cbl_api_base_url,
                &cli_args.cbl_api_key,
                provider,
                request.into(),
                cli_args.log_mode,
                cli_args.output_file,
            )
            .await;
        }
        cli::EvaluationCommand::ReRun {
            rerun: cli::ReRunEvaluationCommand::Voice { provider, request },
        } => {
            return run_voice_cli(
                &cli_args.cbl_api_base_url,
                &cli_args.cbl_api_key,
                provider,
                request.into(),
                cli_args.log_mode,
                cli_args.output_file,
            )
            .await;
        }
        _ => {}
    }

    let provider_command = match &evaluation {
        cli::EvaluationCommand::Voice { .. } => unreachable!("voice dispatched above"),
        cli::EvaluationCommand::SingleTurn { provider, .. }
        | cli::EvaluationCommand::MultiTurn { provider, .. } => provider,
        cli::EvaluationCommand::ReRun { rerun } => match rerun {
            cli::ReRunEvaluationCommand::Voice { .. } => unreachable!("voice dispatched above"),
            cli::ReRunEvaluationCommand::SingleTurn { provider, .. }
            | cli::ReRunEvaluationCommand::MultiTurn { provider, .. } => provider,
        },
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
        (&evaluation).into(),
        &cli_args.cbl_api_key,
    )
    .await?;

    match evaluation {
        cli::EvaluationCommand::Voice { .. } => unreachable!("voice dispatched above"),
        cli::EvaluationCommand::SingleTurn { request, .. } => {
            run_single_turn_evaluation(
                websocket,
                provider,
                request.into(),
                cli_args.log_mode,
                cli_args.output_file,
            )
            .await?;
        }
        cli::EvaluationCommand::MultiTurn { request, .. } => {
            run_multi_turn_evaluation(
                websocket,
                provider,
                request.into(),
                cli_args.log_mode,
                cli_args.output_file,
            )
            .await?;
        }
        cli::EvaluationCommand::ReRun { rerun } => match rerun {
            cli::ReRunEvaluationCommand::Voice { .. } => unreachable!("voice dispatched above"),
            cli::ReRunEvaluationCommand::SingleTurn { request, .. } => {
                run_single_turn_evaluation(
                    websocket,
                    provider,
                    request.into(),
                    cli_args.log_mode,
                    cli_args.output_file,
                )
                .await?;
            }
            cli::ReRunEvaluationCommand::MultiTurn { request, .. } => {
                run_multi_turn_evaluation(
                    websocket,
                    provider,
                    request.into(),
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
    request: SingleTurnEvaluationRequest,
    log_mode: bool,
    output_file: Option<PathBuf>,
) -> Result<(), RunEvaluationError> {
    let test_case_groups = request.test_case_groups().map(<[_]>::to_vec);
    let rerun_selector = request.rerun_selector();
    let maximum_iteration_layers = request.maximum_iteration_layers();
    let result = if log_mode {
        evaluations::singleturn::run_evaluation(websocket, provider, request, None).await?
    } else {
        let (tx, rx) = tokio::sync::mpsc::channel::<SingleTurnProgressIndicatorMessage>(128);
        let render_handle = tokio::spawn(singleturn::render_task(rx, maximum_iteration_layers));

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

    let json = if let Some(rerun_selector) = rerun_selector {
        serialize_rerun_evaluation_output(&result, &rerun_selector)?
    } else {
        serialize_evaluation_output(
            &result,
            &test_case_groups.expect("standard evaluation includes test case groups"),
        )?
    };
    std::fs::write(&filename, json)?;

    print_success_message(log_mode, "single", &filename);
    print_update_warning_if_needed(log_mode).await;

    Ok(())
}

async fn run_multi_turn_evaluation(
    websocket: WebSocketConnection,
    provider: Arc<dyn ResponseProvider>,
    request: MultiTurnEvaluationRequest,
    log_mode: bool,
    output_file: Option<PathBuf>,
) -> Result<(), RunEvaluationError> {
    let test_case_groups = request.test_case_groups().map(<[_]>::to_vec);
    let rerun_selector = request.rerun_selector();
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

    let json = if let Some(rerun_selector) = rerun_selector {
        serialize_rerun_evaluation_output(&result, &rerun_selector)?
    } else {
        serialize_evaluation_output(
            &result,
            &test_case_groups.expect("standard evaluation includes test case groups"),
        )?
    };
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

#[cfg(not(all(feature = "voice", not(target_env = "musl"))))]
#[allow(clippy::unused_async)] // Keep the same dispatch interface in text-only builds.
async fn run_voice_cli(
    _base_url: &str,
    _key: &str,
    _provider: voice::ProviderCommand,
    _request: MultiTurnEvaluationRequest,
    _log_mode: bool,
    _output: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    Err("This build does not include voice. Install the voice-enabled GNU Linux, macOS, or Windows build, or build with --features voice.".into())
}

#[cfg(all(feature = "voice", not(target_env = "musl")))]
async fn run_voice_cli(
    base_url: &str,
    key: &str,
    provider: voice::ProviderCommand,
    request: MultiTurnEvaluationRequest,
    log_mode: bool,
    output: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let groups = request.test_case_groups().map(<[_]>::to_vec);
    let selector = request.rerun_selector();
    let kind = if selector.is_some() {
        evaluations::EvaluationType::VoiceRerun
    } else {
        evaluations::EvaluationType::Voice
    };
    let voice::ProviderCommand::Livekit { config } = provider;
    let websocket = websockets::connect(base_url, kind, key).await?;
    let (progress, render) = if log_mode {
        (None, None)
    } else {
        let (tx, rx) = tokio::sync::mpsc::channel(128);
        (Some(tx), Some(tokio::spawn(multiturn::render_task(rx))))
    };
    let result = voice::run(websocket, &config, request, progress).await;
    if let Some(render) = render {
        let _ = render.await;
    }
    let result = result?;
    let json = if let Some(selector) = selector {
        serialize_rerun_evaluation_output(&result, &selector)?
    } else {
        serialize_evaluation_output(&result, &groups.unwrap_or_default())?
    };
    let filename = output.unwrap_or_else(|| {
        PathBuf::from(format!(
            "circuit_breaker_labs_voice_evaluation_{}.json",
            Local::now().format("%Y%m%d_%H%M%S")
        ))
    });
    std::fs::write(&filename, json)?;
    print_success_message(log_mode, "voice multi", &filename);
    Ok(())
}
