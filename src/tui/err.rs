use thiserror::Error;

#[derive(Error, Debug)]
pub enum TuiError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Rendering error: {0}")]
    Render(String),

    #[error("Progress channel closed unexpectedly")]
    ChannelClosed,
}
