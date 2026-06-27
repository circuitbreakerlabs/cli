use crate::{cli, consts};

use super::{HttpApiError, build_http_url_with_pagination, http_get_json};

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

pub async fn run(
    command: &cli::ApiEvaluationsCommand,
    ws_base_url: &str,
    api_key: &str,
    log_mode: bool,
    json: bool,
) -> Result<(), HttpApiError> {
    let (endpoint, label) = if command.single_turn {
        (
            consts::endpoints::SINGLE_TURN_EVALUATIONS_ENDPOINT,
            "single-turn",
        )
    } else if command.multi_turn {
        (
            consts::endpoints::MULTI_TURN_EVALUATIONS_ENDPOINT,
            "multi-turn",
        )
    } else {
        unreachable!("clap requires one evaluation type");
    };

    let url = build_http_url_with_pagination(ws_base_url, endpoint, command.limit, command.offset)?;
    let results: Vec<HistoricEvaluationSummary> = http_get_json(&url, api_key).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else if log_mode {
        if results.is_empty() {
            tracing::info!("No historic {} evaluation results found.", label);
        }
        for result in &results {
            tracing::info!(
                "{} evaluation result: test_result_id={} evaluation_id={} test_case_id={} passed={} score={} created_at={}",
                label,
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
    use super::{HistoricEvaluationSummary, build_historic_evaluations_table, truncate_for_table};
    use crate::http_api::build_http_url_with_pagination;

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
