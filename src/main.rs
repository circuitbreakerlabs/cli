mod cli;
mod consts;
mod evaluation_output;
mod evaluations;
mod http_api;
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
use evaluation_output::serialize_evaluation_output;
use evaluations::ResponseProviderRegistry;
use evaluations::multiturn::MultiTurnFinalResponse;
use evaluations::singleturn::SingleTurnFinalResponse;
use protocol_types::{MultiTurnRequest, SingleTurnRequest};
use ratatui::crossterm::style::{Attribute, Color, SetAttribute, SetForegroundColor};
use response_provider::{CustomProvider, OllamaProvider, OpenAIProvider, ResponseProvider};
use std::collections::{HashMap, HashSet};
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

    #[error("Provider error: {0}")]
    Provider(#[from] response_provider::ProviderError),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_args = cli::Args::parse();
    cli_args.validate().unwrap_or_else(|e| e.exit());

    if cli_args.log_mode {
        tracing_subscriber::fmt()
            .with_max_level(Into::<tracing::Level>::into(cli_args.log_level))
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

    let provider_command = match &evaluation {
        cli::EvaluationCommand::SingleTurn { provider, .. }
        | cli::EvaluationCommand::MultiTurn { provider, .. } => provider,
    };

    let (providers, target_models) = build_provider_registry(provider_command, &headers)?;

    let mut evaluation = evaluation;
    match &mut evaluation {
        cli::EvaluationCommand::SingleTurn { request, .. } => {
            request.target_models.clone_from(&target_models);
        }
        cli::EvaluationCommand::MultiTurn { request, .. } => {
            request.target_models.clone_from(&target_models);
        }
    }

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
                providers,
                request,
                cli_args.log_mode,
                cli_args.output_file,
            )
            .await?;
        }
        cli::EvaluationCommand::MultiTurn { request, .. } => {
            run_multi_turn_evaluation(
                websocket,
                providers,
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
    providers: ResponseProviderRegistry,
    request: SingleTurnRequest,
    log_mode: bool,
    output_file: Option<PathBuf>,
) -> Result<(), RunEvaluationError> {
    let test_case_groups = request.test_case_groups.clone();
    let result = if log_mode {
        evaluations::singleturn::run_evaluation(websocket, providers, request, None).await?
    } else {
        let (tx, rx) = tokio::sync::mpsc::channel::<SingleTurnProgressIndicatorMessage>(128);
        let render_handle = tokio::spawn(singleturn::render_task(
            rx,
            request.maximum_iteration_layers,
        ));

        let result =
            evaluations::singleturn::run_evaluation(websocket, providers, request, Some(tx))
                .await?;

        let _ = render_handle.await;
        result
    };

    let filename = output_file.unwrap_or_else(|| {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        PathBuf::from(format!(
            "circuit_breaker_labs_single_turn_evaluation_{timestamp}.json",
        ))
    });

    let saved_files = match result {
        SingleTurnFinalResponse::Single(result) => {
            let json = serialize_evaluation_output(&result, &test_case_groups)?;
            std::fs::write(&filename, json)?;
            vec![filename]
        }
        SingleTurnFinalResponse::Parallel(result) => {
            write_parallel_results(&filename, result.results.iter(), &test_case_groups)?
        }
    };

    print_success_messages(log_mode, "single", &saved_files);
    print_update_warning_if_needed(log_mode).await;

    Ok(())
}

async fn run_multi_turn_evaluation(
    websocket: WebSocketConnection,
    providers: ResponseProviderRegistry,
    request: MultiTurnRequest,
    log_mode: bool,
    output_file: Option<PathBuf>,
) -> Result<(), RunEvaluationError> {
    let test_case_groups = request.test_case_groups.clone();
    let result = if log_mode {
        evaluations::multiturn::run_evaluation(websocket, providers, request, None).await?
    } else {
        let (tx, rx) = tokio::sync::mpsc::channel::<MultiTurnProgressIndicatorMessage>(128);
        let render_handle = tokio::spawn(multiturn::render_task(rx));

        let result =
            evaluations::multiturn::run_evaluation(websocket, providers, request, Some(tx)).await?;

        let _ = render_handle.await;
        result
    };

    let filename = output_file.unwrap_or_else(|| {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        PathBuf::from(format!(
            "circuit_breaker_labs_multi_turn_evaluation_{timestamp}.json",
        ))
    });

    let saved_files = match result {
        MultiTurnFinalResponse::Single(result) => {
            let json = serialize_evaluation_output(&result, &test_case_groups)?;
            std::fs::write(&filename, json)?;
            vec![filename]
        }
        MultiTurnFinalResponse::Parallel(result) => {
            write_parallel_results(&filename, result.results.iter(), &test_case_groups)?
        }
    };

    print_success_messages(log_mode, "multi", &saved_files);
    print_update_warning_if_needed(log_mode).await;

    Ok(())
}

fn build_provider_registry(
    provider_command: &cli::ProviderCommand,
    headers: &reqwest::header::HeaderMap,
) -> Result<(ResponseProviderRegistry, Vec<String>), response_provider::ProviderError> {
    match provider_command {
        cli::ProviderCommand::Ollama(config) => {
            let model_ids = model_ids_without_duplicates(config.model_ids())?;
            if model_ids.len() == 1 {
                let provider = Arc::new(OllamaProvider::new(config.clone(), headers)?)
                    as Arc<dyn ResponseProvider>;
                Ok((ResponseProviderRegistry::single(provider), Vec::new()))
            } else {
                let mut providers = HashMap::new();
                for model_id in &model_ids {
                    let provider = Arc::new(OllamaProvider::new(
                        config.with_model(model_id.clone()),
                        headers,
                    )?) as Arc<dyn ResponseProvider>;
                    providers.insert(model_id.clone(), provider);
                }
                Ok((ResponseProviderRegistry::parallel(providers), model_ids))
            }
        }
        cli::ProviderCommand::OpenAI(config) => {
            let model_ids = model_ids_without_duplicates(config.model_ids())?;
            if model_ids.len() == 1 {
                let provider = Arc::new(OpenAIProvider::new(config.clone(), headers)?)
                    as Arc<dyn ResponseProvider>;
                Ok((ResponseProviderRegistry::single(provider), Vec::new()))
            } else {
                let mut providers = HashMap::new();
                for model_id in &model_ids {
                    let provider = Arc::new(OpenAIProvider::new(
                        config.with_model(model_id.clone()),
                        headers,
                    )?) as Arc<dyn ResponseProvider>;
                    providers.insert(model_id.clone(), provider);
                }
                Ok((ResponseProviderRegistry::parallel(providers), model_ids))
            }
        }
        cli::ProviderCommand::Custom(config) => {
            let provider =
                Arc::new(CustomProvider::new(config, headers)?) as Arc<dyn ResponseProvider>;
            Ok((ResponseProviderRegistry::single(provider), Vec::new()))
        }
    }
}

fn model_ids_without_duplicates(
    model_ids: &[String],
) -> Result<Vec<String>, response_provider::ProviderError> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();

    for model_id in model_ids {
        if !seen.insert(model_id) {
            return Err(response_provider::ProviderError::Config(format!(
                "Duplicate model '{model_id}' is not allowed in a parallel evaluation"
            )));
        }
        unique.push(model_id.clone());
    }

    Ok(unique)
}

fn write_parallel_results<'a, T, I>(
    base_filename: &Path,
    results: I,
    test_case_groups: &[protocol_types::common::TestCaseGroup],
) -> Result<Vec<PathBuf>, RunEvaluationError>
where
    T: serde::Serialize + 'a,
    I: IntoIterator<Item = (&'a String, &'a T)>,
{
    let mut saved_files = Vec::new();

    for (model_id, result) in results {
        let filename = model_output_filename(base_filename, model_id);
        let json = serialize_evaluation_output(result, test_case_groups)?;
        std::fs::write(&filename, json)?;
        saved_files.push(filename);
    }

    saved_files.sort();
    Ok(saved_files)
}

fn model_output_filename(base_filename: &Path, model_id: &str) -> PathBuf {
    let sanitized_model_id = sanitize_model_id_for_filename(model_id);
    let stem = base_filename
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("evaluation");
    let extension = base_filename.extension().and_then(|value| value.to_str());
    let filename = match extension {
        Some(extension) if !extension.is_empty() => {
            format!("{stem}_{sanitized_model_id}.{extension}")
        }
        _ => format!("{stem}_{sanitized_model_id}"),
    };

    base_filename.with_file_name(filename)
}

fn sanitize_model_id_for_filename(model_id: &str) -> String {
    model_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn print_success_messages(log_mode: bool, turn_type: &str, filenames: &[PathBuf]) {
    for filename in filenames {
        print_success_message(log_mode, turn_type, filename);
    }
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

#[cfg(test)]
mod tests {
    use super::{model_output_filename, write_parallel_results};
    use serde::Serialize;
    use serde_json::json;
    use std::collections::HashMap;

    #[derive(Serialize)]
    struct TestResult {
        total_passed: i32,
        total_failed: i32,
    }

    #[test]
    fn model_output_filename_appends_sanitized_model_id_before_extension() {
        let filename = model_output_filename(
            std::path::Path::new("/tmp/evaluation.json"),
            "openai/gpt-4.1:nano",
        );

        assert_eq!(
            filename,
            std::path::PathBuf::from("/tmp/evaluation_openai_gpt-4.1_nano.json")
        );
    }

    #[test]
    fn write_parallel_results_writes_one_json_per_model() {
        let base_filename = std::env::temp_dir().join(format!(
            "cbl_parallel_output_test_{}.json",
            std::process::id()
        ));
        let results = HashMap::from([
            (
                "model-a".to_string(),
                TestResult {
                    total_passed: 1,
                    total_failed: 0,
                },
            ),
            (
                "model/b".to_string(),
                TestResult {
                    total_passed: 0,
                    total_failed: 1,
                },
            ),
        ]);

        let saved_files = write_parallel_results(
            &base_filename,
            results.iter(),
            &["suicidal_ideation".to_string()],
        )
        .expect("parallel results should write");

        assert_eq!(saved_files.len(), 2);
        assert!(saved_files.iter().any(|path| path.ends_with(format!(
            "cbl_parallel_output_test_{}_model-a.json",
            std::process::id()
        ))));
        assert!(saved_files.iter().any(|path| path.ends_with(format!(
            "cbl_parallel_output_test_{}_model_b.json",
            std::process::id()
        ))));

        for path in saved_files {
            let value: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(&path).expect("result file should be readable"),
            )
            .expect("result file should contain json");
            assert_eq!(value["test_case_groups"], json!(["suicidal_ideation"]));
            std::fs::remove_file(path).expect("test output file should be removable");
        }
    }
}
