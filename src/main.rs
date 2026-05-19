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
use clap::{CommandFactory, Parser};
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

#[derive(Error, Debug)]
enum HttpApiError {
    #[error("HTTP request error: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Request failed with status {0}: {1}")]
    Status(u16, String),

    #[error("JSON serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
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

    if cli_args.monthly_quota {
        run_monthly_quota(
            &cli_args.cbl_api_base_url,
            &cli_args.cbl_api_key,
            cli_args.log_mode,
            cli_args.json,
        )
        .await?;
        print_update_warning_if_needed(cli_args.log_mode).await;
        return Ok(());
    }

    if cli_args.validate_api_key {
        run_validate_api_key(
            &cli_args.cbl_api_base_url,
            &cli_args.cbl_api_key,
            cli_args.log_mode,
            cli_args.json,
        )
        .await?;
        print_update_warning_if_needed(cli_args.log_mode).await;
        return Ok(());
    }

    if cli_args.test_case_groups {
        run_test_case_groups(
            &cli_args.cbl_api_base_url,
            &cli_args.cbl_api_key,
            cli_args.log_mode,
            cli_args.json,
        )
        .await?;
        print_update_warning_if_needed(cli_args.log_mode).await;
        return Ok(());
    }

    let Some(command) = cli_args.command else {
        cli::Args::command().print_help()?;
        println!();
        return Ok(());
    };
    let cli::Command::Eval { evaluation } = command;

    let provider_command = match &evaluation {
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
        (&evaluation).into(),
        &cli_args.cbl_api_key,
    )
    .await?;

    match evaluation {
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

fn http_base_url(ws_base_url: &str) -> String {
    if let Some(rest) = ws_base_url.strip_prefix("wss://") {
        format!("https://{rest}")
    } else if let Some(rest) = ws_base_url.strip_prefix("ws://") {
        format!("http://{rest}")
    } else {
        ws_base_url.to_string()
    }
}

fn build_http_url(ws_base_url: &str, endpoint: &str) -> String {
    format!(
        "{}/{}",
        http_base_url(ws_base_url).trim_end_matches('/'),
        endpoint.trim_start_matches('/')
    )
}

#[derive(serde::Deserialize, serde::Serialize)]
struct MonthlyQuotaResponse {
    generated_tests: i32,
    alloted_test_generations: i32,
}

fn format_with_commas(n: i32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

#[derive(tabled::Tabled)]
struct QuotaDisplay {
    #[tabled(rename = "Monthly Quota")]
    content: String,
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn build_quota_table(quota: &MonthlyQuotaResponse) -> String {
    const CONTENT_WIDTH: usize = 37;

    let pct = if quota.alloted_test_generations == 0 {
        0.0_f64
    } else {
        f64::from(quota.generated_tests) / f64::from(quota.alloted_test_generations) * 100.0
    };

    let generated_fmt = format_with_commas(quota.generated_tests);
    let limit_fmt = format_with_commas(quota.alloted_test_generations);
    let pct_label = format!("{pct:.0}% used");

    let bar_width = CONTENT_WIDTH.saturating_sub(pct_label.len() + 2);
    let filled = ((bar_width as f64 * pct / 100.0).round() as usize).min(bar_width);
    let empty = bar_width - filled;

    let numbers = format!("{generated_fmt} / {limit_fmt}");
    let label = "Generated tests";
    let stats_line = format!(
        "{label}{numbers:>width$}",
        width = CONTENT_WIDTH - label.len()
    );
    let bar_line = format!("{}{}  {pct_label}", "█".repeat(filled), "░".repeat(empty));

    tabled::Table::new([QuotaDisplay {
        content: format!("{stats_line}\n{bar_line}"),
    }])
    .with(tabled::settings::Style::modern())
    .to_string()
}

async fn http_get_json<T: serde::de::DeserializeOwned>(
    url: &str,
    api_key: &str,
) -> Result<T, HttpApiError> {
    let response = reqwest::Client::new()
        .get(url)
        .header(consts::headers::CBL_API_KEY, api_key)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(HttpApiError::Status(status, body));
    }

    Ok(response.json::<T>().await?)
}

async fn run_monthly_quota(
    ws_base_url: &str,
    api_key: &str,
    log_mode: bool,
    json: bool,
) -> Result<(), HttpApiError> {
    let url = build_http_url(ws_base_url, consts::endpoints::MONTHLY_QUOTA_ENDPOINT);
    let quota: MonthlyQuotaResponse = http_get_json(&url, api_key).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&quota)?);
    } else if log_mode {
        tracing::info!(
            "Monthly quota: {} / {} test generations used",
            quota.generated_tests,
            quota.alloted_test_generations,
        );
    } else {
        println!("{}", build_quota_table(&quota));
    }

    Ok(())
}

#[derive(serde::Deserialize, serde::Serialize)]
struct ValidateApiKeyResponse {
    valid: bool,
}

#[derive(tabled::Tabled)]
struct ValidateDisplay {
    #[tabled(rename = "API Key Status")]
    status: String,
}

async fn run_validate_api_key(
    ws_base_url: &str,
    api_key: &str,
    log_mode: bool,
    json: bool,
) -> Result<(), HttpApiError> {
    let url = build_http_url(ws_base_url, consts::endpoints::VALIDATE_API_KEY_ENDPOINT);
    let data: ValidateApiKeyResponse = http_get_json(&url, api_key).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&data)?);
    } else if log_mode {
        tracing::info!(
            "API key is {}",
            if data.valid { "valid" } else { "invalid" }
        );
    } else {
        let status = if data.valid {
            "✓ Valid".to_string()
        } else {
            "✗ Invalid".to_string()
        };
        println!(
            "{}",
            tabled::Table::new([ValidateDisplay { status }])
                .with(tabled::settings::Style::modern())
        );
    }

    Ok(())
}

#[derive(serde::Deserialize, serde::Serialize)]
struct TestCaseGroupItem {
    name: String,
    description: Option<String>,
}

#[derive(tabled::Tabled)]
struct TestCaseGroupDisplay {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Description")]
    description: String,
}

async fn run_test_case_groups(
    ws_base_url: &str,
    api_key: &str,
    log_mode: bool,
    json: bool,
) -> Result<(), HttpApiError> {
    let url = build_http_url(ws_base_url, consts::endpoints::TEST_CASE_GROUPS_ENDPOINT);
    let groups: Vec<TestCaseGroupItem> = http_get_json(&url, api_key).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&groups)?);
    } else if log_mode {
        if groups.is_empty() {
            tracing::info!("No test case groups found.");
        }
        for g in &groups {
            tracing::info!(
                "Test case group: {} — {}",
                g.name,
                g.description.as_deref().unwrap_or("no description")
            );
        }
    } else if groups.is_empty() {
        println!("No test case groups found.");
    } else {
        let rows: Vec<TestCaseGroupDisplay> = groups
            .into_iter()
            .map(|g| TestCaseGroupDisplay {
                name: g.name,
                description: g.description.unwrap_or_else(|| "—".to_string()),
            })
            .collect();
        println!(
            "{}",
            tabled::Table::new(rows).with(tabled::settings::Style::modern())
        );
    }

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
