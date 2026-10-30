use super::super::common::{default_max_concurrent_test_case_executions, parse_test_case_group};
use super::super::multi_turn::request::parse_threshold;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, clap::Args)]
pub struct MultiPartyEvalRequest {
    /// One or more comma-separated test case groups to run.
    #[arg(long, value_delimiter = ',', value_parser = parse_test_case_group, required = true)]
    pub test_case_groups: Vec<String>,
    /// Maximum simultaneous conversations (1-30).
    #[arg(long, default_value_t = default_max_concurrent_test_case_executions(), value_parser = clap::value_parser!(u8).range(1..=30))]
    pub max_concurrent_test_case_executions: u8,
    /// Responses below this safety score threshold fail.
    #[arg(short, long, value_parser = parse_threshold)]
    pub threshold: f32,
    /// Maximum utterances across all speakers, including the customer (1-100).
    #[arg(short, long, value_parser = clap::value_parser!(u8).range(1..=100))]
    pub max_turns: u8,
    /// Capture a separate server-side trace for each conversation.
    #[arg(long)]
    pub capture_trace: bool,
}

/// Payload for `multiparty_request` messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPartyRequest {
    pub test_case_groups: Vec<String>,
    pub max_concurrent_test_case_executions: u8,
    pub threshold: f32,
    pub max_turns: u8,
    pub capture_trace: bool,
}

impl From<MultiPartyEvalRequest> for MultiPartyRequest {
    fn from(request: MultiPartyEvalRequest) -> Self {
        Self {
            test_case_groups: request.test_case_groups,
            max_concurrent_test_case_executions: request.max_concurrent_test_case_executions,
            threshold: request.threshold,
            max_turns: request.max_turns,
            capture_trace: request.capture_trace,
        }
    }
}

#[cfg(all(test, feature = "openai"))]
mod tests {
    use super::MultiPartyRequest;
    use crate::cli::{Args, Command, EvaluationCommand};
    use clap::Parser;

    #[test]
    fn multiparty_accepts_odd_turns_and_rejects_legacy_selectors() {
        let argv = [
            "cbl",
            "--cbl-api-key",
            "unused",
            "eval",
            "multi-party",
            "--test-case-groups",
            "a,b",
            "--threshold",
            "0.5",
            "--max-turns",
            "7",
            "openai",
            "--api-key",
            "unused",
            "--model",
            "test",
        ];
        let parsed = Args::try_parse_from(argv).unwrap();
        let Some(Command::Eval {
            evaluation: EvaluationCommand::MultiParty { request, .. },
        }) = parsed.command
        else {
            panic!("expected multi-party command");
        };
        let request: MultiPartyRequest = request.into();
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "test_case_groups": ["a", "b"],
                "max_concurrent_test_case_executions": 30,
                "threshold": 0.5,
                "max_turns": 7,
                "capture_trace": false,
            })
        );
        let mut invalid = argv.to_vec();
        invalid.extend(["--multiparty-test-case-id", "7"]);
        assert!(Args::try_parse_from(invalid).is_err());
        for value in ["0", "31", "-1"] {
            let mut invalid = argv[..11].to_vec();
            invalid.extend(["--max-concurrent-test-case-executions", value]);
            invalid.extend(&argv[11..]);
            assert!(Args::try_parse_from(invalid).is_err());
        }
    }
}
