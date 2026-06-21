use super::headers::Headers;
use crate::protocol_types::{MultiTurnEvalRequest, SingleTurnEvalRequest};
use crate::response_provider::{CustomProviderConfig, OllamaProviderConfig, OpenAIProviderConfig};
use clap::{ArgGroup, CommandFactory, Parser, Subcommand, ValueEnum};
use reqwest::header::HeaderMap;
use std::path::PathBuf;

#[allow(clippy::struct_excessive_bools)]
#[derive(Parser, Debug)]
#[command(
    version = crate::cli::version::VERSION,
    long_version = crate::cli::version::VERSION,
    about = crate::cli::about::ABOUT,
    long_about = crate::cli::about::LONG_ABOUT,
    arg_required_else_help = true
)]
pub struct Args {
    /// Circuit Breaker Labs API key
    #[arg(long, env = "CBL_API_KEY")]
    pub cbl_api_key: String,

    /// Circuit Breaker Labs API base URL
    #[arg(
        long,
        env = "CBL_API_BASE_URL",
        default_value = crate::consts::endpoints::CBL_BASE_URL
    )]
    pub cbl_api_base_url: String,

    /// Logging level
    #[arg(long, value_enum, default_value_t = LogLevel::Info)]
    pub log_level: LogLevel,

    /// Enable log mode (disables TUI, outputs logs to stdout)
    #[arg(long)]
    pub log_mode: bool,

    /// Output file path for evaluation results (default: auto-generated with timestamp)
    #[arg(long)]
    pub output_file: Option<PathBuf>,

    /// Add custom headers to provider requests (format: "Key:Value", can be repeated)
    #[arg(long = "add-header", value_parser = clap::value_parser!(Headers))]
    headers: Vec<Headers>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Args {
    pub fn headers(&self) -> HeaderMap {
        super::headers::merge_headers(&self.headers)
    }

    pub fn validate(&self) -> Result<(), clap::Error> {
        if self.command.is_none() {
            return Err(Self::command().error(
                clap::error::ErrorKind::MissingSubcommand,
                "an eval or api subcommand is required",
            ));
        }

        Ok(())
    }
}

#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
    /// Query the Circuit Breaker Labs API
    Api(ApiCommand),

    /// Run evaluations
    Eval {
        #[command(subcommand)]
        evaluation: EvaluationCommand,
    },
}

#[derive(clap::Args, Debug)]
#[command(group(
    ArgGroup::new("api_queries")
        .args([
            "monthly_quota",
            "validate_api_key",
            "test_case_groups",
            "single_turn_evaluations",
            "multi_turn_evaluations",
        ])
        .required(true)
))]
pub struct ApiCommand {
    #[command(flatten)]
    pub query: ApiQueryCommand,

    /// Output result in JSON format
    #[arg(long, short = 'J', requires = "api_queries")]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct ApiQueryCommand {
    /// Display monthly quota usage
    #[arg(long = "monthly-quota", short = 'M')]
    pub monthly_quota: bool,

    /// Validate the API key
    #[arg(long = "validate-api-key", short = 'A')]
    pub validate_api_key: bool,

    /// List accessible test case groups
    #[arg(long = "test-case-groups", short = 'T')]
    pub test_case_groups: bool,

    /// List historic single-turn evaluation results
    #[arg(long = "single-turn-evaluations")]
    pub single_turn_evaluations: bool,

    /// List historic multi-turn evaluation results
    #[arg(long = "multi-turn-evaluations")]
    pub multi_turn_evaluations: bool,

    /// Maximum number of historic evaluation results to return
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..=100))]
    pub limit: Option<u16>,

    /// Number of historic evaluation results to skip
    #[arg(long)]
    pub offset: Option<u32>,
}

#[derive(Subcommand, Debug)]
pub enum EvaluationCommand {
    /// Run single-turn evaluation
    SingleTurn {
        #[command(subcommand)]
        provider: ProviderCommand,
        #[command(flatten)]
        request: SingleTurnEvalRequest,
    },

    /// Run multi-turn evaluation
    MultiTurn {
        #[command(subcommand)]
        provider: ProviderCommand,
        #[command(flatten)]
        request: MultiTurnEvalRequest,
    },
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lowercase")]
pub enum ProviderCommand {
    /// Use Ollama provider
    Ollama(OllamaProviderConfig),
    #[allow(clippy::doc_markdown)]
    /// Use OpenAI provider
    OpenAI(OpenAIProviderConfig),
    /// Use Custom Rhai-scripted provider
    Custom(CustomProviderConfig),
}

#[derive(Clone, Copy, ValueEnum, Debug)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<LogLevel> for tracing::Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => tracing::Level::ERROR,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Trace => tracing::Level::TRACE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Args;
    use clap::{Parser, error::ErrorKind};

    #[test]
    fn parses_valid_single_turn_command() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "2",
            "--test-case-groups",
            "suicidal_ideation",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect("single-turn args should parse");

        #[allow(clippy::match_wildcard_for_single_variants)]
        match args.command {
            Some(super::Command::Eval { evaluation }) => match evaluation {
                super::EvaluationCommand::SingleTurn { request, .. } => {
                    assert!((request.threshold - 0.5).abs() < f32::EPSILON);
                    assert_eq!(request.variations, 2);
                    assert_eq!(request.maximum_iteration_layers, 2);
                    assert_eq!(
                        request.test_case_groups,
                        Some(vec!["suicidal_ideation".to_string()])
                    );
                }
                super::EvaluationCommand::MultiTurn { .. } => {
                    panic!("expected single-turn command")
                }
            },
            _ => panic!("expected eval command"),
        }
    }

    #[test]
    fn rejects_legacy_top_level_evaluation_commands() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "2",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("legacy top-level evaluation command should be rejected");

        assert_eq!(err.kind(), ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn rejects_out_of_range_threshold() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "single-turn",
            "--threshold",
            "1.5",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "2",
            "--test-case-groups",
            "suicidal_ideation",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("threshold should be rejected");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(
            err.to_string()
                .contains("expected a number between 0 and 1")
        );
    }

    #[test]
    fn rejects_negative_threshold() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "single-turn",
            "--threshold",
            "-0.1",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "2",
            "--test-case-groups",
            "suicidal_ideation",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("negative threshold should be rejected");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(
            err.to_string()
                .contains("expected a number between 0 and 1")
        );
    }

    #[test]
    fn rejects_non_positive_variations() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "0",
            "--maximum-iteration-layers",
            "2",
            "--test-case-groups",
            "suicidal_ideation",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("variations should be rejected");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(
            err.to_string()
                .contains("expected an integer between 1 and 5")
        );
    }

    #[test]
    fn rejects_variations_above_spec_maximum() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "6",
            "--maximum-iteration-layers",
            "2",
            "--test-case-groups",
            "suicidal_ideation",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("variations above spec maximum should be rejected");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(
            err.to_string()
                .contains("expected an integer between 1 and 5")
        );
    }

    #[test]
    fn allows_zero_maximum_iteration_layers() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "0",
            "--test-case-groups",
            "suicidal_ideation",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect("zero iteration layers should parse");

        #[allow(clippy::match_wildcard_for_single_variants)]
        match args.command {
            Some(super::Command::Eval { evaluation }) => match evaluation {
                super::EvaluationCommand::SingleTurn { request, .. } => {
                    assert_eq!(request.maximum_iteration_layers, 0);
                }
                super::EvaluationCommand::MultiTurn { .. } => {
                    panic!("expected single-turn command")
                }
            },
            _ => panic!("expected eval command"),
        }
    }

    #[test]
    fn rejects_maximum_iteration_layers_above_spec_maximum() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "3",
            "--test-case-groups",
            "suicidal_ideation",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("maximum_iteration_layers above spec maximum should be rejected");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(
            err.to_string()
                .contains("expected an integer between 0 and 2")
        );
    }

    #[test]
    fn rejects_negative_maximum_iteration_layers() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "-1",
            "--test-case-groups",
            "suicidal_ideation",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("negative maximum_iteration_layers should be rejected");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(
            err.to_string()
                .contains("expected an integer between 0 and 2")
        );
    }

    #[test]
    fn parses_valid_multi_turn_command() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "multi-turn",
            "--threshold",
            "0.5",
            "--max-turns",
            "4",
            "--test-case-groups",
            "suicidal_ideation",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect("multi-turn args should parse");

        #[allow(clippy::match_wildcard_for_single_variants)]
        match args.command {
            Some(super::Command::Eval { evaluation }) => match evaluation {
                super::EvaluationCommand::MultiTurn { request, .. } => {
                    assert!((request.threshold - 0.5).abs() < f32::EPSILON);
                    assert_eq!(request.max_turns, 4);
                    assert_eq!(
                        request.test_case_groups,
                        Some(vec!["suicidal_ideation".to_string()])
                    );
                }
                super::EvaluationCommand::SingleTurn { .. } => {
                    panic!("expected multi-turn command")
                }
            },
            _ => panic!("expected eval command"),
        }
    }

    #[test]
    fn rejects_odd_max_turns() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "multi-turn",
            "--threshold",
            "0.5",
            "--max-turns",
            "3",
            "--test-case-groups",
            "suicidal_ideation",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("odd max_turns should be rejected");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(
            err.to_string()
                .contains("expected an even integer between 2 and 20")
        );
    }

    #[test]
    fn rejects_max_turns_above_spec_maximum() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "multi-turn",
            "--threshold",
            "0.5",
            "--max-turns",
            "22",
            "--test-case-groups",
            "suicidal_ideation",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("max_turns above spec maximum should be rejected");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(
            err.to_string()
                .contains("expected an even integer between 2 and 20")
        );
    }

    #[test]
    fn rejects_max_turns_below_spec_minimum() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "multi-turn",
            "--threshold",
            "0.5",
            "--max-turns",
            "0",
            "--test-case-groups",
            "suicidal_ideation",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("max_turns below spec minimum should be rejected");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(
            err.to_string()
                .contains("expected an even integer between 2 and 20")
        );
    }

    #[test]
    fn rejects_missing_single_turn_test_case_groups() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "1",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("missing test case groups should be rejected");

        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
        assert!(err.to_string().contains("--test-case-groups"));
    }

    #[test]
    fn parses_valid_single_turn_rerun_command() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "1",
            "--test-result-id",
            "42",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect("single-turn rerun args should parse");

        match args.command {
            Some(super::Command::Eval { evaluation }) => match evaluation {
                super::EvaluationCommand::SingleTurn { request, .. } => {
                    assert_eq!(request.test_result_id, Some(42));
                    assert!(request.test_case_groups.is_none());
                }
                super::EvaluationCommand::MultiTurn { .. } => {
                    panic!("expected single-turn command")
                }
            },
            _ => panic!("expected eval command"),
        }
    }

    #[test]
    fn rejects_single_turn_with_test_case_groups_and_test_result_id() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "1",
            "--test-case-groups",
            "suicidal_ideation",
            "--test-result-id",
            "42",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("single-turn source flags should conflict");

        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn rejects_non_positive_single_turn_test_result_id() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "1",
            "--test-result-id",
            "0",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("test_result_id should be rejected");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(err.to_string().contains("expected an integer >= 1"));
    }

    #[test]
    fn rejects_missing_multi_turn_test_case_groups() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "multi-turn",
            "--threshold",
            "0.5",
            "--max-turns",
            "4",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("missing test case groups should be rejected");

        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
        assert!(err.to_string().contains("--test-case-groups"));
    }

    #[test]
    fn parses_valid_multi_turn_rerun_command() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "multi-turn",
            "--threshold",
            "0.5",
            "--max-turns",
            "4",
            "--test-result-id",
            "42",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect("multi-turn rerun args should parse");

        match args.command {
            Some(super::Command::Eval { evaluation }) => match evaluation {
                super::EvaluationCommand::MultiTurn { request, .. } => {
                    assert_eq!(request.test_result_id, Some(42));
                    assert!(request.test_case_groups.is_none());
                }
                super::EvaluationCommand::SingleTurn { .. } => {
                    panic!("expected multi-turn command")
                }
            },
            _ => panic!("expected eval command"),
        }
    }

    #[test]
    fn rejects_multi_turn_with_test_case_groups_and_test_result_id() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "multi-turn",
            "--threshold",
            "0.5",
            "--max-turns",
            "4",
            "--test-case-groups",
            "suicidal_ideation",
            "--test-result-id",
            "42",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("multi-turn source flags should conflict");

        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn rejects_empty_single_turn_test_case_groups() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "1",
            "--test-case-groups",
            "",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("empty test case groups should be rejected");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(err.to_string().contains("non-empty test case group"));
    }

    #[test]
    fn rejects_empty_multi_turn_test_case_groups() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "multi-turn",
            "--threshold",
            "0.5",
            "--max-turns",
            "4",
            "--test-case-groups",
            "",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("empty test case groups should be rejected");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(err.to_string().contains("non-empty test case group"));
    }

    #[test]
    fn parses_api_monthly_quota_flag() {
        let args = Args::try_parse_from(["cbl", "--cbl-api-key", "key", "api", "--monthly-quota"])
            .expect("api --monthly-quota should parse");
        match args.command {
            Some(super::Command::Api(api)) => assert!(api.query.monthly_quota),
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn parses_api_monthly_quota_short_flag() {
        let args = Args::try_parse_from(["cbl", "--cbl-api-key", "key", "api", "-M"])
            .expect("api -M should parse");
        match args.command {
            Some(super::Command::Api(api)) => assert!(api.query.monthly_quota),
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn parses_api_validate_api_key_flag() {
        let args =
            Args::try_parse_from(["cbl", "--cbl-api-key", "key", "api", "--validate-api-key"])
                .expect("api --validate-api-key should parse");
        match args.command {
            Some(super::Command::Api(api)) => assert!(api.query.validate_api_key),
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn parses_api_validate_api_key_short_flag() {
        let args = Args::try_parse_from(["cbl", "--cbl-api-key", "key", "api", "-A"])
            .expect("api -A should parse");
        match args.command {
            Some(super::Command::Api(api)) => assert!(api.query.validate_api_key),
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn parses_api_test_case_groups_flag() {
        let args =
            Args::try_parse_from(["cbl", "--cbl-api-key", "key", "api", "--test-case-groups"])
                .expect("api --test-case-groups should parse");
        match args.command {
            Some(super::Command::Api(api)) => assert!(api.query.test_case_groups),
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn parses_api_test_case_groups_short_flag() {
        let args = Args::try_parse_from(["cbl", "--cbl-api-key", "key", "api", "-T"])
            .expect("api -T should parse");
        match args.command {
            Some(super::Command::Api(api)) => assert!(api.query.test_case_groups),
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn parses_api_single_turn_evaluations_flag() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "--single-turn-evaluations",
        ])
        .expect("api --single-turn-evaluations should parse");
        match args.command {
            Some(super::Command::Api(api)) => assert!(api.query.single_turn_evaluations),
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn parses_api_multi_turn_evaluations_flag() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "--multi-turn-evaluations",
        ])
        .expect("api --multi-turn-evaluations should parse");
        match args.command {
            Some(super::Command::Api(api)) => assert!(api.query.multi_turn_evaluations),
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn parses_api_evaluations_pagination() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "--single-turn-evaluations",
            "--limit",
            "25",
            "--offset",
            "50",
        ])
        .expect("api evaluation pagination should parse");
        match args.command {
            Some(super::Command::Api(api)) => {
                assert_eq!(api.query.limit, Some(25));
                assert_eq!(api.query.offset, Some(50));
            }
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn rejects_zero_api_evaluations_limit() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "--single-turn-evaluations",
            "--limit",
            "0",
        ])
        .expect_err("zero limit should be rejected");
        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }

    #[test]
    fn rejects_api_evaluations_limit_above_one_hundred() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "--single-turn-evaluations",
            "--limit",
            "101",
        ])
        .expect_err("limit above 100 should be rejected");
        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }

    #[test]
    fn rejects_top_level_api_query_flag() {
        let err = Args::try_parse_from(["cbl", "--cbl-api-key", "key", "--monthly-quota"])
            .expect_err("top-level --monthly-quota should be rejected");
        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn rejects_json_without_query_flag() {
        let err = Args::try_parse_from(["cbl", "--cbl-api-key", "key", "api", "--json"])
            .expect_err("--json alone should be rejected");
        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn rejects_missing_subcommand_without_query_flag() {
        let args = Args::try_parse_from(["cbl", "--cbl-api-key", "key"])
            .expect("parsing should succeed before validation");
        let err = args
            .validate()
            .expect_err("validate() should reject missing subcommand");
        assert_eq!(err.kind(), ErrorKind::MissingSubcommand);
    }

    #[test]
    fn accepts_json_with_monthly_quota() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "--monthly-quota",
            "--json",
        ])
        .expect("api --monthly-quota --json should parse");
        match args.command {
            Some(super::Command::Api(api)) => {
                assert!(api.query.monthly_quota);
                assert!(api.json);
            }
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn accepts_json_with_validate_api_key() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "--validate-api-key",
            "--json",
        ])
        .expect("api --validate-api-key --json should parse");
        match args.command {
            Some(super::Command::Api(api)) => {
                assert!(api.query.validate_api_key);
                assert!(api.json);
            }
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn accepts_json_with_test_case_groups() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "--test-case-groups",
            "--json",
        ])
        .expect("api --test-case-groups --json should parse");
        match args.command {
            Some(super::Command::Api(api)) => {
                assert!(api.query.test_case_groups);
                assert!(api.json);
            }
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn accepts_json_with_single_turn_evaluations() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "--single-turn-evaluations",
            "--json",
        ])
        .expect("api --single-turn-evaluations --json should parse");
        match args.command {
            Some(super::Command::Api(api)) => {
                assert!(api.query.single_turn_evaluations);
                assert!(api.json);
            }
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn accepts_json_with_multi_turn_evaluations() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "--multi-turn-evaluations",
            "--json",
        ])
        .expect("api --multi-turn-evaluations --json should parse");
        match args.command {
            Some(super::Command::Api(api)) => {
                assert!(api.query.multi_turn_evaluations);
                assert!(api.json);
            }
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn parses_json_short_flag() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "--monthly-quota",
            "-J",
        ])
        .expect("api -J should parse");
        match args.command {
            Some(super::Command::Api(api)) => assert!(api.json),
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn rejects_two_query_flags() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "--monthly-quota",
            "--validate-api-key",
        ])
        .expect_err("two query flags together should be rejected");
        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn rejects_evaluations_query_flag_conflict() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "--monthly-quota",
            "--single-turn-evaluations",
        ])
        .expect_err("evaluation query flag should conflict with other query flags");
        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn rejects_api_query_flag_with_single_turn_subcommand() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "eval",
            "--monthly-quota",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "1",
            "--test-case-groups",
            "suicidal_ideation",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("eval --monthly-quota should be rejected");
        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn rejects_api_query_flag_with_multi_turn_subcommand() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "eval",
            "--validate-api-key",
            "multi-turn",
            "--threshold",
            "0.5",
            "--max-turns",
            "4",
            "--test-case-groups",
            "suicidal_ideation",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("eval --validate-api-key should be rejected");
        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }
}
