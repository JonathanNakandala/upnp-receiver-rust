pub mod av_transport;
pub mod connection_manager;
pub mod coordinator;
pub(crate) mod error;
pub mod media_renderer;
mod rendering_control;
mod types_impl;

pub use av_transport::AVTransport;
pub use connection_manager::ConnectionManager;
pub use error::ServiceError;
pub use media_renderer::MediaRenderer;
pub use types::ConnectionState;
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

    #[derive(Debug, Clone)]
    pub struct MediaInfo {
        pub uri: String,
        pub metadata: Option<String>,
        pub duration: Option<Duration>,
    }
}
