use thiserror::Error;

/// Errors from managing the dummy backend's child process and its wire
/// protocol — distinct from [`super::super::MediaBackendError`], which is
/// the backend-agnostic error type every [`MediaBackend`] implementation
/// (dummy or GStreamer) maps its own errors into.
///
/// [`MediaBackend`]: super::super::MediaBackend
#[derive(Debug, Error)]
pub enum DummyProcessError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("No active connection to media process")]
    NoConnection,

    #[error("No media loaded")]
    NoMediaLoaded,

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

pub type Result<T> = std::result::Result<T, DummyProcessError>;
