use std::{io, time::Duration};
use thiserror::Error;

use crate::server::soap::types::av_transport::TransportState;

#[derive(Error, Debug, Clone)]
pub enum ServiceError {
    #[error("Transport error: {0}")]
    Transport(TransportError),

    #[error("Connection error: {0}")]
    Connection(ConnectionError),

    #[error("Protocol error: {0}")]
    Protocol(ProtocolError),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Template error: {0}")]
    Template(String),

    #[error("Invalid value: {0}")]
    InvalidValue(String),
    #[error("IO Error: {0}")]
    IoError(String),
    #[error("Network Error: {0}")]
    Network(String),
}
impl From<io::Error> for ServiceError {
    fn from(err: io::Error) -> Self {
        ServiceError::Io(err.to_string())
    }
}

impl From<askama::Error> for ServiceError {
    fn from(err: askama::Error) -> Self {
        ServiceError::Template(err.to_string())
    }
}

impl From<TransportError> for ServiceError {
    fn from(err: TransportError) -> Self {
        ServiceError::Transport(err)
    }
}
#[derive(Error, Debug, Clone)]
pub enum TransportError {
    #[error("invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition {
        from: TransportState,
        to: TransportState,
    },

    #[error("no media present")]
    NoMedia,

    #[error("media loading failed: {0}")]
    MediaLoadFailed(String),

    #[error("seek error: cannot seek to {requested:?} (duration: {duration:?})")]
    SeekError {
        requested: Duration,
        duration: Option<Duration>,
    },

    #[error("unsupported operation: {0}")]
    UnsupportedOperation(String),

    #[error("internal error: {0}")]
    Internal(String),
    #[error("other error: {0}")]
    Other(String),
}

#[derive(Error, Debug, Clone)]
pub enum ConnectionError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("connection lost")]
    ConnectionLost,

    #[error("protocol not supported: {0}")]
    UnsupportedProtocol(String),

    #[error("invalid connection state: {0}")]
    InvalidState(String),

    /// `PrepareForConnection`'s `RemoteProtocolInfo` doesn't match anything
    /// this ConnectionManager reports in `SinkProtocolInfo` — UPnP error 701.
    #[error("incompatible protocol info: {0}")]
    IncompatibleProtocolInfo(String),

    /// `PrepareForConnection`'s `Direction` isn't one this ConnectionManager
    /// can satisfy — a MediaRenderer only ever sinks (`Input`) — UPnP error
    /// 702.
    #[error("incompatible direction: {0}")]
    IncompatibleDirection(String),

    /// `PrepareForConnection` was called while the connection table is
    /// already at capacity — UPnP error 708.
    #[error("connection table overflow")]
    ConnectionTableOverflow,
}

#[derive(Error, Debug, Clone)]
pub enum ProtocolError {
    #[error("invalid message format: {0}")]
    InvalidFormat(String),

    #[error("missing required field: {0}")]
    MissingField(String),

    #[error("invalid header: {0}")]
    InvalidHeader(String),

    #[error("unsupported protocol version: {0}")]
    UnsupportedVersion(String),
}
