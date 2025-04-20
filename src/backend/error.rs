use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaProcessError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("No active connection to media process")]
    NoConnection,

    #[error("Process error: {0}")]
    Process(String),

    #[error("Exceeded maximum restart attempts ({0})")]
    MaxRestartAttempts(u32),

    #[error("Unexpected response: {0}")]
    UnexpectedResponse(String),

    #[error("Command failed: {0}")]
    CommandFailed(String),

    #[error("Tokio task join error: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),
}

#[derive(Error, Debug)]
pub enum MediaBackendError {
    #[error("Backend connection error: {0}")]
    Connection(String),
    #[error("No media loaded")]
    NoMediaLoaded,
    #[error("Operation not supported: {0}")]
    Unsupported(String),
    #[error("Backend process error: {0}")]
    ProcessError(String),
    #[error("Other backend error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, MediaProcessError>;
