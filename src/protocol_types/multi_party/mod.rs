mod request;
mod response;

#[cfg(text_targets)]
pub use request::MultiPartyEvalRequest;
pub use request::MultiPartyRequest;
#[cfg(test)]
pub use response::MultiPartyErrorCode;
pub use response::MultiPartyResponse;
