use crate::{cli, consts};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HttpApiError {
    #[error("HTTP request error: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Request failed with status {0}: {1}")]
    Status(u16, String),

    #[error("JSON serialization error: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),
}

pub async fn handle(
    command: cli::ApiCommand,
    ws_base_url: &str,
    api_key: &str,
    log_mode: bool,
) -> Result<(), HttpApiError> {
    if command.query.monthly_quota {
        return run_monthly_quota(ws_base_url, api_key, log_mode, command.json).await;
    }

    if command.query.validate_api_key {
        return run_validate_api_key(ws_base_url, api_key, log_mode, command.json).await;
    }

    if command.query.test_case_groups {
        return run_test_case_groups(ws_base_url, api_key, log_mode, command.json).await;
    }

    if command.query.single_turn_evaluations {
        return run_historic_evaluations(HistoricEvaluationsRequest {
            ws_base_url,
            api_key,
            log_mode,
            json: command.json,
            endpoint: consts::endpoints::SINGLE_TURN_EVALUATIONS_ENDPOINT,
            label: "single-turn",
            limit: command.query.limit,
            offset: command.query.offset,
        })
        .await;
    }

    if command.query.multi_turn_evaluations {
        return run_historic_evaluations(HistoricEvaluationsRequest {
            ws_base_url,
            api_key,
            log_mode,
            json: command.json,
            endpoint: consts::endpoints::MULTI_TURN_EVALUATIONS_ENDPOINT,
            label: "multi-turn",
            limit: command.query.limit,
            offset: command.query.offset,
        })
        .await;
    }

    unreachable!("clap requires one API query flag");
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

fn build_http_url_with_pagination(
    ws_base_url: &str,
    endpoint: &str,
    limit: Option<u16>,
    offset: Option<u32>,
) -> Result<String, HttpApiError> {
    let mut url = url::Url::parse(&build_http_url(ws_base_url, endpoint))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("limit", &limit.unwrap_or(50).to_string());
        pairs.append_pair("offset", &offset.unwrap_or(0).to_string());
    }
    Ok(url.to_string())
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

#[derive(serde::Deserialize, serde::Serialize)]
struct HistoricEvaluationSummary {
    test_result_id: i64,
    evaluation_id: i64,
    test_case_id: Option<i64>,
    initial_user_input: Option<String>,
    passed: Option<bool>,
    score: Option<f64>,
    model_response: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(tabled::Tabled)]
struct HistoricEvaluationDisplay {
    #[tabled(rename = "Created")]
    created: String,
    #[tabled(rename = "Result ID")]
    test_result_id: i64,
    #[tabled(rename = "Eval ID")]
    evaluation_id: i64,
    #[tabled(rename = "Case ID")]
    test_case_id: String,
    #[tabled(rename = "Passed")]
    passed: String,
    #[tabled(rename = "Score")]
    score: String,
    #[tabled(rename = "Input")]
    input: String,
    #[tabled(rename = "Response")]
    response: String,
}

fn format_optional_i64(value: Option<i64>) -> String {
    value.map_or_else(|| "-".to_string(), |v| v.to_string())
}

fn format_optional_bool(value: Option<bool>) -> String {
    value.map_or_else(
        || "-".to_string(),
        |v| {
            if v {
                "true".to_string()
            } else {
                "false".to_string()
            }
        },
    )
}

fn format_optional_score(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_string(), |v| format!("{v:.3}"))
}

fn truncate_for_table(value: Option<&str>, max_chars: usize) -> String {
    let Some(value) = value else {
        return "-".to_string();
    };
    let value = value.replace(['\r', '\n'], " ");
    if value.chars().count() <= max_chars {
        return value;
    }
    let mut truncated = value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

fn build_historic_evaluations_table(results: &[HistoricEvaluationSummary]) -> String {
    if results.is_empty() {
        return "No historic evaluation results found.".to_string();
    }

    let rows = results.iter().map(|result| HistoricEvaluationDisplay {
        created: result.created_at.to_rfc3339(),
        test_result_id: result.test_result_id,
        evaluation_id: result.evaluation_id,
        test_case_id: format_optional_i64(result.test_case_id),
        passed: format_optional_bool(result.passed),
        score: format_optional_score(result.score),
        input: truncate_for_table(result.initial_user_input.as_deref(), 80),
        response: truncate_for_table(result.model_response.as_deref(), 80),
    });

    tabled::Table::new(rows)
        .with(tabled::settings::Style::modern())
        .to_string()
}

struct HistoricEvaluationsRequest<'a> {
    ws_base_url: &'a str,
    api_key: &'a str,
    log_mode: bool,
    json: bool,
    endpoint: &'a str,
    label: &'a str,
    limit: Option<u16>,
    offset: Option<u32>,
}

async fn run_historic_evaluations(
    request: HistoricEvaluationsRequest<'_>,
) -> Result<(), HttpApiError> {
    let url = build_http_url_with_pagination(
        request.ws_base_url,
        request.endpoint,
        request.limit,
        request.offset,
    )?;
    let results: Vec<HistoricEvaluationSummary> = http_get_json(&url, request.api_key).await?;

    if request.json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else if request.log_mode {
        if results.is_empty() {
            tracing::info!("No historic {} evaluation results found.", request.label);
        }
        for result in &results {
            tracing::info!(
                "{} evaluation result: test_result_id={} evaluation_id={} test_case_id={} passed={} score={} created_at={}",
                request.label,
                result.test_result_id,
                result.evaluation_id,
                format_optional_i64(result.test_case_id),
                format_optional_bool(result.passed),
                format_optional_score(result.score),
                result.created_at.to_rfc3339(),
            );
        }
    } else {
        println!("{}", build_historic_evaluations_table(&results));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        HistoricEvaluationSummary, build_historic_evaluations_table,
        build_http_url_with_pagination, truncate_for_table,
    };

    #[test]
    fn builds_paginated_http_url_with_defaults() {
        let url = build_http_url_with_pagination("wss://api.example.test/v1", "/items", None, None)
            .expect("url should build");

        assert_eq!(url, "https://api.example.test/v1/items?limit=50&offset=0");
    }

    #[test]
    fn builds_paginated_http_url_with_overrides() {
        let url =
            build_http_url_with_pagination("ws://api.example.test/v1/", "items", Some(10), Some(5))
                .expect("url should build");

        assert_eq!(url, "http://api.example.test/v1/items?limit=10&offset=5");
    }

    #[test]
    fn truncates_table_text() {
        assert_eq!(truncate_for_table(Some("abcdef"), 4), "abc…");
        assert_eq!(truncate_for_table(Some("ab\ncd"), 10), "ab cd");
        assert_eq!(truncate_for_table(None, 10), "-");
    }

    #[test]
    fn historic_evaluations_table_handles_empty_results() {
        assert_eq!(
            build_historic_evaluations_table(&[]),
            "No historic evaluation results found."
        );
    }

    #[test]
    fn historic_evaluations_table_formats_optional_fields() {
        let result = HistoricEvaluationSummary {
            test_result_id: 42,
            evaluation_id: 7,
            test_case_id: None,
            initial_user_input: Some("hello".to_string()),
            passed: Some(true),
            score: Some(0.1234),
            model_response: None,
            created_at: "2026-06-20T18:00:00Z"
                .parse()
                .expect("timestamp should parse"),
        };

        let table = build_historic_evaluations_table(&[result]);

        assert!(table.contains("42"));
        assert!(table.contains('7'));
        assert!(table.contains("true"));
        assert!(table.contains("0.123"));
        assert!(table.contains("hello"));
        assert!(table.contains('-'));
    }
}
