mod baseurl;
mod multiturn;
mod singleturn;

pub use baseurl::CBL_BASE_URL;
pub use multiturn::MULTI_TURN_ENDPOINT;
pub use singleturn::SINGLE_TURN_ENDPOINT;

pub fn endpoint_from_evaluation_type(eval: &crate::evaluations::EvaluationType) -> &str {
    match eval {
        crate::evaluations::EvaluationType::SingleTurn => singleturn::SINGLE_TURN_ENDPOINT,
        crate::evaluations::EvaluationType::MultiTurn => multiturn::MULTI_TURN_ENDPOINT,
    }
}
