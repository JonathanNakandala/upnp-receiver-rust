use super::{media_process_manager::MediaProcessManager, MediaBackend, MediaBackendError};
use crate::backend::error::MediaProcessError;
use crate::server::soap::types::av_transport::TransportState;
use async_trait::async_trait;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
pub struct MediaProcessBackend {
    manager: Arc<Mutex<MediaProcessManager>>,
}

impl MediaProcessBackend {
    pub fn new(manager: Arc<Mutex<MediaProcessManager>>) -> Self {
        Self { manager }
    }

    // Helper to convert error types
    fn convert_error(err: MediaProcessError) -> MediaBackendError {
        match err {
            MediaProcessError::NoConnection => {
                MediaBackendError::Connection("No connection to media process".to_string())
            }
            MediaProcessError::MaxRestartAttempts(attempts) => MediaBackendError::ProcessError(
                format!("Max restart attempts ({}) reached", attempts),
            ),
            MediaProcessError::Connection(msg) => MediaBackendError::Connection(msg),
            MediaProcessError::CommandFailed(msg) => MediaBackendError::Other(msg),
            MediaProcessError::UnexpectedResponse(msg) => {
                MediaBackendError::Other(format!("Unexpected response: {}", msg))
            }
            _ => MediaBackendError::Other(format!("Media process error: {:?}", err)),
        }
    }

    // Helper to convert playback states
    fn convert_playback_state(state: TransportState) -> TransportState {
        state
    }
}

#[async_trait]
impl MediaBackend for MediaProcessBackend {
    async fn load_media(
        &self,
        uri: &str,
        metadata: Option<String>,
    ) -> Result<(), MediaBackendError> {
        self.manager
            .lock()
            .await
            .load_uri(uri, metadata)
            .await
            .map_err(|err| Self::convert_error(err))
    }

    async fn play(&self) -> Result<(), MediaBackendError> {
        self.manager
            .lock()
            .await
            .play()
            .await
            .map_err(|err| Self::convert_error(err))
    }

    async fn pause(&self) -> Result<(), MediaBackendError> {
        self.manager
            .lock()
            .await
            .pause()
            .await
            .map_err(|err| Self::convert_error(err))
    }

    async fn stop(&self) -> Result<(), MediaBackendError> {
        self.manager
            .lock()
            .await
            .stop()
            .await
            .map_err(|err| Self::convert_error(err))
    }

    async fn get_position(&self) -> Result<Duration, MediaBackendError> {
        self.manager
            .lock()
            .await
            .get_position()
            .await
            .map_err(|err| Self::convert_error(err))
    }

    async fn seek(&self, position: Duration) -> Result<(), MediaBackendError> {
        self.manager
            .lock()
            .await
            .seek(position.as_millis() as u64)
            .await
            .map_err(|err| Self::convert_error(err))
    }

    async fn get_duration(&self) -> Result<Option<Duration>, MediaBackendError> {
        self.manager
            .lock()
            .await
            .get_duration()
            .await
            .map_err(|err| Self::convert_error(err))
    }

    async fn get_playback_state(&self) -> Result<TransportState, MediaBackendError> {
        self.manager
            .lock()
            .await
            .get_state()
            .await
            .map(Self::convert_playback_state)
            .map_err(|err| Self::convert_error(err))
    }

    async fn set_volume(&self, volume: u8) -> Result<(), MediaBackendError> {
        Err(MediaBackendError::Unsupported(format!(
            "Volume control not yet implemented. Requested volume level: {}",
            volume
        )))
    }

    async fn get_volume(&self) -> Result<u8, MediaBackendError> {
        Err(MediaBackendError::Unsupported(
            "Volume control not yet implemented".to_string(),
        ))
    }

    async fn set_mute(&self, muted: bool) -> Result<(), MediaBackendError> {
        Err(MediaBackendError::Unsupported(format!(
            "Mute control not yet implemented. Requested mute state: {}",
            muted
        )))
    }

    async fn get_mute(&self) -> Result<bool, MediaBackendError> {
        Err(MediaBackendError::Unsupported(
            "Mute control not yet implemented".to_string(),
        ))
    }

    async fn get_current_uri(&self) -> Result<Option<String>, MediaBackendError> {
        Ok(self.manager.lock().await.get_uri().await)
    }

    async fn get_metadata(&self) -> Result<Option<String>, MediaBackendError> {
        Ok(self.manager.lock().await.get_current_metadata().await)
    }
}
