use super::headers::Headers;
use crate::protocol_types::{MultiTurnRequest, SingleTurnRequest};
use crate::response_provider::{CustomProviderConfig, OllamaProviderConfig, OpenAIProviderConfig};
use clap::{Parser, Subcommand, ValueEnum};
use reqwest::header::HeaderMap;
use std::path::PathBuf;

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
    pub command: Command,
}

impl Args {
    pub fn headers(&self) -> HeaderMap {
        super::headers::merge_headers(&self.headers)
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run evaluations
    Eval {
        #[command(subcommand)]
        evaluation: EvaluationCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum EvaluationCommand {
    /// Run single-turn evaluation
    SingleTurn {
        #[command(subcommand)]
        provider: ProviderCommand,
        #[command(flatten)]
        request: SingleTurnRequest,
    },

    /// Run multi-turn evaluation
    MultiTurn {
        #[command(subcommand)]
        provider: ProviderCommand,
        #[command(flatten)]
        request: MultiTurnRequest,
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
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect("single-turn args should parse");

        #[allow(clippy::match_wildcard_for_single_variants)]
        match args.command {
            super::Command::Eval { evaluation } => match evaluation {
                super::EvaluationCommand::SingleTurn { request, .. } => {
                    assert!((request.threshold - 0.5).abs() < f32::EPSILON);
                    assert_eq!(request.variations, 2);
                    assert_eq!(request.maximum_iteration_layers, 2);
                }
                _ => panic!("expected single-turn command"),
            },
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
            "3",
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
            "3",
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
            "openai",
            "--api-key",
            "openai-key",
            "--model",
            "gpt-4.1-nano",
        ])
        .expect("zero iteration layers should parse");

        #[allow(clippy::match_wildcard_for_single_variants)]
        match args.command {
            super::Command::Eval { evaluation } => match evaluation {
                super::EvaluationCommand::SingleTurn { request, .. } => {
                    assert_eq!(request.maximum_iteration_layers, 0);
                }
                _ => panic!("expected single-turn command"),
            },
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
            "--test-types",
            "user_persona",
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
            "--test-types",
            "user_persona",
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
    fn rejects_missing_multi_turn_test_types() {
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
        .expect_err("missing test types should be rejected");

        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
        assert!(err.to_string().contains("--test-types"));
    }
}
