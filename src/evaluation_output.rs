use crate::protocol_types::common::{RerunSelector, TestCaseGroup};

#[derive(serde::Serialize)]
struct EvaluationOutput<'a, T>
where
    T: serde::Serialize,
{
    #[serde(flatten)]
    result: &'a T,
    test_case_groups: &'a [TestCaseGroup],
}

pub fn serialize_evaluation_output<T>(
    result: &T,
    test_case_groups: &[TestCaseGroup],
) -> Result<String, serde_json::Error>
where
    T: serde::Serialize,
{
    serde_json::to_string_pretty(&EvaluationOutput {
        result,
        test_case_groups,
    })
}

#[derive(serde::Serialize)]
struct RerunEvaluationOutput<'a, T>
where
    T: serde::Serialize,
{
    #[serde(flatten)]
    result: &'a T,
    #[serde(flatten)]
    source: RerunEvaluationSource<'a>,
}

#[derive(serde::Serialize)]
#[serde(untagged)]
enum RerunEvaluationSource<'a> {
    TestResultIds { test_result_ids: &'a [i64] },
    SourceEvaluationId { source_evaluation_id: i64 },
}

pub fn serialize_rerun_evaluation_output<T>(
    result: &T,
    selector: &RerunSelector,
) -> Result<String, serde_json::Error>
where
    T: serde::Serialize,
{
    let source = match selector {
        RerunSelector::TestResultIds { test_result_ids } => {
            RerunEvaluationSource::TestResultIds { test_result_ids }
        }
        RerunSelector::EvaluationId { evaluation_id } => {
            RerunEvaluationSource::SourceEvaluationId {
                source_evaluation_id: *evaluation_id,
            }
        }
    };
    serde_json::to_string_pretty(&RerunEvaluationOutput { result, source })
}

#[cfg(test)]
mod tests {
    use super::{RerunSelector, serialize_evaluation_output, serialize_rerun_evaluation_output};
    use serde::Serialize;
    use serde_json::json;

    #[derive(Serialize)]
    struct TestResult {
        total_passed: i32,
        total_failed: i32,
    }

    #[test]
    fn evaluation_output_includes_test_case_groups() {
        let result = TestResult {
            total_passed: 3,
            total_failed: 1,
        };

        let json = serialize_evaluation_output(
            &result,
            &["suicidal_ideation".to_string(), "custom_group".to_string()],
        )
        .expect("evaluation output should serialize");
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("evaluation output should be valid json");

        assert_eq!(
            value,
            json!({
                "total_passed": 3,
                "total_failed": 1,
                "test_case_groups": [
                    "suicidal_ideation",
                    "custom_group"
                ]
            })
        );
    }

    #[test]
    fn rerun_evaluation_output_includes_test_result_ids() {
        let result = TestResult {
            total_passed: 3,
            total_failed: 1,
        };

        let json = serialize_rerun_evaluation_output(
            &result,
            &RerunSelector::TestResultIds {
                test_result_ids: vec![42, 43],
            },
        )
        .expect("rerun evaluation output should serialize");
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("evaluation output should be valid json");

        assert_eq!(
            value,
            json!({
                "total_passed": 3,
                "total_failed": 1,
                "test_result_ids": [42, 43]
            })
        );
    }

    #[test]
    fn rerun_evaluation_output_includes_source_evaluation_id() {
        let result = TestResult {
            total_passed: 3,
            total_failed: 1,
        };

        let json = serialize_rerun_evaluation_output(
            &result,
            &RerunSelector::EvaluationId { evaluation_id: 123 },
        )
        .expect("rerun evaluation output should serialize");
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("evaluation output should be valid json");

        assert_eq!(
            value,
            json!({
                "total_passed": 3,
                "total_failed": 1,
                "source_evaluation_id": 123
            })
        );
    }
}
