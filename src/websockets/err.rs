use thiserror::Error;

#[derive(Error, Debug)]
pub enum WebSocketError {
    #[error("Parsing error: {0}")]
    Parsing(String),

    #[error("Connection error: {0}")]
    Connect(String),
}
