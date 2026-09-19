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
    next: Option<(String, Option<String>)>,
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
    async fn seek(&self, position: Duration) -> Result<(), TransportError>;
    async fn get_position(&self) -> Result<Duration, TransportError>;
    async fn get_state(&self) -> Result<TransportState, TransportError>;
    async fn get_media_info(&self) -> Result<Option<MediaInfo>, TransportError>;
    async fn get_position_info(&self) -> Result<Option<PositionInfo>, TransportError>;
    /// Queues the track to play next — UPnP's `SetNextAVTransportURI`. An
    /// empty `uri` clears whatever was previously queued (spec semantics
    /// for `SetNextAVTransportURI(NextURI="")`).
    async fn set_next_transport_uri(
        &self,
        uri: String,
        metadata: Option<String>,
    ) -> Result<(), TransportError>;
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
                next: None,
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
            next_uri: String::new(),
            next_metadata: None,
        });
        state.playback_state = TransportState::Stopped;
        // A fresh SetAVTransportURI starts a new "current track" context —
        // whatever was queued to follow the old one no longer applies.
        state.next = None;
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

    async fn seek(&self, position: Duration) -> Result<(), TransportError> {
        let mut state = self.state.write().await;
        let duration = state.media.as_ref().ok_or(TransportError::NoMedia)?.duration;

        if let Some(duration) = duration {
            if position > duration {
                return Err(TransportError::SeekError {
                    requested: position,
                    duration: Some(duration),
                });
            }
        }

        state.position = position;
        Ok(())
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
        Ok(state.media.clone().map(|media| MediaInfo {
            next_uri: state
                .next
                .as_ref()
                .map(|(uri, _)| uri.clone())
                .unwrap_or_default(),
            next_metadata: state.next.as_ref().and_then(|(_, m)| m.clone()),
            ..media
        }))
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

    async fn set_next_transport_uri(
        &self,
        uri: String,
        metadata: Option<String>,
    ) -> Result<(), TransportError> {
        let mut state = self.state.write().await;
        state.next = if uri.is_empty() {
            None
        } else {
            Some((uri, metadata))
        };
        Ok(())
    }
}
