use async_trait::async_trait;
use std::time::Duration;

use super::MediaBackendError;
use crate::server::soap::types::av_transport::{TransportState, TransportStatus};

/// A track identified by URI plus its (opaque, DIDL-Lite) metadata — the
/// shape both the queued "next" track and a `track_changes` notification
/// carry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrackInfo {
    pub uri: String,
    pub metadata: Option<String>,
}

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
    /// Whether the *last* unprompted transition (end-of-stream vs. a
    /// pipeline error) left the backend in an error state — UPnP's
    /// `TransportStatus`. Distinct from `get_playback_state`: a backend can
    /// be `Stopped` *and* `ErrorOccurred` at once, until the next
    /// `load_media` starts a fresh attempt. Implementations with no error
    /// concept of their own can always return `Ok(TransportStatus::OK)`.
    async fn get_status(&self) -> Result<TransportStatus, MediaBackendError>;

    // Volume control methods (for RenderingControl service)
    async fn set_volume(&self, volume: u8) -> Result<(), MediaBackendError>;
    async fn get_volume(&self) -> Result<u8, MediaBackendError>;
    async fn set_mute(&self, muted: bool) -> Result<(), MediaBackendError>;
    async fn get_mute(&self) -> Result<bool, MediaBackendError>;

    // Media info
    async fn get_current_uri(&self) -> Result<Option<String>, MediaBackendError>;
    async fn get_metadata(&self) -> Result<Option<String>, MediaBackendError>;

    /// A channel that fires whenever the backend's transport state changes
    /// on its own — not in direct response to a `play`/`pause`/`stop`/
    /// `load_media` call — e.g. reaching end-of-stream or hitting a decode
    /// error. Callers that want subscribed control points notified of these
    /// promptly (rather than only on their next `GetTransportInfo` poll)
    /// should forward every value this reports as a `LastChange` event.
    /// Implementations with no such async transitions can return a channel
    /// that never fires.
    fn state_changes(&self) -> tokio::sync::watch::Receiver<TransportState>;

    // Gapless queueing — UPnP's `SetNextAVTransportURI`/`NextAVTransportURI`.
    // AVTransport:1 only ever supports a single lookahead slot, not a real
    // playlist: a control point wanting more calls `set_next_media` again
    // after each transition.
    /// Queues `uri`/`metadata` to play immediately after the current track
    /// finishes. A backend with a gapless mechanism (`GStreamerBackend` uses
    /// `playbin`'s `about-to-finish` signal) switches to it without a gap;
    /// one without can just remember it and swap in `load_media`+`play` on
    /// its own end-of-stream detection — still correct, just not gapless.
    async fn set_next_media(
        &self,
        uri: &str,
        metadata: Option<String>,
    ) -> Result<(), MediaBackendError>;
    /// Clears whatever's queued via `set_next_media` — UPnP's
    /// `SetNextAVTransportURI(NextURI="")` semantics.
    async fn clear_next_media(&self) -> Result<(), MediaBackendError>;
    /// What's currently queued via `set_next_media`, if anything.
    async fn get_next_media(&self) -> Result<Option<TrackInfo>, MediaBackendError>;

    /// Fires when the *current* track changes without a direct
    /// `load_media` call from us — i.e. a gapless promotion of the queued
    /// next track. `state_changes` alone wouldn't cover this: the transport
    /// state doesn't change (stays `Playing`) across a gapless transition,
    /// only the URI/metadata does. Implementations with no gapless
    /// mechanism can return a channel that never fires.
    fn track_changes(&self) -> tokio::sync::watch::Receiver<TrackInfo>;
}
