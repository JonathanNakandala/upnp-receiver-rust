use super::error::ServiceError;
use super::types::{ConnectionState, ProtocolInfo};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Manages UPnP connection and protocol information
#[async_trait]
pub trait ConnectionManager: Send + Sync {
    /// Returns the current connection status
    async fn get_status(&self) -> Result<ConnectionState, ServiceError>;

    /// Returns supported protocol information in UPnP format
    async fn get_protocol_info(&self) -> Result<Vec<ProtocolInfo>, ServiceError>;
}

pub struct DummyConnectionManager {
    state: Arc<RwLock<ConnectionState>>,
}

impl DummyConnectionManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(ConnectionState::Connected)),
        }
    }
}

#[async_trait]
impl ConnectionManager for DummyConnectionManager {
    async fn get_status(&self) -> Result<ConnectionState, ServiceError> {
        Ok(*self.state.read().await)
    }

    async fn get_protocol_info(&self) -> Result<Vec<ProtocolInfo>, ServiceError> {
        Ok(vec![ProtocolInfo {
            protocol: "server-get".to_string(),
            network: "*".to_string(),
            mime_type: "audio/mpeg".to_string(),
            additional_info: "*".to_string(),
        }])
    }
}
