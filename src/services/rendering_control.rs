// src/services/rendering_control.rs
use crate::services::error::ServiceError;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error};

#[derive(Debug)]
pub struct RenderingState {
    volume: u16,
    mute: bool,
}

impl Default for RenderingState {
    fn default() -> Self {
        Self {
            volume: 50, // Default to 50% volume
            mute: false,
        }
    }
}

#[async_trait]
pub trait RenderingControl: Send + Sync {
    async fn set_volume(&self, volume: u16) -> Result<(), ServiceError>;
    async fn get_volume(&self) -> Result<u16, ServiceError>;
    async fn set_mute(&self, mute: bool) -> Result<(), ServiceError>;
    async fn get_mute(&self) -> Result<bool, ServiceError>;
}

pub struct RenderingControlImpl {
    state: Arc<RwLock<RenderingState>>,
}

impl RenderingControlImpl {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(RenderingState::default())),
        }
    }

    fn validate_volume(volume: u16) -> Result<(), ServiceError> {
        if volume > 100 {
            error!(volume = volume, "Invalid volume value");
            return Err(ServiceError::InvalidValue(format!(
                "Volume must be between 0 and 100, got {}",
                volume
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl RenderingControl for RenderingControlImpl {
    async fn set_volume(&self, volume: u16) -> Result<(), ServiceError> {
        Self::validate_volume(volume)?;
        debug!(volume = volume, "Setting volume");

        let mut state = self.state.write().await;
        state.volume = volume;
        Ok(())
    }

    async fn get_volume(&self) -> Result<u16, ServiceError> {
        let state = self.state.read().await;
        Ok(state.volume)
    }

    async fn set_mute(&self, mute: bool) -> Result<(), ServiceError> {
        debug!(mute = mute, "Setting mute state");
        let mut state = self.state.write().await;
        state.mute = mute;
        Ok(())
    }

    async fn get_mute(&self) -> Result<bool, ServiceError> {
        let state = self.state.read().await;
        Ok(state.mute)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_volume_control() {
        let control = RenderingControlImpl::new();

        // Test initial volume
        assert_eq!(control.get_volume().await.unwrap(), 50);

        // Test setting volume
        control.set_volume(75).await.unwrap();
        assert_eq!(control.get_volume().await.unwrap(), 75);

        // Test invalid volume
        assert!(control.set_volume(101).await.is_err());
    }

    #[tokio::test]
    async fn test_mute_control() {
        let control = RenderingControlImpl::new();

        // Test initial mute state
        assert_eq!(control.get_mute().await.unwrap(), false);

        // Test setting mute
        control.set_mute(true).await.unwrap();
        assert_eq!(control.get_mute().await.unwrap(), true);

        // Test toggling mute
        control.set_mute(false).await.unwrap();
        assert_eq!(control.get_mute().await.unwrap(), false);
    }
}