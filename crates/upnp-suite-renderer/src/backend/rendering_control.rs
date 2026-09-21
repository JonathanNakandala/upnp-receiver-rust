use crate::services::error::{ConnectionError, ServiceError};
use crate::services::rendering_control::{validate_percent, RenderingControl};
use async_trait::async_trait;
use std::sync::Arc;

use super::{MediaBackend, MediaBackendError};

pub struct MediaBackendRenderingControl {
    backend: Arc<dyn MediaBackend>,
}

impl MediaBackendRenderingControl {
    pub fn new(backend: Arc<dyn MediaBackend>) -> Self {
        Self { backend }
    }

    fn convert_error(err: MediaBackendError) -> ServiceError {
        match err {
            MediaBackendError::Unsupported(msg) => ServiceError::InvalidValue(msg),
            MediaBackendError::Connection(msg) => {
                ServiceError::Connection(ConnectionError::ConnectionFailed(msg))
            }
            other => ServiceError::Network(other.to_string()),
        }
    }

    // UPnP's Volume state variable is a ui2, but we only ever deal in
    // percent (0-100)
    fn validate_volume(volume: u16) -> Result<(), ServiceError> {
        if volume > 100 {
            return Err(ServiceError::InvalidValue(format!(
                "Volume must be between 0 and 100, got {}",
                volume
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl RenderingControl for MediaBackendRenderingControl {
    async fn set_volume(&self, volume: u16) -> Result<(), ServiceError> {
        Self::validate_volume(volume)?;
        self.backend
            .set_volume(volume as u8)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_volume(&self) -> Result<u16, ServiceError> {
        self.backend
            .get_volume()
            .await
            .map(|v| v as u16)
            .map_err(Self::convert_error)
    }

    async fn set_mute(&self, mute: bool) -> Result<(), ServiceError> {
        self.backend
            .set_mute(mute)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_mute(&self) -> Result<bool, ServiceError> {
        self.backend.get_mute().await.map_err(Self::convert_error)
    }

    async fn set_brightness(&self, brightness: u16) -> Result<(), ServiceError> {
        validate_percent(brightness, "Brightness")?;
        self.backend
            .set_brightness(brightness)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_brightness(&self) -> Result<u16, ServiceError> {
        self.backend
            .get_brightness()
            .await
            .map_err(Self::convert_error)
    }

    async fn set_contrast(&self, contrast: u16) -> Result<(), ServiceError> {
        validate_percent(contrast, "Contrast")?;
        self.backend
            .set_contrast(contrast)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_contrast(&self) -> Result<u16, ServiceError> {
        self.backend
            .get_contrast()
            .await
            .map_err(Self::convert_error)
    }
}
