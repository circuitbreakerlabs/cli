// Shared evaluation infrastructure is retained for adapter-independent tests,
// even when no selected target uses it in the binary.
mod cli;
#[cfg_attr(not(eval_targets), allow(dead_code))]
mod consts;
#[cfg_attr(not(eval_targets), allow(dead_code))]
mod evaluation_output;
#[cfg_attr(not(text_targets), allow(dead_code))]
mod evaluations;
mod http_api;
mod protocol_types;
#[cfg_attr(not(text_targets), allow(dead_code))]
mod response_provider;
#[cfg_attr(not(eval_targets), allow(dead_code))]
mod tui;
mod update_check;
#[cfg_attr(not(feature = "livekit"), allow(dead_code))]
mod voice;
#[cfg_attr(not(eval_targets), allow(dead_code))]
mod websockets;

#[cfg(eval_targets)]
use std::path::{Path, PathBuf};
#[cfg(text_targets)]
use std::sync::Arc;
use update_check::print_update_warning_if_needed;

#[cfg(eval_targets)]
use chrono::Local;
use clap::Parser;
#[cfg(eval_targets)]
use evaluation_output::{serialize_evaluation_output, serialize_rerun_evaluation_output};
#[cfg(eval_targets)]
use protocol_types::MultiTurnEvaluationRequest;
#[cfg(text_targets)]
use protocol_types::SingleTurnEvaluationRequest;
#[cfg(eval_targets)]
use ratatui::crossterm::style::{Attribute, Color, SetAttribute, SetForegroundColor};
#[cfg(feature = "custom")]
use response_provider::CustomProvider;
#[cfg(feature = "ollama")]
use response_provider::OllamaProvider;
#[cfg(feature = "openai")]
use response_provider::OpenAIProvider;
#[cfg(text_targets)]
use response_provider::ResponseProvider;
#[cfg(text_targets)]
use tui::{MultiTurnProgressIndicatorMessage, SingleTurnProgressIndicatorMessage};
#[cfg(eval_targets)]
use tui::{multiturn, singleturn};
#[cfg(text_targets)]
use websockets::WebSocketConnection;

#[cfg(text_targets)]
use evaluations::EvaluationError;
#[cfg(text_targets)]
use thiserror::Error;
use tracing_subscriber::prelude::*;

#[cfg(text_targets)]
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
    let mut cli_args = cli::Args::parse();
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

    let command = cli_args
        .command
        .take()
        .expect("validated CLI args should include a subcommand");

    match command {
        cli::Command::Api(api_command) => {
            http_api::handle(
                api_command,
                &cli_args.cbl_api_base_url,
                &cli_args.cbl_api_key,
                cli_args.log_mode,
            )
            .await?;
            print_update_warning_if_needed(cli_args.log_mode).await;
            Ok(())
        }
        #[cfg(eval_targets)]
        cli::Command::Eval { evaluation } => run_evaluation(evaluation, cli_args).await,
    }
}

#[cfg(eval_targets)]
#[allow(clippy::too_many_lines)]
async fn run_evaluation(
    evaluation: cli::EvaluationCommand,
    cli_args: cli::Args,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "livekit")]
    match evaluation {
        cli::EvaluationCommand::SingleTurn {
            provider: cli::ProviderCommand::Livekit { config },
            request,
        } => {
            let target = voice::prepare_livekit(&config, 1).map_err(|error| error.to_string())?;
            return run_single_turn_voice_cli(
                &cli_args.cbl_api_base_url,
                &cli_args.cbl_api_key,
                target,
                request.into(),
                cli_args.log_mode,
                cli_args.output_file,
            )
            .await;
        }
        cli::EvaluationCommand::MultiTurn {
            provider: cli::ProviderCommand::Livekit { config },
            request,
        } => {
            let request: MultiTurnEvaluationRequest = request.into();
            let target = voice::prepare_livekit(&config, request.max_turns())
                .map_err(|error| error.to_string())?;
            return run_voice_cli(
                &cli_args.cbl_api_base_url,
                &cli_args.cbl_api_key,
                target,
                request,
                cli_args.log_mode,
                cli_args.output_file,
            )
            .await;
        }
        cli::EvaluationCommand::ReRun {
            rerun:
                cli::ReRunEvaluationCommand::MultiTurn {
                    provider: cli::ProviderCommand::Livekit { config },
                    request,
                },
        } => {
            let request: MultiTurnEvaluationRequest = request.into();
            let target = voice::prepare_livekit(&config, request.max_turns())
                .map_err(|error| error.to_string())?;
            return run_voice_cli(
                &cli_args.cbl_api_base_url,
                &cli_args.cbl_api_key,
                target,
                request,
                cli_args.log_mode,
                cli_args.output_file,
            )
            .await;
        }
        #[cfg(text_targets)]
        _ => {}
    }

    #[cfg(text_targets)]
    {
        let headers = cli_args.headers();
        let provider_command = match &evaluation {
            cli::EvaluationCommand::SingleTurn { provider, .. }
            | cli::EvaluationCommand::MultiTurn { provider, .. } => provider.text(),
            cli::EvaluationCommand::ReRun { rerun } => match rerun {
                cli::ReRunEvaluationCommand::SingleTurn { provider, .. } => provider,
                cli::ReRunEvaluationCommand::MultiTurn { provider, .. } => provider.text(),
            },
        };

        let provider = match provider_command {
            #[cfg(feature = "ollama")]
            cli::TextProviderCommand::Ollama(config) => {
                Arc::new(OllamaProvider::new(config.clone(), &headers)?)
                    as Arc<dyn ResponseProvider>
            }
            #[cfg(feature = "openai")]
            cli::TextProviderCommand::OpenAI(config) => {
                Arc::new(OpenAIProvider::new(config.clone(), &headers)?)
                    as Arc<dyn ResponseProvider>
            }
            #[cfg(feature = "custom")]
            cli::TextProviderCommand::Custom(config) => {
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
    }
    #[cfg(text_targets)]
    Ok(())
}

#[cfg(text_targets)]
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

    print_success_message(log_mode, "single-turn", &filename);
    print_update_warning_if_needed(log_mode).await;

    Ok(())
}

#[cfg(text_targets)]
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

    print_success_message(log_mode, "multi-turn", &filename);
    print_update_warning_if_needed(log_mode).await;

    Ok(())
}

#[cfg(eval_targets)]
fn print_success_message(log_mode: bool, turn_type: &str, filename: &Path) {
    if log_mode {
        tracing::info!(
            "Saved full {} evaluation results to {}",
            turn_type,
            filename.display(),
        );
    } else {
        println!(
            "Saved full {} evaluation results to {}{}{}{}{}{}",
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

#[cfg(feature = "livekit")]
async fn run_single_turn_voice_cli(
    base_url: &str,
    key: &str,
    target: voice::PreparedTarget,
    request: protocol_types::SingleTurnEvaluationRequest,
    log_mode: bool,
    output: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let groups = request.test_case_groups().map(<[_]>::to_vec);
    let maximum_iteration_layers = request.maximum_iteration_layers();
    let websocket =
        websockets::connect(base_url, evaluations::EvaluationType::SingleTurnVoice, key).await?;
    let (progress, render) = if log_mode {
        (None, None)
    } else {
        let (tx, rx) = tokio::sync::mpsc::channel(128);
        (
            Some(tx),
            Some(tokio::spawn(singleturn::render_task(
                rx,
                maximum_iteration_layers,
            ))),
        )
    };
    let result = voice::run_single(websocket, target, request, progress).await;
    if let Some(render) = render {
        let _ = render.await;
    }
    let result = result?;
    let json =
        evaluation_output::serialize_evaluation_output(&result, &groups.unwrap_or_default())?;
    let filename = output.unwrap_or_else(|| {
        PathBuf::from(format!(
            "circuit_breaker_labs_single_turn_voice_evaluation_{}.json",
            Local::now().format("%Y%m%d_%H%M%S")
        ))
    });
    std::fs::write(&filename, json)?;
    print_success_message(log_mode, "single-turn voice", &filename);
    print_update_warning_if_needed(log_mode).await;
    Ok(())
}

#[cfg(feature = "livekit")]
async fn run_voice_cli(
    base_url: &str,
    key: &str,
    target: voice::PreparedTarget,
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
    let websocket = websockets::connect(base_url, kind, key).await?;
    let (progress, render) = if log_mode {
        (None, None)
    } else {
        let (tx, rx) = tokio::sync::mpsc::channel(128);
        (Some(tx), Some(tokio::spawn(multiturn::render_task(rx))))
    };
    let result = voice::run(websocket, target, request, progress).await;
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
    print_success_message(log_mode, "multi-turn voice", &filename);
    Ok(())
}
