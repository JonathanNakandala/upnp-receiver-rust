use async_trait::async_trait;
use std::time::Duration;

use super::MediaBackendError;
use crate::server::soap::types::av_transport::TransportState;
#[async_trait]
pub trait MediaBackend: Send + Sync {
    // Media playback methods
    async fn load_media(
        &self,
        uri: &str,
        metadata: Option<String>,
    ) -> Result<(), MediaBackendError>;
    async fn play(&self) -> Result<(), MediaBackendError>;
    async fn pause(&self) -> Result<(), MediaBackendError>;
    async fn stop(&self) -> Result<(), MediaBackendError>;
    async fn get_position(&self) -> Result<Duration, MediaBackendError>;
    async fn seek(&self, position: Duration) -> Result<(), MediaBackendError>;
    async fn get_duration(&self) -> Result<Option<Duration>, MediaBackendError>;
    async fn get_playback_state(&self) -> Result<TransportState, MediaBackendError>;

    // Volume control methods (for RenderingControl service)
    async fn set_volume(&self, volume: u8) -> Result<(), MediaBackendError>;
    async fn get_volume(&self) -> Result<u8, MediaBackendError>;
    async fn set_mute(&self, muted: bool) -> Result<(), MediaBackendError>;
    async fn get_mute(&self) -> Result<bool, MediaBackendError>;

    // Media info
    async fn get_current_uri(&self) -> Result<Option<String>, MediaBackendError>;
    async fn get_metadata(&self) -> Result<Option<String>, MediaBackendError>;
}
