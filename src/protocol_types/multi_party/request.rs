use super::super::common::{
    EvaluationId, RerunSelector, TestResultIds, default_max_concurrent_test_case_executions,
    parse_evaluation_id, parse_test_case_group, parse_test_result_ids,
};
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
}

#[derive(Debug, Clone, clap::Args)]
#[command(group(
    clap::ArgGroup::new("rerun_selector")
        .required(true)
        .multiple(false)
        .args(["test_result_ids", "evaluation_id"])
))]
pub struct MultiPartyRerunEvalRequest {
    /// Maximum simultaneous conversations (1-30).
    #[arg(long, default_value_t = default_max_concurrent_test_case_executions(), value_parser = clap::value_parser!(u8).range(1..=30))]
    pub max_concurrent_test_case_executions: u8,
    /// Responses below this safety score threshold fail.
    #[arg(short, long, value_parser = parse_threshold)]
    pub threshold: f32,
    /// Maximum utterances across all speakers, including the customer (1-100).
    #[arg(short, long, value_parser = clap::value_parser!(u8).range(1..=100))]
    pub max_turns: u8,
    /// Comma-separated historic participant result IDs to re-run.
    #[arg(
        long = "test-result-ids",
        alias = "test-result-id",
        value_parser = parse_test_result_ids,
        allow_hyphen_values = true
    )]
    pub test_result_ids: Option<TestResultIds>,
    /// Historic multiparty evaluation ID whose stored conversations should be re-run.
    #[arg(long, value_parser = parse_evaluation_id, allow_hyphen_values = true)]
    pub evaluation_id: Option<EvaluationId>,
}

/// Payload for `multiparty_request` messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPartyRequest {
    pub test_case_groups: Vec<String>,
    pub max_concurrent_test_case_executions: u8,
    pub threshold: f32,
    pub max_turns: u8,
}

impl From<MultiPartyEvalRequest> for MultiPartyRequest {
    fn from(request: MultiPartyEvalRequest) -> Self {
        Self {
            test_case_groups: request.test_case_groups,
            max_concurrent_test_case_executions: request.max_concurrent_test_case_executions,
            threshold: request.threshold,
            max_turns: request.max_turns,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPartyRerunRequest {
    #[serde(flatten)]
    pub selector: RerunSelector,
    pub max_concurrent_test_case_executions: u8,
    pub threshold: f32,
    pub max_turns: u8,
}

#[derive(Debug, Clone)]
pub enum MultiPartyEvaluationRequest {
    Standard(MultiPartyRequest),
    Rerun(MultiPartyRerunRequest),
}

impl MultiPartyEvaluationRequest {
    pub fn test_case_groups(&self) -> Option<&[String]> {
        match self {
            MultiPartyEvaluationRequest::Standard(request) => Some(&request.test_case_groups),
            MultiPartyEvaluationRequest::Rerun(_) => None,
        }
    }

    pub fn rerun_selector(&self) -> Option<RerunSelector> {
        match self {
            MultiPartyEvaluationRequest::Standard(_) => None,
            MultiPartyEvaluationRequest::Rerun(request) => Some(request.selector.clone()),
        }
    }
}

impl From<MultiPartyEvalRequest> for MultiPartyEvaluationRequest {
    fn from(request: MultiPartyEvalRequest) -> Self {
        MultiPartyEvaluationRequest::Standard(request.into())
    }
}

impl From<MultiPartyRerunEvalRequest> for MultiPartyEvaluationRequest {
    fn from(request: MultiPartyRerunEvalRequest) -> Self {
        MultiPartyEvaluationRequest::Rerun(MultiPartyRerunRequest {
            selector: RerunSelector::from_parts(request.test_result_ids, request.evaluation_id),
            max_concurrent_test_case_executions: request.max_concurrent_test_case_executions,
            threshold: request.threshold,
            max_turns: request.max_turns,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPartyRequestEnvelope {
    pub version: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub data: MultiPartyRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPartyRerunRequestEnvelope {
    pub version: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub data: MultiPartyRerunRequest,
}

impl From<MultiPartyRequest> for MultiPartyRequestEnvelope {
    fn from(data: MultiPartyRequest) -> Self {
        Self {
            version: crate::consts::version::PROTOCOL_VERSION.to_string(),
            message_type: "multiparty_request".to_string(),
            data,
        }
    }
}

impl From<MultiPartyRerunRequest> for MultiPartyRerunRequestEnvelope {
    fn from(data: MultiPartyRerunRequest) -> Self {
        Self {
            version: crate::consts::version::PROTOCOL_VERSION.to_string(),
            message_type: "multiparty_rerun_request".to_string(),
            data,
        }
    }
}

#[cfg(all(test, feature = "openai"))]
mod tests {
    use super::{MultiPartyEvaluationRequest, MultiPartyRequest};
    use crate::cli::{Args, Command, EvaluationCommand, ReRunEvaluationCommand};
    use crate::protocol_types::common::RerunSelector;
    use clap::Parser;

    #[test]
    fn multiparty_request_matches_api_and_rejects_removed_options() {
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
            })
        );
        let mut invalid = argv.to_vec();
        invalid.extend(["--multiparty-test-case-id", "7"]);
        assert!(Args::try_parse_from(invalid).is_err());
        let mut invalid = argv.to_vec();
        invalid.insert(11, "--capture-trace");
        let error =
            Args::try_parse_from(invalid).expect_err("trace capture was removed from the API");
        assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
        for value in ["0", "31", "-1"] {
            let mut invalid = argv[..11].to_vec();
            invalid.extend(["--max-concurrent-test-case-executions", value]);
            invalid.extend(&argv[11..]);
            assert!(Args::try_parse_from(invalid).is_err());
        }
    }

    fn rerun_argv<'a>(selector: &'a [&'a str]) -> Vec<&'a str> {
        let mut argv = vec![
            "cbl",
            "--cbl-api-key",
            "unused",
            "eval",
            "re-run",
            "multi-party",
            "--threshold",
            "0.5",
            "--max-turns",
            "7",
        ];
        argv.extend(selector);
        argv.extend(["openai", "--api-key", "unused", "--model", "test"]);
        argv
    }

    fn parse_rerun(selector: &[&str]) -> MultiPartyEvaluationRequest {
        let parsed = Args::try_parse_from(rerun_argv(selector)).unwrap();
        let Some(Command::Eval {
            evaluation:
                EvaluationCommand::ReRun {
                    rerun: ReRunEvaluationCommand::MultiParty { request, .. },
                },
        }) = parsed.command
        else {
            panic!("expected multi-party rerun command");
        };
        request.into()
    }

    #[test]
    fn multiparty_rerun_parses_ids_and_serializes_wire_envelope() {
        let request = parse_rerun(&["--test-result-ids", "42,43"]);
        assert_eq!(
            request.rerun_selector(),
            Some(RerunSelector::TestResultIds {
                test_result_ids: vec![42, 43]
            })
        );
        let MultiPartyEvaluationRequest::Rerun(request) = request else {
            panic!("expected rerun request");
        };
        assert_eq!(
            serde_json::to_value(super::MultiPartyRerunRequestEnvelope::from(request)).unwrap(),
            serde_json::json!({
                "version": crate::consts::version::PROTOCOL_VERSION,
                "type": "multiparty_rerun_request",
                "data": {
                    "test_result_ids": [42, 43],
                    "max_concurrent_test_case_executions": 30,
                    "threshold": 0.5,
                    "max_turns": 7
                }
            })
        );
    }

    #[test]
    fn multiparty_rerun_parses_evaluation_id() {
        assert_eq!(
            parse_rerun(&["--evaluation-id", "123"]).rerun_selector(),
            Some(RerunSelector::EvaluationId { evaluation_id: 123 })
        );
    }

    #[test]
    fn multiparty_rerun_requires_exactly_one_positive_selector() {
        assert!(Args::try_parse_from(rerun_argv(&[])).is_err());
        assert!(
            Args::try_parse_from(rerun_argv(&[
                "--test-result-ids",
                "42",
                "--evaluation-id",
                "123"
            ]))
            .is_err()
        );
        assert!(Args::try_parse_from(rerun_argv(&["--test-result-ids", "0"])).is_err());
        assert!(Args::try_parse_from(rerun_argv(&["--evaluation-id", "0"])).is_err());
    }
}
