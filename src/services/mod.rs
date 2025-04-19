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

    #[derive(Debug, Clone)]
    pub struct MediaInfo {
        pub uri: String,
        pub metadata: Option<String>,
        pub duration: Option<Duration>,
    }
}
