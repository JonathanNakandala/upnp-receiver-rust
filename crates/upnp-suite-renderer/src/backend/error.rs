use thiserror::Error;

/// Backend-agnostic error type every [`MediaBackend`] implementation maps
/// its own errors into, so callers (the SOAP/service layer) never need to
/// know which concrete backend is in use.
///
/// [`MediaBackend`]: super::MediaBackend
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
    #[error("GStreamer error: {0}")]
    GStreamer(String),
    #[error("Other backend error: {0}")]
    Other(String),
}
