use crate::server::soap::types::av_transport::TransportState;
use crate::services::error::TransportError;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use super::types::MediaInfo;

pub struct AVTransportState {
    playback_state: TransportState,
    media: Option<MediaInfo>,
    position: Duration,
}
pub struct PositionInfo {
    pub uri: String,
    pub metadata: Option<String>,
    pub position: Option<Duration>,
    pub duration: Option<Duration>,
}
impl AVTransportState {
    fn can_transition_to(&self, target: TransportState) -> bool {
        use TransportState::*;
        match (self.playback_state, target) {
            (NoMediaPresent, Stopped) => self.media.is_some(),
            (Stopped, Playing) => self.media.is_some(),
            (Playing, PausedPlayback) => true,
            (Playing, Stopped) => true,
            (PausedPlayback, Playing) => true,
            (PausedPlayback, Stopped) => true,
            _ => false,
        }
    }
}

#[async_trait]
pub trait AVTransport: Send + Sync {
    async fn set_transport_uri(
        &self,
        uri: String,
        metadata: Option<String>,
    ) -> Result<(), TransportError>;
    async fn play(&self) -> Result<(), TransportError>;
    async fn pause(&self) -> Result<(), TransportError>;
    async fn stop(&self) -> Result<(), TransportError>;
    async fn get_position(&self) -> Result<Duration, TransportError>;
    async fn get_state(&self) -> Result<TransportState, TransportError>;
    async fn get_media_info(&self) -> Result<Option<MediaInfo>, TransportError>;
    async fn get_position_info(&self) -> Result<Option<PositionInfo>, TransportError>;
}

pub struct AVTransportImpl {
    state: Arc<RwLock<AVTransportState>>,
}

impl AVTransportImpl {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(AVTransportState {
                playback_state: TransportState::NoMediaPresent,
                media: None,
                position: Duration::from_secs(0),
            })),
        }
    }

    async fn transition_state(&self, target: TransportState) -> Result<(), TransportError> {
        let mut state = self.state.write().await;

        if !state.can_transition_to(target) {
            return Err(TransportError::InvalidStateTransition {
                from: state.playback_state,
                to: target,
            });
        }

        state.playback_state = target;
        Ok(())
    }
}

#[async_trait]
impl AVTransport for AVTransportImpl {
    async fn set_transport_uri(
        &self,
        uri: String,
        metadata: Option<String>,
    ) -> Result<(), TransportError> {
        let mut state = self.state.write().await;
        state.media = Some(MediaInfo {
            uri,
            metadata,
            duration: None, // Would be set when media is actually loaded
        });
        state.playback_state = TransportState::Stopped;
        Ok(())
    }

    async fn play(&self) -> Result<(), TransportError> {
        // Play is idempotent per the AVTransport spec: control points routinely
        // reissue Play (e.g. after a seek), and that isn't an invalid transition
        // the way Pause-while-Stopped is.
        {
            let state = self.state.read().await;
            if state.playback_state == TransportState::Playing {
                return Ok(());
            }
        }
        self.transition_state(TransportState::Playing).await
    }

    async fn pause(&self) -> Result<(), TransportError> {
        self.transition_state(TransportState::PausedPlayback).await
    }

    async fn stop(&self) -> Result<(), TransportError> {
        self.transition_state(TransportState::Stopped).await
    }

    async fn get_position(&self) -> Result<Duration, TransportError> {
        let state = self.state.read().await;
        Ok(state.position)
    }

    async fn get_state(&self) -> Result<TransportState, TransportError> {
        let state = self.state.read().await;
        Ok(state.playback_state)
    }

    async fn get_media_info(&self) -> Result<Option<MediaInfo>, TransportError> {
        let state = self.state.read().await;
        Ok(state.media.clone())
    }

    async fn get_position_info(&self) -> Result<Option<PositionInfo>, TransportError> {
        let state = self.state.read().await;

        if let Some(media) = &state.media {
            Ok(Some(PositionInfo {
                uri: media.uri.clone(),
                metadata: media.metadata.clone(),
                position: Some(state.position),
                duration: media.duration,
            }))
        } else {
            Ok(None)
        }
    }
}
