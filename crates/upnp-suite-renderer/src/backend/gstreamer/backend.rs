//! [`MediaBackend`] implementation using GStreamer for playback
//! Requires it to be installed locally

use async_trait::async_trait;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::watch;
use tracing::{error, info, warn};

use super::video::{self, VideoOutput};
use crate::backend::{MediaBackend, MediaBackendError, TrackInfo};
use crate::server::soap::types::av_transport::{PlayMode, TransportState, TransportStatus};

/// What we track ourselves rather than asking GStreamer, either because it
/// doesn't know it at all (UPnP DIDL-Lite metadata) or because asking it
/// would mean reading a value that lags what we just told it to do (the
/// transport state — see [`GStreamerBackend::playbin`]).
struct PlayerState {
    uri: Option<String>,
    metadata: Option<String>,
    transport_state: TransportState,
    // Whether the *last* unprompted transition (bus watcher) was a clean
    // end-of-stream or a pipeline error — UPnP's `TransportStatus`. Reset to
    // `OK` by `load_media`, since a fresh `SetAVTransportURI` is a new
    // attempt regardless of how the previous one ended.
    status: TransportStatus,
    // Current playback rate, set via `set_play_speed` — UPnP's `Play`
    // action's `Speed` argument. Reset to 1.0 by `load_media`/`stop`.
    speed: f64,
    // The single UPnP `SetNextAVTransportURI` lookahead slot. Consumed by
    // the `about-to-finish` handler for a gapless transition; cleared by an
    // explicit `load_media` (a fresh `SetAVTransportURI` invalidates
    // whatever the previous track had queued to follow it).
    next: Option<TrackInfo>,
    // UPnP's `CurrentPlayMode`, set via `SetPlayMode`. Unlike `next`, this
    // is a standing preference rather than a one-shot queue entry — it
    // survives `load_media`/`stop` and is only ever changed by an explicit
    // `SetPlayMode`. `about-to-finish` consults it when nothing's queued in
    // `next` (see `connect_about_to_finish`).
    play_mode: PlayMode,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            uri: None,
            metadata: None,
            transport_state: TransportState::NoMediaPresent,
            status: TransportStatus::OK,
            speed: 1.0,
            next: None,
            play_mode: PlayMode::Normal,
        }
    }
}

/// Which GStreamer playback element to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybinVersion {
    /// `playbin` - used for audio.
    #[default]
    V1,
    /// `playbin3` - always used for video: `playbin`'s negotiation commits
    /// to a hardware decoder's GL-memory output before checking whether the
    /// subtitle-overlay stage downstream can read it, and fails when it
    /// can't. `playbin3` checks the whole downstream chain's caps first, so
    /// it asks the decoder for plain memory instead when that's needed.
    V3,
}

impl PlaybinVersion {
    fn element_name(self) -> &'static str {
        match self {
            PlaybinVersion::V1 => "playbin",
            PlaybinVersion::V3 => "playbin3",
        }
    }
}

/// The GStreamer element plus what has to be rebuilt alongside it whenever
/// the element itself is swapped for a different [`PlaybinVersion`].
struct Pipeline {
    playbin: gst::Element,
    version: PlaybinVersion,
    bus_watcher: tokio::task::JoinHandle<()>,
}

impl Pipeline {
    fn build(
        version: PlaybinVersion,
        video_output: Option<VideoOutput>,
        state: Arc<Mutex<PlayerState>>,
        state_tx: watch::Sender<TransportState>,
        track_tx: watch::Sender<TrackInfo>,
    ) -> Result<Self, MediaBackendError> {
        let element_name = version.element_name();
        let playbin = gst::ElementFactory::make(element_name)
            .name("upnp-receiver-playbin")
            .build()
            .map_err(|e| {
                MediaBackendError::GStreamer(format!("Failed to create {element_name}: {e}"))
            })?;
        info!(playbin = element_name, "Building GStreamer playback pipeline");

        use_download_buffering(&playbin);
        playbin.set_property("video-sink", video::build_video_sink(video_output)?);

        let bus_watcher = spawn_bus_watcher(playbin.clone(), state.clone(), state_tx);
        connect_about_to_finish(&playbin, state, track_tx);

        Ok(Self {
            playbin,
            version,
            bus_watcher,
        })
    }

    fn shutdown(self) {
        self.bus_watcher.abort();
        let _ = self.playbin.set_state(gst::State::Null);
    }
}

pub struct GStreamerBackend {
    pipeline: Mutex<Pipeline>,
    // Kept so a pipeline rebuild (see `ensure_playbin_version`) wires the
    // new element's `video-sink` up the same way the first one was.
    video_output: Option<VideoOutput>,
    state: Arc<Mutex<PlayerState>>,
    // Fires on transitions GStreamer makes on its own — end-of-stream or a
    // pipeline error — as opposed to ones a `play`/`pause`/`stop` call
    // caused. The SOAP layer already publishes a `LastChange` event right
    // after each of those calls returns; this is what lets the *unprompted*
    // ones reach subscribed control points too, instead of only surfacing
    // on their next `GetTransportInfo` poll. See [`MediaBackend::state_changes`].
    state_tx: watch::Sender<TransportState>,
    // Fires when the queued next track gets gaplessly promoted to current —
    // see [`MediaBackend::track_changes`] for why this needs to be separate
    // from `state_tx` (the transport state doesn't change across a gapless
    // transition, only the URI/metadata does).
    track_tx: watch::Sender<TrackInfo>,
}

impl GStreamerBackend {
    pub fn new() -> Result<Self, MediaBackendError> {
        Self::build(None, PlaybinVersion::default())
    }

    /// Same as [`Self::new`], but with an explicit [`PlaybinVersion`]
    /// instead of the default - for tests or comparing the two directly.
    pub fn new_with_playbin_version(version: PlaybinVersion) -> Result<Self, MediaBackendError> {
        Self::build(None, version)
    }

    /// Same as [`Self::new`], but also displays decoded video in its own
    /// window via `video_output` - see [`super::video`] for how frames get
    /// from GStreamer's decoding thread to that window.
    pub fn new_with_video_output(video_output: VideoOutput) -> Result<Self, MediaBackendError> {
        Self::build(Some(video_output), PlaybinVersion::V3)
    }

    fn build(
        video_output: Option<VideoOutput>,
        playbin_version: PlaybinVersion,
    ) -> Result<Self, MediaBackendError> {
        ensure_initialized()?;

        let playbin_version = required_version(video_output.is_some(), playbin_version);

        let state = Arc::new(Mutex::new(PlayerState::default()));
        let (state_tx, _) = watch::channel(TransportState::NoMediaPresent);
        let (track_tx, _) = watch::channel(TrackInfo::default());

        let pipeline = Pipeline::build(
            playbin_version,
            video_output.clone(),
            state.clone(),
            state_tx.clone(),
            track_tx.clone(),
        )?;

        Ok(Self {
            pipeline: Mutex::new(pipeline),
            video_output,
            state,
            state_tx,
            track_tx,
        })
    }

    /// Rebuilds the pipeline with `desired`'s element if it isn't already
    /// running it, carrying volume and mute over to the replacement. A
    /// backend built with video output stays on `playbin3` regardless of
    /// `desired` - see [`PlaybinVersion::V3`].
    fn ensure_playbin_version(&self, desired: PlaybinVersion) -> Result<(), MediaBackendError> {
        let desired = required_version(self.video_output.is_some(), desired);

        let mut pipeline = self.pipeline.lock().unwrap();
        if pipeline.version == desired {
            return Ok(());
        }

        info!(
            from = pipeline.version.element_name(),
            to = desired.element_name(),
            "Switching GStreamer pipeline"
        );

        let volume: f64 = pipeline.playbin.property("volume");
        let mute: bool = pipeline.playbin.property("mute");

        let replacement = Pipeline::build(
            desired,
            self.video_output.clone(),
            self.state.clone(),
            self.state_tx.clone(),
            self.track_tx.clone(),
        )?;
        replacement.playbin.set_property("volume", volume);
        replacement.playbin.set_property("mute", mute);

        std::mem::replace(&mut *pipeline, replacement).shutdown();
        Ok(())
    }

    /// The current playbin/playbin3 element. Cheap to clone (a GObject
    /// reference, not the pipeline itself) - callers hold this instead of
    /// the lock while doing anything that isn't instant, so a concurrent
    /// `ensure_playbin_version` swap never blocks on them.
    fn playbin(&self) -> gst::Element {
        self.pipeline.lock().unwrap().playbin.clone()
    }
}

/// Video needs `playbin3` unconditionally; audio uses whichever version was
/// requested.
fn required_version(has_video_output: bool, requested: PlaybinVersion) -> PlaybinVersion {
    if has_video_output {
        PlaybinVersion::V3
    } else {
        requested
    }
}

/// Swaps the `buffering` flag for `download`. Everything this renderer
/// plays is a finite, seekable, on-demand file, never a live stream, so
/// `download` is the better fit: it caches to a local temp file as it
/// downloads, so a seek into anything already downloaded is instant with no
/// network wait, and playback isn't gated on an in-memory ring buffer
/// refilling to a percentage threshold - which on a source whose real
/// throughput is well under the link speed (a phone-hosted HTTP server on
/// gigabit ethernet, for instance - the link isn't the bottleneck, the
/// server's own disk/CPU is) can mean repeatedly pausing and resuming
/// playback while that threshold refills.
fn use_download_buffering(playbin: &gst::Element) {
    let flags = playbin.property_value("flags");
    let Some(flags_class) = gst::glib::FlagsClass::with_type(flags.type_()) else {
        warn!("playbin's \"flags\" property isn't a flags type; leaving buffering mode as-is");
        return;
    };

    let updated = flags_class
        .builder_with_value(flags)
        .map(|builder| builder.unset_by_nick("buffering").set_by_nick("download"))
        .and_then(|builder| builder.build());

    match updated {
        Some(flags) => playbin.set_property("flags", flags),
        None => warn!("Failed to switch playbin from buffering to download mode"),
    }
}

/// Whether `metadata`'s DIDL-Lite hints at video content, checked as a
/// plain substring search rather than XML parsing - it only ever decides
/// which pipeline to start a track with, nothing depends on it being exact.
fn metadata_suggests_video(metadata: Option<&str>) -> bool {
    metadata.is_some_and(|metadata| {
        metadata.contains(":video/") || metadata.contains("object.item.videoItem")
    })
}

impl Drop for GStreamerBackend {
    fn drop(&mut self) {
        // Best-effort: `abort()` only requests cancellation (it takes effect
        // next time the task is polled/yields), but nothing here depends on
        // the watcher having stopped by the time this returns.
        let pipeline = self.pipeline.get_mut().unwrap();
        pipeline.bus_watcher.abort();
        // Best-effort: release the audio device and any decoder resources.
        let _ = pipeline.playbin.set_state(gst::State::Null);
    }
}

/// Watches the bus for the messages that mean playback stopped without
/// anyone calling `stop()` — end-of-stream, and a pipeline error
/// (bad/missing codec, dropped network stream, etc.) — plus warnings, logged
/// for diagnostics but not otherwise acted on. Without the Eos/Error handling
/// here, `GetTransportInfo` would keep reporting `Playing` forever after a
/// track finishes naturally or a stream dies mid-playback.
///
/// Runs as a tokio task, polling the bus's normal async queue with a
/// non-blocking [`gst::Bus::pop_filtered`] on a short interval — deliberately
/// *not* [`gst::Bus::stream_filtered`]/[`gst::Bus::stream`], which install a
/// sync bus handler. A sync handler's callback runs synchronously on
/// whichever thread posts the message — for `playbin` that includes its own
/// internal streaming/decoding threads — and fires for *every* bus message
/// (state changes, QOS, tags, ...), not just the ones filtered for here.
/// Doing channel/allocation work inline on those threads for every message
/// was audible as playback stutter. Polling instead only ever touches
/// GStreamer's lightweight default async enqueue, from this task's own
/// thread.
fn spawn_bus_watcher(
    playbin: gst::Element,
    state: Arc<Mutex<PlayerState>>,
    state_tx: watch::Sender<TransportState>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Some(bus) = playbin.bus() else {
            return;
        };

        let message_types = [
            gst::MessageType::Eos,
            gst::MessageType::Error,
            gst::MessageType::Warning,
        ];
        let mut interval = tokio::time::interval(Duration::from_millis(100));

        loop {
            interval.tick().await;

            // Drain every matching message queued since the last tick, not
            // just one — a burst (e.g. a warning right before an error)
            // shouldn't have to wait out extra ticks to be seen.
            while let Some(msg) = bus.pop_filtered(&message_types) {
                match msg.view() {
                    gst::MessageView::Eos(_) => {
                        info!("GStreamer reported end-of-stream, stopping");
                        stop_after_unprompted_end(&playbin, &state, &state_tx, TransportStatus::OK);
                    }
                    gst::MessageView::Error(err) => {
                        error!(
                            error = %err.error(),
                            debug = ?err.debug(),
                            "GStreamer pipeline error, stopping playback"
                        );
                        stop_after_unprompted_end(
                            &playbin,
                            &state,
                            &state_tx,
                            TransportStatus::ErrorOccurred,
                        );
                    }
                    gst::MessageView::Warning(warning) => {
                        warn!(
                            warning = %warning.error(),
                            debug = ?warning.debug(),
                            "GStreamer pipeline warning"
                        );
                    }
                    _ => {}
                }
            }
        }
    })
}

/// Shared cleanup for both bus-watcher cases: drop the pipeline back to
/// Ready (same as a caller-driven `stop()`), update the state/status we
/// report to `GetTransportInfo`, and push the state out to anyone
/// subscribed via [`MediaBackend::state_changes`]. `status` distinguishes
/// the two callers: a clean end-of-stream is still `TransportStatus::OK`
/// (playback simply finished), a pipeline error is `ErrorOccurred`.
fn stop_after_unprompted_end(
    playbin: &gst::Element,
    state: &Arc<Mutex<PlayerState>>,
    state_tx: &watch::Sender<TransportState>,
    status: TransportStatus,
) {
    let _ = playbin.set_state(gst::State::Ready);

    let new_state = {
        let mut state = state.lock().unwrap();
        state.transport_state = if state.uri.is_some() {
            TransportState::Stopped
        } else {
            TransportState::NoMediaPresent
        };
        state.status = status;
        state.transport_state
    };

    let _ = state_tx.send(new_state);
}

/// Connects `playbin`'s `about-to-finish` signal, which is how GStreamer
/// itself recommends doing gapless playback: it fires a couple of seconds
/// before end-of-stream, on GStreamer's streaming thread, and the
/// handler sets the `uri` property to the next track right there
/// — `playbin` then splices it in with no audible gap.
///
/// If an explicit `SetNextAVTransportURI` is queued, that always takes
/// priority. Otherwise, if `CurrentPlayMode` is `REPEAT_ONE`/`REPEAT_ALL`,
/// the current track is re-queued so it loops gaplessly — this renderer has
/// no playlist beyond the current track, so "repeat" and "repeat all" both
/// reduce to "loop what's playing now". Anything else (`NORMAL`/`SHUFFLE`,
/// which has no queue to shuffle within), we do nothing and `playbin` plays
/// out to EOS as normal, same as before this existed.
///
/// A gapless transition stays on the current pipeline element even if the
/// next track needs the other [`PlaybinVersion`] - switching pipelines mid
/// gapless-splice isn't supported. An explicit `SetAVTransportURI` (the
/// common case) always goes through [`GStreamerBackend::load_media`], which
/// does pick the right version.
fn connect_about_to_finish(
    playbin: &gst::Element,
    state: Arc<Mutex<PlayerState>>,
    track_tx: watch::Sender<TrackInfo>,
) {
    let playbin_for_signal = playbin.clone();
    playbin.connect("about-to-finish", false, move |_args| {
        let next = {
            let mut state = state.lock().unwrap();
            if let Some(next) = state.next.take() {
                state.uri = Some(next.uri.clone());
                state.metadata = next.metadata.clone();
                Some(next)
            } else if matches!(state.play_mode, PlayMode::RepeatOne | PlayMode::RepeatAll) {
                state.uri.clone().map(|uri| TrackInfo {
                    uri,
                    metadata: state.metadata.clone(),
                })
            } else {
                None
            }
        };

        let next = next?;

        info!(uri = %next.uri, "Gapless: promoting queued next track to current");
        playbin_for_signal.set_property("uri", &next.uri);
        let _ = track_tx.send(next);

        None
    });
}

/// `gst::init()` must run once per process before any element is created,
/// and isn't itself safe to call concurrently from multiple backends.
pub(super) fn ensure_initialized() -> Result<(), MediaBackendError> {
    static INIT: OnceLock<Result<(), String>> = OnceLock::new();
    INIT.get_or_init(|| gst::init().map_err(|e| e.to_string()))
        .clone()
        .map_err(MediaBackendError::GStreamer)
}

fn state_change_error(err: gst::StateChangeError) -> MediaBackendError {
    MediaBackendError::GStreamer(format!("State change failed: {}", err))
}

#[async_trait]
impl MediaBackend for GStreamerBackend {
    async fn load_media(
        &self,
        uri: &str,
        metadata: Option<String>,
    ) -> Result<(), MediaBackendError> {
        let desired_version = if metadata_suggests_video(metadata.as_deref()) {
            PlaybinVersion::V3
        } else {
            PlaybinVersion::V1
        };
        self.ensure_playbin_version(desired_version)?;
        let playbin = self.playbin();

        // Drop back to Ready before switching URIs — playbin doesn't support
        // changing `uri` while past that state.
        playbin
            .set_state(gst::State::Ready)
            .map_err(state_change_error)?;

        playbin.set_property("uri", uri);

        {
            let mut state = self.state.lock().unwrap();
            state.uri = Some(uri.to_string());
            state.metadata = metadata;
            state.transport_state = TransportState::Stopped;
            // A fresh SetAVTransportURI is a new attempt regardless of how
            // the previous track ended — don't carry a stale error forward.
            state.status = TransportStatus::OK;
            // A new track starts at normal speed regardless of what the
            // previous one was playing at.
            state.speed = 1.0;
            // An explicit SetAVTransportURI starts a new "current track"
            // context — whatever was previously queued to follow the old
            // track no longer applies.
            state.next = None;
        }

        // Kick off preroll into Paused so playback can start immediately
        // once Play is called — fire-and-forget. `set_state` only requests
        // the transition; the actual work (caps negotiation, etc.) happens
        // asynchronously on GStreamer's own threads afterwards and can take
        // a little while. We deliberately don't block this call on it: the
        // `transport_state` we just set above (not a query back to
        // GStreamer) is what get_playback_state()/GetTransportInfo reports,
        // so callers see a prompt, consistent answer regardless of how far
        // GStreamer has actually gotten.
        //
        // Note duration specifically is not guaranteed to be known even
        // once PAUSED is reached: for formats without an explicit duration
        // header (plain MP3 without a Xing/VBRI frame, for instance),
        // GStreamer only resolves it once bytes are actually flowing, i.e.
        // some time after Play. get_duration() reflects that honestly by
        // returning None until GStreamer itself knows.
        playbin
            .set_state(gst::State::Paused)
            .map_err(state_change_error)?;

        Ok(())
    }

    async fn play(&self) -> Result<(), MediaBackendError> {
        self.playbin()
            .set_state(gst::State::Playing)
            .map_err(state_change_error)?;
        self.state.lock().unwrap().transport_state = TransportState::Playing;
        Ok(())
    }

    async fn pause(&self) -> Result<(), MediaBackendError> {
        self.playbin()
            .set_state(gst::State::Paused)
            .map_err(state_change_error)?;
        self.state.lock().unwrap().transport_state = TransportState::PausedPlayback;
        Ok(())
    }

    async fn stop(&self) -> Result<(), MediaBackendError> {
        self.playbin()
            .set_state(gst::State::Ready)
            .map_err(state_change_error)?;
        let mut state = self.state.lock().unwrap();
        state.transport_state = if state.uri.is_some() {
            TransportState::Stopped
        } else {
            TransportState::NoMediaPresent
        };
        state.speed = 1.0;
        Ok(())
    }

    async fn get_position(&self) -> Result<Duration, MediaBackendError> {
        Ok(self
            .playbin()
            .query_position::<gst::ClockTime>()
            .map(|t| Duration::from_nanos(t.nseconds()))
            .unwrap_or_default())
    }

    async fn seek(&self, position: Duration) -> Result<(), MediaBackendError> {
        let target = gst::ClockTime::from_nseconds(position.as_nanos() as u64);
        self.playbin()
            .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, target)
            .map_err(|e| MediaBackendError::GStreamer(format!("Seek failed: {}", e)))
    }

    async fn get_duration(&self) -> Result<Option<Duration>, MediaBackendError> {
        Ok(self
            .playbin()
            .query_duration::<gst::ClockTime>()
            .map(|t| Duration::from_nanos(t.nseconds())))
    }

    async fn get_playback_state(&self) -> Result<TransportState, MediaBackendError> {
        // The state we set the moment each command was accepted, not a
        // query back to GStreamer — its own internal state lags behind
        // whatever we last told it to do (see the comments in load_media,
        // play, pause and stop), so querying it here would report stale
        // values for a little while after every transition.
        Ok(self.state.lock().unwrap().transport_state)
    }

    async fn get_status(&self) -> Result<TransportStatus, MediaBackendError> {
        Ok(self.state.lock().unwrap().status)
    }

    async fn set_play_speed(&self, speed: f64) -> Result<(), MediaBackendError> {
        if speed <= 0.0 {
            // Reverse/zero-rate playback needs every element in the chain
            // (source, demuxer, decoder) to support negative-rate seeking,
            // which most audio decoders don't — rejecting outright is more
            // honest than a seek that silently does nothing or errors deep
            // in the pipeline.
            return Err(MediaBackendError::Unsupported(format!(
                "Reverse or zero playback speed is not supported: {}",
                speed
            )));
        }

        // The AVTransport handler always reissues Speed after Play (so it
        // still takes effect if a control point is scrubbing speed
        // mid-playback) so we can do nothing if it's not changing.
        if (self.state.lock().unwrap().speed - speed).abs() < f64::EPSILON {
            return Ok(());
        }

        let playbin = self.playbin();

        // Re-seek at the new rate from the current position — GStreamer has
        // no separate "just change the rate" call; a seek is how rate
        // changes are done, same as a position change.
        let position = playbin
            .query_position::<gst::ClockTime>()
            .unwrap_or(gst::ClockTime::ZERO);

        playbin
            .seek(
                speed,
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::SeekType::Set,
                position,
                gst::SeekType::None,
                gst::ClockTime::NONE,
            )
            .map_err(|e| MediaBackendError::GStreamer(format!("Speed change failed: {}", e)))?;

        self.state.lock().unwrap().speed = speed;
        Ok(())
    }

    async fn get_play_speed(&self) -> Result<f64, MediaBackendError> {
        Ok(self.state.lock().unwrap().speed)
    }

    async fn set_play_mode(&self, mode: PlayMode) -> Result<(), MediaBackendError> {
        self.state.lock().unwrap().play_mode = mode;
        Ok(())
    }

    async fn get_play_mode(&self) -> Result<PlayMode, MediaBackendError> {
        Ok(self.state.lock().unwrap().play_mode)
    }

    async fn set_volume(&self, volume: u8) -> Result<(), MediaBackendError> {
        let linear = (volume as f64 / 100.0).clamp(0.0, 1.0);
        self.playbin().set_property("volume", linear);
        Ok(())
    }

    async fn get_volume(&self) -> Result<u8, MediaBackendError> {
        let linear: f64 = self.playbin().property("volume");
        Ok((linear.clamp(0.0, 1.0) * 100.0).round() as u8)
    }

    async fn set_mute(&self, muted: bool) -> Result<(), MediaBackendError> {
        self.playbin().set_property("mute", muted);
        Ok(())
    }

    async fn get_mute(&self) -> Result<bool, MediaBackendError> {
        Ok(self.playbin().property("mute"))
    }

    async fn get_current_uri(&self) -> Result<Option<String>, MediaBackendError> {
        Ok(self.state.lock().unwrap().uri.clone())
    }

    async fn get_metadata(&self) -> Result<Option<String>, MediaBackendError> {
        Ok(self.state.lock().unwrap().metadata.clone())
    }

    fn state_changes(&self) -> watch::Receiver<TransportState> {
        self.state_tx.subscribe()
    }

    async fn set_next_media(
        &self,
        uri: &str,
        metadata: Option<String>,
    ) -> Result<(), MediaBackendError> {
        self.state.lock().unwrap().next = Some(TrackInfo {
            uri: uri.to_string(),
            metadata,
        });
        Ok(())
    }

    async fn clear_next_media(&self) -> Result<(), MediaBackendError> {
        self.state.lock().unwrap().next = None;
        Ok(())
    }

    async fn get_next_media(&self) -> Result<Option<TrackInfo>, MediaBackendError> {
        Ok(self.state.lock().unwrap().next.clone())
    }

    fn track_changes(&self) -> watch::Receiver<TrackInfo> {
        self.track_tx.subscribe()
    }
}

// Tests that need real audio playback (correctness) or wall-clock latency
// budgets (performance regressions) live in `tests/gstreamer_playback_test.rs`
// and `tests/gstreamer_performance_test.rs` respectively — they only ever
// touch `GStreamerBackend` through its public `MediaBackend` API, so they
// belong as integration tests rather than unit tests here. What's left below
// are the unit tests that don't need real hardware/audio.
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reports_no_media_present_before_anything_is_loaded() {
        let backend = GStreamerBackend::new().unwrap();
        assert_eq!(
            backend.get_playback_state().await.unwrap(),
            TransportState::NoMediaPresent
        );
        assert_eq!(backend.get_current_uri().await.unwrap(), None);
    }

    #[tokio::test]
    async fn volume_and_mute_round_trip() {
        let backend = GStreamerBackend::new().unwrap();

        backend.set_volume(42).await.unwrap();
        assert_eq!(backend.get_volume().await.unwrap(), 42);

        backend.set_mute(true).await.unwrap();
        assert!(backend.get_mute().await.unwrap());

        backend.set_mute(false).await.unwrap();
        assert!(!backend.get_mute().await.unwrap());
    }

    #[test]
    fn detects_video_from_res_protocol_info() {
        let metadata = r#"<res protocolInfo="http-get:*:video/x-matroska:*">http://x</res>"#;
        assert!(metadata_suggests_video(Some(metadata)));
    }

    #[test]
    fn detects_video_from_upnp_class() {
        let metadata = "<upnp:class>object.item.videoItem</upnp:class>";
        assert!(metadata_suggests_video(Some(metadata)));
    }

    #[test]
    fn does_not_flag_audio_metadata_as_video() {
        let metadata = r#"<res protocolInfo="http-get:*:audio/mpeg:*">http://x</res>"#;
        assert!(!metadata_suggests_video(Some(metadata)));
        assert!(!metadata_suggests_video(None));
    }

    #[tokio::test]
    async fn switches_playbin_version_between_audio_and_video_loads() {
        let backend = GStreamerBackend::new().unwrap();
        assert_eq!(
            backend.pipeline.lock().unwrap().version,
            PlaybinVersion::V1
        );

        // The version switch is a side effect of `load_media` regardless of
        // whether the URI itself turns out to be playable - a nonexistent
        // path is enough to test it without a real media fixture, so the
        // playback result itself is deliberately ignored here.
        let _ = backend
            .load_media(
                "file:///nonexistent.mkv",
                Some(r#"<res protocolInfo="http-get:*:video/x-matroska:*"/>"#.to_string()),
            )
            .await;
        assert_eq!(
            backend.pipeline.lock().unwrap().version,
            PlaybinVersion::V3
        );

        let _ = backend
            .load_media(
                "file:///nonexistent.mp3",
                Some(r#"<res protocolInfo="http-get:*:audio/mpeg:*"/>"#.to_string()),
            )
            .await;
        assert_eq!(
            backend.pipeline.lock().unwrap().version,
            PlaybinVersion::V1
        );
    }

    #[test]
    fn switches_buffering_flag_for_download() {
        gst::init().unwrap();
        let playbin = gst::ElementFactory::make("playbin3").build().unwrap();
        use_download_buffering(&playbin);

        let flags: u32 = playbin
            .property_value("flags")
            .transform::<u32>()
            .unwrap()
            .get::<u32>()
            .unwrap();

        assert_eq!(flags & 0x100, 0, "buffering bit should be cleared");
        assert_eq!(flags & 0x80, 0x80, "download bit should be set");
    }
}
