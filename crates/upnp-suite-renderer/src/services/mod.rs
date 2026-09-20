pub mod av_transport;
pub mod connection_manager;
pub(crate) mod error;
pub mod instance;
pub mod media_renderer;
pub mod rendering_control;
mod types_impl;

pub use error::ServiceError;
pub use media_renderer::MediaRenderer;
pub mod types {
    use std::time::Duration;

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum ConnectionState {
        Connected,
        Disconnected,
        Pending,
    }

    #[derive(Debug, Clone)]
    pub struct ProtocolInfo {
        pub protocol: String,
        pub network: String,
        pub mime_type: String,
        pub additional_info: String,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ConnectionDirection {
        Input,
        Output,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ConnectionManagerStatus {
        Ok,
        ContentFormatMismatch,
        InsufficientBandwidth,
        UnreliableChannel,
        Unknown,
    }

    #[derive(Debug, Clone)]
    pub struct ConnectionInfo {
        pub connection_id: i32,
        pub rcs_id: i32,
        pub av_transport_id: i32,
        pub protocol_info: String,
        pub peer_connection_manager: String,
        pub peer_connection_id: i32,
        pub direction: ConnectionDirection,
        pub status: ConnectionManagerStatus,
    }

    /// What `ConnectionManager::prepare_connection` (UPnP's
    /// `PrepareForConnection()`) hands back: the newly allocated
    /// `ConnectionID`, plus the `AVTransportID`/`RcsID` a control point
    /// should use for subsequent AVTransport/RenderingControl calls tied to
    /// this connection (`-1` for either means no such service instance is
    /// associated with the connection).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PreparedConnection {
        pub connection_id: i32,
        pub av_transport_id: i32,
        pub rcs_id: i32,
    }

    #[derive(Debug, Clone)]
    pub struct MediaInfo {
        pub uri: String,
        pub metadata: Option<String>,
        pub duration: Option<Duration>,
        // UPnP's single `SetNextAVTransportURI` lookahead slot — empty
        // string/`None` when nothing's queued.
        pub next_uri: String,
        pub next_metadata: Option<String>,
    }
}
