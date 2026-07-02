use super::headers::Headers;
use crate::protocol_types::{
    MultiTurnEvalRequest, MultiTurnRerunEvalRequest, SingleTurnEvalRequest,
    SingleTurnRerunEvalRequest,
};
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

        if let Some(Command::Api(api)) = &self.command {
            api.validate()?;
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
pub struct ApiCommand {
    #[command(flatten)]
    pub query: ApiQueryCommand,

    #[command(subcommand)]
    pub command: Option<ApiSubcommand>,

    /// Output result in JSON format
    #[arg(long, short = 'J', global = true)]
    pub json: bool,
}

impl ApiCommand {
    fn validate(&self) -> Result<(), clap::Error> {
        let query_count = [
            self.query.monthly_quota,
            self.query.validate_api_key,
            self.query.test_case_groups,
        ]
        .into_iter()
        .filter(|selected| *selected)
        .count();
        let action_count = query_count + usize::from(self.command.is_some());

        match action_count {
            0 => Err(Args::command().error(
                clap::error::ErrorKind::MissingRequiredArgument,
                "an API query flag or subcommand is required",
            )),
            1 => Ok(()),
            _ => Err(Args::command().error(
                clap::error::ErrorKind::ArgumentConflict,
                "only one API query flag or subcommand can be used",
            )),
        }
    }
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
}

#[derive(Subcommand, Debug)]
pub enum ApiSubcommand {
    /// List historic evaluation results
    Evaluations(ApiEvaluationsCommand),
}

#[derive(clap::Args, Debug)]
#[command(group(
    ArgGroup::new("evaluation_type")
        .args(["single_turn", "multi_turn"])
        .required(true)
))]
pub struct ApiEvaluationsCommand {
    /// List historic single-turn evaluation results
    #[arg(long = "single-turn")]
    pub single_turn: bool,

    /// List historic multi-turn evaluation results
    #[arg(long = "multi-turn")]
    pub multi_turn: bool,

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

    /// Re-run a historic evaluation result
    ReRun {
        #[command(subcommand)]
        rerun: ReRunEvaluationCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum ReRunEvaluationCommand {
    /// Re-run a historic single-turn evaluation result
    SingleTurn {
        #[command(subcommand)]
        provider: ProviderCommand,
        #[command(flatten)]
        request: SingleTurnRerunEvalRequest,
    },

    /// Re-run a historic multi-turn evaluation result
    MultiTurn {
        #[command(subcommand)]
        provider: ProviderCommand,
        #[command(flatten)]
        request: MultiTurnRerunEvalRequest,
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
                    assert_eq!(request.test_case_groups, vec!["suicidal_ideation"]);
                }
                super::EvaluationCommand::MultiTurn { .. } => {
                    panic!("expected single-turn command")
                }
                super::EvaluationCommand::ReRun { .. } => panic!("expected single-turn command"),
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
                super::EvaluationCommand::ReRun { .. } => panic!("expected single-turn command"),
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
                    assert_eq!(request.test_case_groups, vec!["suicidal_ideation"]);
                }
                super::EvaluationCommand::SingleTurn { .. } => {
                    panic!("expected multi-turn command")
                }
                super::EvaluationCommand::ReRun { .. } => panic!("expected multi-turn command"),
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
                .contains("expected an even integer between 2 and 100")
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
            "102",
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
                .contains("expected an even integer between 2 and 100")
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
                .contains("expected an even integer between 2 and 100")
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
    }

    #[test]
    fn parses_valid_single_turn_rerun_command() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "re-run",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "1",
            "--test-result-ids",
            "42,43",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect("single-turn rerun args should parse");

        match args.command {
            Some(super::Command::Eval { evaluation }) => match evaluation {
                super::EvaluationCommand::ReRun { rerun } => match rerun {
                    super::ReRunEvaluationCommand::SingleTurn { request, .. } => {
                        assert_eq!(
                            request
                                .test_result_ids
                                .as_ref()
                                .expect("test result IDs should be set")
                                .as_slice(),
                            &[42, 43],
                        );
                        assert_eq!(request.evaluation_id, None);
                        assert!((request.threshold - 0.5).abs() < f32::EPSILON);
                        assert_eq!(request.variations, 2);
                        assert_eq!(request.maximum_iteration_layers, 1);
                    }
                    super::ReRunEvaluationCommand::MultiTurn { .. } => {
                        panic!("expected single-turn rerun command")
                    }
                },
                super::EvaluationCommand::SingleTurn { .. }
                | super::EvaluationCommand::MultiTurn { .. } => panic!("expected rerun command"),
            },
            _ => panic!("expected eval command"),
        }
    }

    #[test]
    fn rejects_single_turn_test_result_ids_outside_rerun_subcommand() {
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
            "--test-result-ids",
            "42",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("standard single-turn should not accept test_result_ids");

        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn rejects_invalid_single_turn_test_result_ids() {
        for value in ["0", "-1", "abc", "1,,2", "1,1"] {
            let err = Args::try_parse_from([
                "cbl",
                "--cbl-api-key",
                "cbl-key",
                "eval",
                "re-run",
                "single-turn",
                "--threshold",
                "0.5",
                "--variations",
                "2",
                "--maximum-iteration-layers",
                "1",
                "--test-result-ids",
                value,
                "openai",
                "--api-key",
                "openai-key",
                "--model",
                "gpt-4.1-nano",
            ])
            .expect_err("test_result_ids should be rejected");

            assert_eq!(err.kind(), ErrorKind::ValueValidation);
            assert!(err.to_string().contains("test_result_ids"));
            assert!(
                err.to_string()
                    .contains("expected comma-separated integers >= 1")
            );
        }
    }

    #[test]
    fn parses_valid_single_turn_rerun_command_with_evaluation_id() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "re-run",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "1",
            "--evaluation-id",
            "123",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect("single-turn rerun args should parse");

        match args.command {
            Some(super::Command::Eval { evaluation }) => match evaluation {
                super::EvaluationCommand::ReRun { rerun } => match rerun {
                    super::ReRunEvaluationCommand::SingleTurn { request, .. } => {
                        assert!(request.test_result_ids.is_none());
                        assert_eq!(request.evaluation_id, Some(123));
                    }
                    super::ReRunEvaluationCommand::MultiTurn { .. } => {
                        panic!("expected single-turn rerun command")
                    }
                },
                super::EvaluationCommand::SingleTurn { .. }
                | super::EvaluationCommand::MultiTurn { .. } => panic!("expected rerun command"),
            },
            _ => panic!("expected eval command"),
        }
    }

    #[test]
    fn rejects_single_turn_rerun_without_selector() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "re-run",
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
        .expect_err("missing rerun selector should be rejected");

        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn rejects_single_turn_rerun_with_multiple_selectors() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "re-run",
            "single-turn",
            "--threshold",
            "0.5",
            "--variations",
            "2",
            "--maximum-iteration-layers",
            "1",
            "--test-result-ids",
            "42",
            "--evaluation-id",
            "123",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("multiple rerun selectors should be rejected");

        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn rejects_multi_turn_rerun_without_selector() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "re-run",
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
        .expect_err("missing rerun selector should be rejected");

        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn parses_valid_multi_turn_rerun_command() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "re-run",
            "multi-turn",
            "--threshold",
            "0.5",
            "--max-turns",
            "4",
            "--test-result-ids",
            "42,43",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect("multi-turn rerun args should parse");

        match args.command {
            Some(super::Command::Eval { evaluation }) => match evaluation {
                super::EvaluationCommand::ReRun { rerun } => match rerun {
                    super::ReRunEvaluationCommand::MultiTurn { request, .. } => {
                        assert_eq!(
                            request
                                .test_result_ids
                                .as_ref()
                                .expect("test result IDs should be set")
                                .as_slice(),
                            &[42, 43],
                        );
                        assert_eq!(request.evaluation_id, None);
                        assert!((request.threshold - 0.5).abs() < f32::EPSILON);
                        assert_eq!(request.max_turns, 4);
                    }
                    super::ReRunEvaluationCommand::SingleTurn { .. } => {
                        panic!("expected multi-turn rerun command")
                    }
                },
                super::EvaluationCommand::SingleTurn { .. }
                | super::EvaluationCommand::MultiTurn { .. } => panic!("expected rerun command"),
            },
            _ => panic!("expected eval command"),
        }
    }

    #[test]
    fn parses_valid_multi_turn_rerun_command_with_evaluation_id() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "re-run",
            "multi-turn",
            "--threshold",
            "0.5",
            "--max-turns",
            "4",
            "--evaluation-id",
            "123",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect("multi-turn rerun args should parse");

        match args.command {
            Some(super::Command::Eval { evaluation }) => match evaluation {
                super::EvaluationCommand::ReRun { rerun } => match rerun {
                    super::ReRunEvaluationCommand::MultiTurn { request, .. } => {
                        assert!(request.test_result_ids.is_none());
                        assert_eq!(request.evaluation_id, Some(123));
                    }
                    super::ReRunEvaluationCommand::SingleTurn { .. } => {
                        panic!("expected multi-turn rerun command")
                    }
                },
                super::EvaluationCommand::SingleTurn { .. }
                | super::EvaluationCommand::MultiTurn { .. } => panic!("expected rerun command"),
            },
            _ => panic!("expected eval command"),
        }
    }

    #[test]
    fn rejects_multi_turn_rerun_with_multiple_selectors() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "cbl-key",
            "eval",
            "re-run",
            "multi-turn",
            "--threshold",
            "0.5",
            "--max-turns",
            "4",
            "--test-result-ids",
            "42",
            "--evaluation-id",
            "123",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("multiple rerun selectors should be rejected");

        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn rejects_multi_turn_test_result_ids_outside_rerun_subcommand() {
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
            "--test-result-ids",
            "42",
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect_err("standard multi-turn should not accept test_result_ids");

        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn rejects_invalid_multi_turn_test_result_ids() {
        for value in ["0", "-1", "abc", "1,,2", "1,1"] {
            let err = Args::try_parse_from([
                "cbl",
                "--cbl-api-key",
                "cbl-key",
                "eval",
                "re-run",
                "multi-turn",
                "--threshold",
                "0.5",
                "--max-turns",
                "4",
                "--test-result-ids",
                value,
                "openai",
                "--api-key",
                "openai-key",
                "--model",
                "gpt-4.1-nano",
            ])
            .expect_err("test_result_ids should be rejected");

            assert_eq!(err.kind(), ErrorKind::ValueValidation);
            assert!(err.to_string().contains("test_result_ids"));
            assert!(
                err.to_string()
                    .contains("expected comma-separated integers >= 1")
            );
        }
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
    fn parses_api_single_turn_evaluations_subcommand() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "evaluations",
            "--single-turn",
        ])
        .expect("api evaluations --single-turn should parse");
        match args.command {
            Some(super::Command::Api(api)) => match api.command {
                Some(super::ApiSubcommand::Evaluations(evaluations)) => {
                    assert!(evaluations.single_turn);
                    assert!(!evaluations.multi_turn);
                }
                _ => panic!("expected api evaluations command"),
            },
            _ => panic!("expected api command"),
        }
    }

    #[test]
    fn parses_api_multi_turn_evaluations_subcommand() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "evaluations",
            "--multi-turn",
        ])
        .expect("api evaluations --multi-turn should parse");
        match args.command {
            Some(super::Command::Api(api)) => match api.command {
                Some(super::ApiSubcommand::Evaluations(evaluations)) => {
                    assert!(!evaluations.single_turn);
                    assert!(evaluations.multi_turn);
                }
                _ => panic!("expected api evaluations command"),
            },
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
            "evaluations",
            "--multi-turn",
            "--limit",
            "25",
            "--offset",
            "50",
        ])
        .expect("api evaluation pagination should parse");
        match args.command {
            Some(super::Command::Api(api)) => match api.command {
                Some(super::ApiSubcommand::Evaluations(evaluations)) => {
                    assert_eq!(evaluations.limit, Some(25));
                    assert_eq!(evaluations.offset, Some(50));
                }
                _ => panic!("expected api evaluations command"),
            },
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
            "evaluations",
            "--single-turn",
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
            "evaluations",
            "--single-turn",
            "--limit",
            "101",
        ])
        .expect_err("limit above 100 should be rejected");
        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }

    #[test]
    fn rejects_api_evaluations_without_type() {
        let err = Args::try_parse_from(["cbl", "--cbl-api-key", "key", "api", "evaluations"])
            .expect_err("evaluation type should be required");
        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn rejects_api_evaluations_with_two_types() {
        let err = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "evaluations",
            "--single-turn",
            "--multi-turn",
        ])
        .expect_err("evaluation types should conflict");
        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn rejects_top_level_api_query_flag() {
        let err = Args::try_parse_from(["cbl", "--cbl-api-key", "key", "--monthly-quota"])
            .expect_err("top-level --monthly-quota should be rejected");
        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn rejects_json_without_query_flag() {
        let args = Args::try_parse_from(["cbl", "--cbl-api-key", "key", "api", "--json"])
            .expect("--json alone parses before validation");
        let err = args
            .validate()
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
            "evaluations",
            "--single-turn",
            "--json",
        ])
        .expect("api evaluations --single-turn --json should parse");
        match args.command {
            Some(super::Command::Api(api)) => {
                assert!(matches!(
                    api.command,
                    Some(super::ApiSubcommand::Evaluations(_))
                ));
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
            "evaluations",
            "--multi-turn",
            "--json",
        ])
        .expect("api evaluations --multi-turn --json should parse");
        match args.command {
            Some(super::Command::Api(api)) => {
                assert!(matches!(
                    api.command,
                    Some(super::ApiSubcommand::Evaluations(_))
                ));
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
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "--monthly-quota",
            "--validate-api-key",
        ])
        .expect("API query flag conflict parses before validation");
        let err = args
            .validate()
            .expect_err("two query flags together should be rejected");
        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn rejects_evaluations_query_flag_conflict() {
        let args = Args::try_parse_from([
            "cbl",
            "--cbl-api-key",
            "key",
            "api",
            "--monthly-quota",
            "evaluations",
            "--single-turn",
        ])
        .expect("API action conflict parses before validation");
        let err = args
            .validate()
            .expect_err("evaluation subcommand should conflict with other query flags");
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
