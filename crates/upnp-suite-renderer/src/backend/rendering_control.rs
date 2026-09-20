use crate::services::error::{ConnectionError, ServiceError};
use crate::services::rendering_control::RenderingControl;
use async_trait::async_trait;
use std::sync::Arc;

use super::{MediaBackend, MediaBackendError};

/// [`RenderingControl`] implementation backed by a real [`MediaBackend`]
/// (GStreamer's `playbin` volume/mute properties, in practice) instead of
/// [`RenderingControlImpl`](crate::services::rendering_control::RenderingControlImpl)'s
/// disconnected in-memory state — without this, a control point's volume
/// slider moves and reads back correctly while doing nothing to the actual
/// sound.
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
    // percent (0-100) — same range RenderingControlImpl enforced, kept here
    // so callers see the same rejection regardless of which backend is
    // wired in.
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
}
