mod baseurl;
mod monthly_quota;
mod multiturn;
mod singleturn;
mod test_case_groups;
mod validate_api_key;

pub use baseurl::CBL_BASE_URL;
pub use monthly_quota::MONTHLY_QUOTA_ENDPOINT;
pub use test_case_groups::TEST_CASE_GROUPS_ENDPOINT;
pub use validate_api_key::VALIDATE_API_KEY_ENDPOINT;

pub fn endpoint_from_evaluation_type(eval: &crate::evaluations::EvaluationType) -> &str {
    match eval {
        crate::evaluations::EvaluationType::SingleTurn => singleturn::SINGLE_TURN_ENDPOINT,
        crate::evaluations::EvaluationType::MultiTurn => multiturn::MULTI_TURN_ENDPOINT,
    }
}
