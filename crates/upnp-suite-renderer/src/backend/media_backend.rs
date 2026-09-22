use async_trait::async_trait;
use std::time::Duration;

use super::MediaBackendError;
use crate::server::soap::types::av_transport::{PlayMode, TransportState, TransportStatus};

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

    /// Changes playback rate without restarting the track — UPnP's `Play`
    /// action's `Speed` argument (1.0 is normal speed, 2.0 double speed,
    /// 0.5 half speed, etc.). Takes effect from the current position
    /// onward and persists until the next `load_media`/`stop`, which reset
    /// it back to 1.0. A backend with no rate-change mechanism of its own
    /// should reject anything other than 1.0 with
    /// `MediaBackendError::Unsupported` rather than silently ignoring it.
    async fn set_play_speed(&self, speed: f64) -> Result<(), MediaBackendError>;
    /// The rate last set via `set_play_speed`, defaulting to 1.0.
    async fn get_play_speed(&self) -> Result<f64, MediaBackendError>;

    /// UPnP's `SetPlayMode`. A backend with a gapless mechanism can honor
    /// `REPEAT_ONE`/`REPEAT_ALL` by re-queueing the current track when
    /// nothing else is queued (`GStreamerBackend` does, via
    /// `about-to-finish`); one without can just remember and report it back
    /// for API completeness.
    async fn set_play_mode(&self, mode: PlayMode) -> Result<(), MediaBackendError>;
    /// The mode last set via `set_play_mode`, defaulting to `NORMAL`.
    async fn get_play_mode(&self) -> Result<PlayMode, MediaBackendError>;

    // Volume control methods (for RenderingControl service)
    async fn set_volume(&self, volume: u8) -> Result<(), MediaBackendError>;
    async fn get_volume(&self) -> Result<u8, MediaBackendError>;
    async fn set_mute(&self, muted: bool) -> Result<(), MediaBackendError>;
    async fn get_mute(&self) -> Result<bool, MediaBackendError>;

    /// UPnP RenderingControl's `Brightness`/`Contrast` - 0-100, 50 =
    /// neutral/no adjustment (see `services::rendering_control` for range
    /// validation). Applies to the video picture only; a backend with no
    /// video output of its own has nothing to apply it to and just
    /// remembers the value, the same way it would for `Volume` with no real
    /// mixer behind it.
    async fn set_brightness(&self, brightness: u16) -> Result<(), MediaBackendError>;
    async fn get_brightness(&self) -> Result<u16, MediaBackendError>;
    async fn set_contrast(&self, contrast: u16) -> Result<(), MediaBackendError>;
    async fn get_contrast(&self) -> Result<u16, MediaBackendError>;

    /// UPnP RenderingControl's 2-point white balance pair per primary color
    /// - `Output = Gain * Input + BlackLevel` (see the spec's own
    /// `xxxVideoGain`/`xxxVideoBlackLevel` relationship note). 0-1000,
    /// 500 = neutral for both - wider than `Brightness`/`Contrast`'s 0-100,
    /// since these are calibration controls where near-black precision
    /// matters (see `video::RenderParams`'s doc).
    async fn set_red_video_gain(&self, gain: u16) -> Result<(), MediaBackendError>;
    async fn get_red_video_gain(&self) -> Result<u16, MediaBackendError>;
    async fn set_green_video_gain(&self, gain: u16) -> Result<(), MediaBackendError>;
    async fn get_green_video_gain(&self) -> Result<u16, MediaBackendError>;
    async fn set_blue_video_gain(&self, gain: u16) -> Result<(), MediaBackendError>;
    async fn get_blue_video_gain(&self) -> Result<u16, MediaBackendError>;
    async fn set_red_video_black_level(&self, level: u16) -> Result<(), MediaBackendError>;
    async fn get_red_video_black_level(&self) -> Result<u16, MediaBackendError>;
    async fn set_green_video_black_level(&self, level: u16) -> Result<(), MediaBackendError>;
    async fn get_green_video_black_level(&self) -> Result<u16, MediaBackendError>;
    async fn set_blue_video_black_level(&self, level: u16) -> Result<(), MediaBackendError>;
    async fn get_blue_video_black_level(&self) -> Result<u16, MediaBackendError>;

    /// UPnP's `ColorTemperature` - 0-100, 50 = neutral = D65 (6500K), the
    /// reference white point of every consumer video standard this
    /// renderer plays. See `video::RenderParams::color_temperature`'s doc.
    async fn set_color_temperature(&self, temperature: u16) -> Result<(), MediaBackendError>;
    async fn get_color_temperature(&self) -> Result<u16, MediaBackendError>;

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
