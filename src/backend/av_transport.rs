use crate::server::soap::types::av_transport::{PlayMode, TransportState, TransportStatus};
use crate::services::av_transport::{AVTransport, PositionInfo};
use crate::services::error::TransportError;
use crate::services::types::MediaInfo;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

use super::{MediaBackend, MediaBackendError};

pub struct MediaBackendAVTransport {
    backend: Arc<dyn MediaBackend>,
}

impl MediaBackendAVTransport {
    pub fn new(backend: Arc<dyn MediaBackend>) -> Self {
        Self { backend }
    }

    // Convert from our backend error to TransportError
    fn convert_error(err: MediaBackendError) -> TransportError {
        match err {
            MediaBackendError::NoMediaLoaded => TransportError::NoMedia,
            MediaBackendError::Connection(_) => {
                TransportError::Other("Backend connection error".to_string())
            }
            MediaBackendError::Unsupported(msg) => TransportError::UnsupportedOperation(msg),
            _ => TransportError::Other(err.to_string()),
        }
    }
}

#[async_trait]
impl AVTransport for MediaBackendAVTransport {
    async fn set_transport_uri(
        &self,
        uri: String,
        metadata: Option<String>,
    ) -> Result<(), TransportError> {
        self.backend
            .load_media(&uri, metadata)
            .await
            .map_err(Self::convert_error)
    }

    async fn play(&self) -> Result<(), TransportError> {
        self.backend.play().await.map_err(Self::convert_error)
    }

    async fn pause(&self) -> Result<(), TransportError> {
        self.backend.pause().await.map_err(Self::convert_error)
    }

    async fn stop(&self) -> Result<(), TransportError> {
        self.backend.stop().await.map_err(Self::convert_error)
    }

    async fn seek(&self, position: Duration) -> Result<(), TransportError> {
        // The backend process itself doesn't reject a seek with nothing loaded
        // (it just moves a position counter that governs nothing), so check
        // here to give the caller the same NoMedia error the fallback
        // AVTransportImpl gives, instead of a false "OK".
        let uri = self
            .backend
            .get_current_uri()
            .await
            .map_err(Self::convert_error)?;
        if uri.is_none() {
            return Err(TransportError::NoMedia);
        }

        self.backend.seek(position).await.map_err(Self::convert_error)
    }

    async fn get_position(&self) -> Result<Duration, TransportError> {
        self.backend
            .get_position()
            .await
            .map_err(Self::convert_error)
    }

    async fn get_state(&self) -> Result<TransportState, TransportError> {
        self.backend
            .get_playback_state()
            .await
            .map_err(Self::convert_error)
    }

    async fn get_status(&self) -> Result<TransportStatus, TransportError> {
        self.backend.get_status().await.map_err(Self::convert_error)
    }

    async fn set_play_speed(&self, speed: f64) -> Result<(), TransportError> {
        self.backend
            .set_play_speed(speed)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_play_speed(&self) -> Result<f64, TransportError> {
        self.backend
            .get_play_speed()
            .await
            .map_err(Self::convert_error)
    }

    async fn set_play_mode(&self, mode: PlayMode) -> Result<(), TransportError> {
        self.backend
            .set_play_mode(mode)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_play_mode(&self) -> Result<PlayMode, TransportError> {
        self.backend
            .get_play_mode()
            .await
            .map_err(Self::convert_error)
    }

    async fn get_media_info(&self) -> Result<Option<MediaInfo>, TransportError> {
        // Get both URI and duration to construct MediaInfo
        let uri_future = self.backend.get_current_uri();
        let duration_future = self.backend.get_duration();
        let metadata_future = self.backend.get_metadata();
        let next_future = self.backend.get_next_media();

        // Execute them in parallel
        let (uri_result, duration_result, metadata_result, next_result) =
            tokio::join!(uri_future, duration_future, metadata_future, next_future);

        // Handle results
        let uri = uri_result.map_err(Self::convert_error)?;
        let duration = duration_result.map_err(Self::convert_error)?;
        let metadata = metadata_result.map_err(Self::convert_error)?;
        let next = next_result.map_err(Self::convert_error)?;

        // If we have a URI, construct MediaInfo
        if let Some(uri) = uri {
            Ok(Some(MediaInfo {
                uri,
                metadata,
                duration,
                next_uri: next.as_ref().map(|t| t.uri.clone()).unwrap_or_default(),
                next_metadata: next.and_then(|t| t.metadata),
            }))
        } else {
            Ok(None)
        }
    }

    async fn set_next_transport_uri(
        &self,
        uri: String,
        metadata: Option<String>,
    ) -> Result<(), TransportError> {
        if uri.is_empty() {
            self.backend
                .clear_next_media()
                .await
                .map_err(Self::convert_error)
        } else {
            self.backend
                .set_next_media(&uri, metadata)
                .await
                .map_err(Self::convert_error)
        }
    }

    async fn next(&self) -> Result<(), TransportError> {
        let next = self
            .backend
            .get_next_media()
            .await
            .map_err(Self::convert_error)?
            .ok_or_else(|| TransportError::UnsupportedOperation("no next track queued".into()))?;

        // Not gapless — unlike the `about-to-finish` promotion the backend
        // does on its own, a control-point-initiated `Next` interrupts
        // playback mid-track, so there's no lookahead buffer to hand off
        // from. load_media + play is the same pair SetAVTransportURI+Play
        // uses.
        self.backend
            .load_media(&next.uri, next.metadata)
            .await
            .map_err(Self::convert_error)?;
        self.backend.play().await.map_err(Self::convert_error)?;
        self.backend
            .clear_next_media()
            .await
            .map_err(Self::convert_error)
    }

    async fn previous(&self) -> Result<(), TransportError> {
        Err(TransportError::UnsupportedOperation(
            "no previous track available".into(),
        ))
    }

    async fn get_position_info(&self) -> Result<Option<PositionInfo>, TransportError> {
        // Similar to get_media_info but also get position
        let uri_future = self.backend.get_current_uri();
        let duration_future = self.backend.get_duration();
        let position_future = self.backend.get_position();
        let metadata_future = self.backend.get_metadata();

        // Execute in parallel
        let (uri_result, duration_result, position_result, metadata_result) = tokio::join!(
            uri_future,
            duration_future,
            position_future,
            metadata_future
        );

        // Process results
        let uri = uri_result.map_err(Self::convert_error)?;

        // Only return position info if we have a URI
        if let Some(uri) = uri {
            let duration = match duration_result {
                Ok(d) => d,
                Err(_) => None, // If duration fails, we'll just use None
            };

            let position = match position_result {
                Ok(p) => Some(p),
                Err(_) => None, // If position fails, we'll just use None
            };

            let metadata = match metadata_result {
                Ok(m) => m,
                Err(_) => None, // If metadata fails, we'll just use None
            };

            Ok(Some(PositionInfo {
                uri,
                metadata,
                position,
                duration,
            }))
        } else {
            Ok(None) // No URI means no position info
        }
    }
}
