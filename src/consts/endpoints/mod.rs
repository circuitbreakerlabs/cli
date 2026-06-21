mod baseurl;
mod evaluations;
mod monthly_quota;
mod multiturn;
mod singleturn;
mod test_case_groups;
mod validate_api_key;

pub use baseurl::CBL_BASE_URL;
pub use evaluations::{MULTI_TURN_EVALUATIONS_ENDPOINT, SINGLE_TURN_EVALUATIONS_ENDPOINT};
pub use monthly_quota::MONTHLY_QUOTA_ENDPOINT;
pub use test_case_groups::TEST_CASE_GROUPS_ENDPOINT;
pub use validate_api_key::VALIDATE_API_KEY_ENDPOINT;

pub fn endpoint_from_evaluation_type(eval: &crate::evaluations::EvaluationType) -> &str {
    match eval {
        crate::evaluations::EvaluationType::SingleTurn => singleturn::SINGLE_TURN_ENDPOINT,
        crate::evaluations::EvaluationType::SingleTurnRerun => {
            singleturn::SINGLE_TURN_RERUN_ENDPOINT
        }
        crate::evaluations::EvaluationType::MultiTurn => multiturn::MULTI_TURN_ENDPOINT,
        crate::evaluations::EvaluationType::MultiTurnRerun => multiturn::MULTI_TURN_RERUN_ENDPOINT,
    }
}

#[cfg(test)]
mod tests {
    use super::endpoint_from_evaluation_type;
    use crate::evaluations::EvaluationType;

    #[test]
    fn evaluation_type_selects_standard_and_rerun_endpoints() {
        assert_eq!(
            endpoint_from_evaluation_type(&EvaluationType::SingleTurn),
            "/ws/singleturn_evaluation"
        );
        assert_eq!(
            endpoint_from_evaluation_type(&EvaluationType::SingleTurnRerun),
            "/ws/singleturn_rerun_evaluation"
        );
        assert_eq!(
            endpoint_from_evaluation_type(&EvaluationType::MultiTurn),
            "/ws/multiturn_evaluation"
        );
        assert_eq!(
            endpoint_from_evaluation_type(&EvaluationType::MultiTurnRerun),
            "/ws/multiturn_rerun_evaluation"
        );
    }
}
