//! [`MediaBackend`] implementation using GStreamer for playback
//! Requires it to be installed locally

use async_trait::async_trait;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tracing::{debug, error, info, trace, warn};

use super::cache_src::{self, ChunkCache, Prefetcher};
use super::video::{self, VideoOutput};
use crate::backend::{MediaBackend, MediaBackendError, TrackInfo};
use crate::server::soap::types::av_transport::{PlayMode, TransportState, TransportStatus};

/// Used by every constructor that doesn't take an explicit budget
/// (`new()`/`new_with_playbin_version()`, only ever used by tests and the
/// no-video-output dummy-fallback path) - `new_with_video_output` (what a
/// real run actually uses) takes an explicit, operator-tunable budget
/// instead. See [`cache_src`] for what this bounds and why.
const DEFAULT_CACHE_BUDGET_BYTES: u64 = 256 * 1024 * 1024;

/// How long a single call on the dedicated GStreamer pipeline thread
/// (`run_on_gst_thread`) gets before it's treated as wedged rather than
/// just slow - see that function's doc for what happens (and what doesn't)
/// once this fires.
const GST_CALL_TIMEOUT: Duration = Duration::from_secs(8);

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
    // The current track's streams, as reported by GStreamer's
    // `GST_MESSAGE_STREAM_COLLECTION` bus message - `None` until that
    // arrives (shortly after `load_media`, once decodebin3 knows what the
    // file contains). `SubtitleControl` needs this: toggling a subtitle
    // track on/off is done by re-sending a `select-streams` event listing
    // which of these stream ids should be active, not a simple property
    // (unlike `playbin` v1's `current-text` - `playbin3` has no such
    // property, this is the only API it exposes for stream selection).
    stream_collection: Option<gst::StreamCollection>,
    // Whether the subtitle track (if any) should be included the next time
    // a `select-streams` event is sent - reset to `true` by `load_media`
    // for each new track, since there's nothing UPnP-visible carrying a
    // per-track "no subtitles" preference forward.
    subtitles_enabled: bool,
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
            stream_collection: None,
            subtitles_enabled: true,
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

/// Everything a pipeline (re)build needs beyond the [`PlaybinVersion`]
/// itself, bundled into one clonable value rather than threaded through as
/// six-plus separate parameters - `clippy::too_many_arguments` flagged the
/// unbundled version, rightly: several of these are same-typed
/// (`Arc<Mutex<_>>`/`Sender<_>`) and easy to mis-order positionally.
/// Cloning is cheap - every field is itself a handle (`Arc`, `Sender`, or a
/// `reqwest::Client`, which is internally reference-counted).
#[derive(Clone)]
struct BackendHandles {
    video_output: Option<VideoOutput>,
    video_frame_counter: Arc<AtomicU64>,
    // Backend-lifetime, shared with every `upnpcachehttpsrc` instance across
    // every pipeline rebuild - see `cache_src`'s module docs for why this
    // can't live inside the element/pipeline itself.
    media_cache: Arc<Mutex<ChunkCache>>,
    http_client: reqwest::Client,
    prefetcher: Arc<Prefetcher>,
    state: Arc<Mutex<PlayerState>>,
    state_tx: watch::Sender<TransportState>,
    track_tx: watch::Sender<TrackInfo>,
}

/// The GStreamer element plus what has to be rebuilt alongside it whenever
/// the element itself is swapped for a different [`PlaybinVersion`].
struct Pipeline {
    playbin: gst::Element,
    version: PlaybinVersion,
    bus_watcher: tokio::task::JoinHandle<()>,
}

impl Pipeline {
    fn build(version: PlaybinVersion, handles: &BackendHandles) -> Result<Self, MediaBackendError> {
        let element_name = version.element_name();
        let playbin = gst::ElementFactory::make(element_name)
            .name("upnp-receiver-playbin")
            .build()
            .map_err(|e| {
                MediaBackendError::GStreamer(format!("Failed to create {element_name}: {e}"))
            })?;
        info!(playbin = element_name, "Building GStreamer playback pipeline");

        // `playbin` fires this for every source it creates or reuses,
        // including on a gapless/playlist URI change - `cache_src::
        // inject_shared` is a no-op for anything that isn't our
        // `upnpcachehttpsrc` (e.g. a `file://` track), so this is safe to
        // connect unconditionally. The cache/client are backend-lifetime,
        // not tied to this pipeline, so they survive this element being
        // torn down and rebuilt (audio/video swap, decoder-stall recovery).
        let shared = cache_src::Shared {
            cache: handles.media_cache.clone(),
            client: handles.http_client.clone(),
            prefetcher: handles.prefetcher.clone(),
        };
        playbin.connect("source-setup", false, move |args| {
            let source = args[1].get::<gst::Element>().expect("source-setup's argument is the new source element");
            cache_src::inject_shared(&source, shared.clone());
            None
        });

        // Only a V3 (video) pipeline gets the real GL-accelerated video-sink -
        // a V1 (audio) one always gets `fakesink`, even when `video_output`
        // is available, so an audio track never spins up a second, idle GL
        // context/appsink chain alongside whatever the video pipeline it's
        // being swapped with is using.
        let video_sink_output = match version {
            PlaybinVersion::V3 => handles.video_output.clone(),
            PlaybinVersion::V1 => None,
        };
        playbin.set_property(
            "video-sink",
            video::build_video_sink(video_sink_output, handles.video_frame_counter.clone())?,
        );

        let bus_watcher = spawn_bus_watcher(playbin.clone(), handles.state.clone(), handles.state_tx.clone());
        connect_about_to_finish(&playbin, handles.state.clone(), handles.track_tx.clone());

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
    // `Arc` so the decoder-stall watchdog (`run_decoder_watchdog`) can hold
    // its own handle and rebuild the pipeline in place without needing
    // `&GStreamerBackend` - it's spawned from inside `Self::build`, before
    // any `Arc<Self>`/`&Self` exists to give it.
    pipeline: Arc<Mutex<Pipeline>>,
    // Kept so a pipeline rebuild (see `ensure_playbin_version`) wires the
    // new element's `video-sink` up the same way the first one was.
    video_output: Option<VideoOutput>,
    // Bumped by the video-sink's appsink callback on every frame actually
    // delivered to the window - see `run_decoder_watchdog`, which watches
    // this stall (while playing a video track) as the sign of a wedged
    // hardware decoder session (observed after a backward seek) and
    // rebuilds the pipeline to recover.
    video_frame_counter: Arc<AtomicU64>,
    // Backend-lifetime, shared with every `upnpcachehttpsrc` instance across
    // every pipeline rebuild - see `cache_src`'s module docs for why this
    // can't live inside the element/pipeline itself.
    media_cache: Arc<Mutex<ChunkCache>>,
    http_client: reqwest::Client,
    prefetcher: Arc<Prefetcher>,
    watchdog: tokio::task::JoinHandle<()>,
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
    // Every blocking `set_state`/`seek` call for this backend's pipeline
    // runs on the single dedicated OS thread behind this channel (see
    // `run_on_gst_thread`) rather than Tokio's blocking-task pool, which
    // hands each call to a different, arbitrary pool thread - GL/Metal-
    // backed elements (`glupload`/`gldownload` in the video pipeline,
    // sharing a GL context with the window's own wgpu renderer) can
    // require consistent thread affinity that a rotating pool can't give.
    gst_thread_tx: mpsc::Sender<GstJob>,
}

impl GStreamerBackend {
    pub fn new() -> Result<Self, MediaBackendError> {
        Self::build(None, PlaybinVersion::default(), DEFAULT_CACHE_BUDGET_BYTES)
    }

    /// Same as [`Self::new`], but with an explicit [`PlaybinVersion`]
    /// instead of the default - for tests or comparing the two directly.
    pub fn new_with_playbin_version(version: PlaybinVersion) -> Result<Self, MediaBackendError> {
        Self::build(None, version, DEFAULT_CACHE_BUDGET_BYTES)
    }

    /// Same as [`Self::new`], but also displays decoded video in its own
    /// window via `video_output` - see [`super::video`] for how frames get
    /// from GStreamer's decoding thread to that window. `cache_budget_bytes`
    /// bounds the in-memory chunk cache every HTTP(S) track's bytes flow
    /// through (see [`super::cache_src`]) - operator-tunable since the right
    /// size depends entirely on the device (a Pi wants this small, anything
    /// with real RAM to spare can afford more).
    pub fn new_with_video_output(
        video_output: VideoOutput,
        cache_budget_bytes: u64,
    ) -> Result<Self, MediaBackendError> {
        Self::build(Some(video_output), PlaybinVersion::V3, cache_budget_bytes)
    }

    fn build(
        video_output: Option<VideoOutput>,
        playbin_version: PlaybinVersion,
        cache_budget_bytes: u64,
    ) -> Result<Self, MediaBackendError> {
        ensure_initialized()?;

        let handles = BackendHandles {
            video_output,
            video_frame_counter: Arc::new(AtomicU64::new(0)),
            media_cache: Arc::new(Mutex::new(ChunkCache::new(cache_budget_bytes))),
            http_client: reqwest::Client::new(),
            prefetcher: Arc::new(Prefetcher::new()),
            state: Arc::new(Mutex::new(PlayerState::default())),
            state_tx: watch::channel(TransportState::NoMediaPresent).0,
            track_tx: watch::channel(TrackInfo::default()).0,
        };

        let pipeline = Pipeline::build(playbin_version, &handles)?;
        let pipeline = Arc::new(Mutex::new(pipeline));

        let watchdog = tokio::spawn(run_decoder_watchdog(pipeline.clone(), handles.clone()));

        Ok(Self {
            pipeline,
            video_output: handles.video_output,
            video_frame_counter: handles.video_frame_counter,
            media_cache: handles.media_cache,
            http_client: handles.http_client,
            prefetcher: handles.prefetcher,
            watchdog,
            state: handles.state,
            state_tx: handles.state_tx,
            track_tx: handles.track_tx,
            gst_thread_tx: spawn_gst_thread(),
        })
    }

    /// Runs a synchronous GStreamer call (`set_state`, `seek`, ...) on this
    /// backend's single dedicated pipeline thread - see the `gst_thread_tx`
    /// field doc for why that's a persistent thread rather than
    /// `tokio::task::spawn_blocking`.
    ///
    /// Bounded by [`GST_CALL_TIMEOUT`]: this dedicated thread processes
    /// jobs strictly in submission order, so a call that never returns
    /// (a genuinely wedged pipeline, not just a slow one) wouldn't just
    /// fail on its own - every *subsequent* call would queue up behind it
    /// and hang too, taking down playback control entirely with no
    /// feedback. Timing out here can't un-wedge the pipeline itself, but
    /// it turns a silent, permanent freeze into a clear, immediate error
    /// this backend's caller (and so the SOAP layer, and so the video
    /// window's menu) can actually see and log.
    async fn run_on_gst_thread<T: Send + 'static>(
        &self,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, MediaBackendError> {
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        let job: GstJob = Box::new(move || {
            let _ = result_tx.send(f());
        });
        self.gst_thread_tx.send(job).map_err(|_| {
            MediaBackendError::GStreamer("GStreamer pipeline thread is gone".to_string())
        })?;

        match tokio::time::timeout(GST_CALL_TIMEOUT, result_rx).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => Err(MediaBackendError::GStreamer(
                "GStreamer pipeline thread dropped the response".to_string(),
            )),
            Err(_) => {
                error!(
                    timeout_secs = GST_CALL_TIMEOUT.as_secs(),
                    "GStreamer pipeline call did not complete in time - the dedicated pipeline \
                     thread is likely wedged; every subsequent call to it will keep timing out \
                     the same way until the process is restarted"
                );
                Err(MediaBackendError::GStreamer(format!(
                    "GStreamer operation timed out after {}s - the pipeline appears stuck",
                    GST_CALL_TIMEOUT.as_secs()
                )))
            }
        }
    }

    /// Clones this backend's shared handles into a [`BackendHandles`] for a
    /// `Pipeline::build` call - see that struct's docs for why it exists.
    fn handles(&self) -> BackendHandles {
        BackendHandles {
            video_output: self.video_output.clone(),
            video_frame_counter: self.video_frame_counter.clone(),
            media_cache: self.media_cache.clone(),
            http_client: self.http_client.clone(),
            prefetcher: self.prefetcher.clone(),
            state: self.state.clone(),
            state_tx: self.state_tx.clone(),
            track_tx: self.track_tx.clone(),
        }
    }

    /// Rebuilds the pipeline with `desired`'s element if it isn't already
    /// running it, carrying volume and mute over to the replacement. `desired`
    /// is purely a per-track decision (`load_media` picks it from the new
    /// track's metadata) - a backend built with a video output plays audio
    /// tracks on `playbin` too, since `playbin3` has no benefit for audio and
    /// noticeably laggier seeking than `playbin`.
    fn ensure_playbin_version(&self, desired: PlaybinVersion) -> Result<(), MediaBackendError> {
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

        let replacement = Pipeline::build(desired, &self.handles())?;
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

/// Local (non-UPnP) subtitle on/off toggle for the video window's menu -
/// its own trait rather than a `MediaBackend` method, since it's a
/// GStreamer/video pipeline property, not a capability every backend has a
/// concept of.
pub trait SubtitleControl: Send + Sync {
    /// Flips the current subtitle track on/off, returning the new state.
    fn toggle_subtitles(&self) -> bool;
    fn subtitles_enabled(&self) -> bool;
}

impl SubtitleControl for GStreamerBackend {
    fn toggle_subtitles(&self) -> bool {
        let enabled = {
            let mut player_state = self.state.lock().unwrap();
            player_state.subtitles_enabled = !player_state.subtitles_enabled;
            player_state.subtitles_enabled
        };
        apply_stream_selection(&self.playbin(), &self.state, enabled);
        enabled
    }

    fn subtitles_enabled(&self) -> bool {
        self.state.lock().unwrap().subtitles_enabled
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
        // either task having stopped by the time this returns.
        self.watchdog.abort();
        // Locked rather than `get_mut` - the watchdog task holds its own
        // clone of this `Arc`, so it's not necessarily the sole owner here.
        let pipeline = self.pipeline.lock().unwrap();
        pipeline.bus_watcher.abort();
        // Best-effort: release the audio device and any decoder resources.
        let _ = pipeline.playbin.set_state(gst::State::Null);
    }
}

/// How often [`run_decoder_watchdog`] checks whether a video frame has
/// arrived since its last check.
const DECODER_STALL_CHECK_INTERVAL: Duration = Duration::from_millis(500);
/// Consecutive stalled checks (no new frame while a video track is playing)
/// before the decoder is treated as wedged rather than just between
/// frames - about 2 seconds at [`DECODER_STALL_CHECK_INTERVAL`], generous
/// enough that ordinary frame pacing never trips it. Only applies once at
/// least one frame has already arrived for the current pipeline - see
/// [`DECODER_STARTUP_STALL_CHECKS`] for before that.
const DECODER_STALL_CHECKS_BEFORE_RECOVERY: u32 = 4;
/// Same idea, but for before the *first* frame of a fresh pipeline has ever
/// arrived - about 10 seconds. A real network fetch (the first chunk of a
/// track, possibly over a slow phone-hosted DLNA server) can legitimately
/// take longer than the couple of seconds [`DECODER_STALL_CHECKS_BEFORE_RECOVERY`]
/// allows once frames are already flowing - that's normal startup latency,
/// not the hardware-decoder wedging this watchdog exists to catch.
const DECODER_STARTUP_STALL_CHECKS: u32 = 20;
/// How long to hold off re-checking after a recovery rebuild - covers the
/// new pipeline's own preroll, which briefly looks like a stall too.
const DECODER_RECOVERY_GRACE_PERIOD: Duration = Duration::from_secs(3);
/// Give up auto-recovering after this many rebuilds in a row for the same
/// stall - if it's still stuck after this many attempts, rebuilding again
/// isn't going to help, so this stops spinning and leaves it stopped for
/// the next explicit `SetAVTransportURI`/`Play` to sort out.
const DECODER_MAX_CONSECUTIVE_RECOVERIES: u32 = 3;

/// Watches for a wedged hardware video decoder and rebuilds the pipeline to
/// recover, in place, at the position it got stuck.
///
/// Apple's VideoToolbox decoder (`vtdec`) has been observed to leave its
/// hardware decode session corrupted after a *backward* flushing seek: from
/// that point on every frame comes back flagged as an error
/// (`GstVideoDecoder`'s "Got frame N with an error flag", logged under the
/// `videodecoder` category) and no further frames ever reach the appsink -
/// the picture just freezes, silently, with no bus error to react to.
/// There's nothing to fix on this side of that bug, so recovery is
/// pragmatic: notice the video-sink has gone quiet while a video track is
/// supposed to be playing, and rebuild the pipeline (which recreates
/// `vtdec`'s session from scratch) at the same position, same as
/// `ensure_playbin_version` already does for an audio/video switch - just
/// forced rather than gated on the [`PlaybinVersion`] actually changing.
async fn run_decoder_watchdog(pipeline: Arc<Mutex<Pipeline>>, handles: BackendHandles) {
    let mut interval = tokio::time::interval(DECODER_STALL_CHECK_INTERVAL);
    let mut last_seen = handles.video_frame_counter.load(Ordering::Relaxed);
    let mut stalled_checks: u32 = 0;
    let mut consecutive_recoveries: u32 = 0;
    // Whether at least one frame has already arrived for the *current*
    // pipeline - false right after a fresh load or a recovery rebuild,
    // which both need `DECODER_STARTUP_STALL_CHECKS`' longer grace instead
    // of the tighter `DECODER_STALL_CHECKS_BEFORE_RECOVERY`.
    let mut has_seen_frame = false;

    loop {
        interval.tick().await;

        let watching_video = {
            let playing = handles.state.lock().unwrap().transport_state == TransportState::Playing;
            pipeline.lock().unwrap().version == PlaybinVersion::V3 && playing
        };
        if !watching_video {
            last_seen = handles.video_frame_counter.load(Ordering::Relaxed);
            stalled_checks = 0;
            consecutive_recoveries = 0;
            has_seen_frame = false;
            continue;
        }

        let current = handles.video_frame_counter.load(Ordering::Relaxed);
        if current != last_seen {
            last_seen = current;
            stalled_checks = 0;
            consecutive_recoveries = 0;
            has_seen_frame = true;
            continue;
        }

        stalled_checks += 1;
        let stall_threshold = if has_seen_frame {
            DECODER_STALL_CHECKS_BEFORE_RECOVERY
        } else {
            DECODER_STARTUP_STALL_CHECKS
        };
        if stalled_checks < stall_threshold {
            continue;
        }
        stalled_checks = 0;

        // Still stuck after every rebuild attempt already used - rebuilding
        // again isn't going to help, so stop for real instead of leaving
        // `GetTransportInfo` claiming `Playing` forever over a frozen
        // picture. This is the same cleanup an unprompted EOS/pipeline
        // error already does, so it surfaces to a subscribed control point
        // the same way. `stalled_checks`/`consecutive_recoveries` reset
        // themselves on the next tick once `transport_state` is no longer
        // `Playing` (the `!watching_video` branch above).
        if consecutive_recoveries >= DECODER_MAX_CONSECUTIVE_RECOVERIES {
            error!(
                attempts = consecutive_recoveries,
                "Video decoder still stalled after repeated automatic recovery; stopping playback"
            );
            let playbin = pipeline.lock().unwrap().playbin.clone();
            stop_after_unprompted_end(&playbin, &handles.state, &handles.state_tx, TransportStatus::ErrorOccurred);
            continue;
        }
        consecutive_recoveries += 1;

        warn!(
            attempt = consecutive_recoveries,
            stalled_for = ?(DECODER_STALL_CHECK_INTERVAL * stall_threshold),
            "Video decoder appears stuck; rebuilding the pipeline to recover"
        );

        if let Err(err) = recover_stuck_video_pipeline(&pipeline, &handles) {
            error!(error = %err, "Failed to recover from a stuck video decoder");
        }

        // The rebuilt pipeline starts from zero frames too - it gets its
        // own startup grace, not the tighter post-warm-up threshold.
        has_seen_frame = false;
        last_seen = handles.video_frame_counter.load(Ordering::Relaxed);
        tokio::time::sleep(DECODER_RECOVERY_GRACE_PERIOD).await;
    }
}

/// Rebuilds the current pipeline in place at the same [`PlaybinVersion`],
/// resuming at the position and (optimistic) `Playing` state it was stuck
/// at - see [`run_decoder_watchdog`] for why this is needed at all.
fn recover_stuck_video_pipeline(
    pipeline: &Arc<Mutex<Pipeline>>,
    handles: &BackendHandles,
) -> Result<(), MediaBackendError> {
    let uri = handles.state.lock().unwrap().uri.clone().ok_or_else(|| {
        MediaBackendError::GStreamer("No URI to resume after decoder recovery".to_string())
    })?;

    let (position, volume, mute) = {
        let pipeline = pipeline.lock().unwrap();
        let position = pipeline
            .playbin
            .query_position::<gst::ClockTime>()
            .unwrap_or(gst::ClockTime::ZERO);
        (
            position,
            pipeline.playbin.property::<f64>("volume"),
            pipeline.playbin.property::<bool>("mute"),
        )
    };

    let replacement = Pipeline::build(PlaybinVersion::V3, handles)?;
    replacement.playbin.set_property("volume", volume);
    replacement.playbin.set_property("mute", mute);

    // Swap the replacement in before touching its state/URI/seek, same as
    // `ensure_playbin_version` - whatever happens below, there's only ever
    // one tracked pipeline, and the old (stuck) one is never left running
    // - unlike leaving it in `replacement` and only swapping in on success,
    // which would leak this element (and its `bus_watcher` task) on any
    // error from here on.
    let playbin = {
        let mut pipeline = pipeline.lock().unwrap();
        let old = std::mem::replace(&mut *pipeline, replacement);
        old.shutdown();
        pipeline.playbin.clone()
    };

    playbin.set_property("uri", &uri);
    playbin.set_state(gst::State::Paused).map_err(state_change_error)?;
    // `set_state` only requests the transition - block for it to actually
    // finish (pads created, caps negotiated) before seeking. Seeking any
    // earlier has no linked pads to seek through yet and fails outright,
    // which is what caused the pad-mismatch warning flood the first version
    // of this recovery path produced.
    let (result, _, _) = playbin.state(gst::ClockTime::from_seconds(5));
    result.map_err(state_change_error)?;
    playbin
        .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, position)
        .map_err(|e| MediaBackendError::GStreamer(format!("Post-recovery seek failed: {e}")))?;
    playbin.set_state(gst::State::Playing).map_err(state_change_error)?;
    Ok(())
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
            gst::MessageType::StreamCollection,
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
                    gst::MessageView::StreamCollection(msg) => {
                        let collection = msg.stream_collection();
                        info!(
                            streams = collection.len(),
                            "GStreamer reported the current track's stream collection"
                        );
                        // A fresh collection means a fresh track - apply
                        // whatever the current subtitle preference is (set
                        // to `true` by `load_media`, or toggled since) to
                        // it, rather than leaving new streams unselected.
                        let subtitles_enabled = {
                            let mut player_state = state.lock().unwrap();
                            player_state.stream_collection = Some(collection);
                            player_state.subtitles_enabled
                        };
                        apply_stream_selection(&playbin, &state, subtitles_enabled);
                    }
                    _ => {}
                }
            }
        }
    })
}

/// Sends a `select-streams` event listing every non-text stream, plus the
/// first text stream if `subtitles_enabled` - the only API `playbin3`
/// exposes for stream selection (see [`PlayerState::stream_collection`]'s
/// docs for why `playbin` v1's simpler `current-text` property doesn't
/// exist here).
fn apply_stream_selection(playbin: &gst::Element, state: &Arc<Mutex<PlayerState>>, subtitles_enabled: bool) {
    let Some(collection) = state.lock().unwrap().stream_collection.clone() else {
        return;
    };

    let mut first_text_included = false;
    let stream_ids: Vec<String> = collection
        .iter()
        .filter_map(|stream| {
            let id = stream.stream_id()?.to_string();
            if stream.stream_type().contains(gst::StreamType::TEXT) {
                if subtitles_enabled && !first_text_included {
                    first_text_included = true;
                    Some(id)
                } else {
                    None
                }
            } else {
                Some(id)
            }
        })
        .collect();

    info!(
        streams = ?stream_ids,
        subtitles_enabled,
        "Sending select-streams"
    );
    let ids: Vec<&str> = stream_ids.iter().map(String::as_str).collect();
    playbin.send_event(gst::event::SelectStreams::new(ids));
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
    INIT.get_or_init(|| {
        gst::init().map_err(|e| e.to_string())?;
        bridge_gstreamer_logging();
        cache_src::register().map_err(|e| e.to_string())?;
        Ok(())
    })
    .clone()
    .map_err(MediaBackendError::GStreamer)
}

/// Bridges GStreamer's own logging into this process's `tracing` output,
/// under the `gstreamer` target. GStreamer has a logging system entirely
/// separate from `tracing`/`RUST_LOG` - by default nothing is generated at
/// all, and once something is (via the `GST_DEBUG` env var, GStreamer's
/// own equivalent of `RUST_LOG` - e.g. `GST_DEBUG=3` for warnings+errors
/// globally, or targeted categories like
/// `GST_DEBUG=GST_STATES:5,GST_EVENT:5` to watch state changes and
/// FLUSH/seek events specifically, which is what a stuck speed-change
/// seek would show up under) it goes to its own raw stderr stream, not
/// this app's structured logs. This makes it show up in the same place as
/// everything else instead of a second stream a caller has to know to
/// look for separately - and, since `GST_DEBUG` generates nothing at all
/// when unset, sets a baseline (warnings/errors) threshold so pipeline
/// problems are visible by default without requiring it.
fn bridge_gstreamer_logging() {
    if std::env::var_os("GST_DEBUG").is_none() {
        gst::log::set_default_threshold(gst::DebugLevel::Warning);
    }

    gst::log::remove_default_log_function();
    gst::log::add_log_function(|category, level, _file, _function, _line, _object, message| {
        let text = message
            .get()
            .map(|m| m.to_string())
            .unwrap_or_default();
        let category = category.name();
        match level {
            gst::DebugLevel::Error => error!(target: "gstreamer", category, "{text}"),
            gst::DebugLevel::Warning | gst::DebugLevel::Fixme => {
                warn!(target: "gstreamer", category, "{text}")
            }
            gst::DebugLevel::Info => info!(target: "gstreamer", category, "{text}"),
            gst::DebugLevel::Debug => debug!(target: "gstreamer", category, "{text}"),
            _ => trace!(target: "gstreamer", category, "{text}"),
        }
    });
}

fn state_change_error(err: gst::StateChangeError) -> MediaBackendError {
    MediaBackendError::GStreamer(format!("State change failed: {}", err))
}

/// A unit of work for the dedicated GStreamer pipeline thread - see
/// `GStreamerBackend::gst_thread_tx`/`run_on_gst_thread`.
type GstJob = Box<dyn FnOnce() + Send + 'static>;

/// Spawns the single, dedicated OS thread every state-changing/seeking
/// GStreamer call for one backend instance runs on. Plain
/// `tokio::task::spawn_blocking` was tried first, but it hands each call to
/// whichever of many pool threads happens to be free - fine for GStreamer's
/// own general thread-safety guarantees, but not necessarily for the video
/// pipeline's GL/Metal-backed elements (`glupload`/`gldownload`, sharing a
/// GL context with the window's own wgpu renderer), which can require
/// consistent thread affinity for GL context operations. A persistent
/// dedicated thread - the same one, every call, for this backend's whole
/// lifetime - avoids that entirely. Jobs run strictly in submission order;
/// the thread exits once every `Sender` (i.e. the owning `GStreamerBackend`)
/// is dropped.
fn spawn_gst_thread() -> mpsc::Sender<GstJob> {
    let (tx, rx) = mpsc::channel::<GstJob>();
    std::thread::Builder::new()
        .name("gstreamer-pipeline".to_string())
        .spawn(move || {
            for job in rx {
                job();
            }
        })
        .expect("failed to spawn the dedicated GStreamer pipeline thread");
    tx
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
        {
            let playbin = playbin.clone();
            self.run_on_gst_thread(move || playbin.set_state(gst::State::Ready))
                .await?
                .map_err(state_change_error)?;
        }

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
            // Stale until the new track's own StreamCollection message
            // arrives; subtitles default back on for each new track, since
            // there's no UPnP-visible per-track "no subtitles" preference
            // to carry forward.
            state.stream_collection = None;
            state.subtitles_enabled = true;
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
        self.run_on_gst_thread(move || playbin.set_state(gst::State::Paused))
            .await?
            .map_err(state_change_error)?;

        Ok(())
    }

    async fn play(&self) -> Result<(), MediaBackendError> {
        // A genuine no-op when already playing - not just an optimization.
        // The AVTransport `Play` handler relies on this (see its comment:
        // "play() alone is a no-op if we're already Playing") to reissue
        // `Play` with a new `Speed` for a pure speed change without
        // disturbing playback first. Re-requesting `Playing` from
        // `Playing` re-triggers a state-change transition GStreamer may
        // still be settling from the *previous* transition, and issuing a
        // FLUSHING seek (`set_play_speed`) right on top of that has been
        // observed to hang the pipeline.
        if self.state.lock().unwrap().transport_state == TransportState::Playing {
            return Ok(());
        }

        let playbin = self.playbin();
        self.run_on_gst_thread(move || playbin.set_state(gst::State::Playing))
            .await?
            .map_err(state_change_error)?;
        self.state.lock().unwrap().transport_state = TransportState::Playing;
        Ok(())
    }

    async fn pause(&self) -> Result<(), MediaBackendError> {
        let playbin = self.playbin();
        self.run_on_gst_thread(move || playbin.set_state(gst::State::Paused))
            .await?
            .map_err(state_change_error)?;
        self.state.lock().unwrap().transport_state = TransportState::PausedPlayback;
        Ok(())
    }

    async fn stop(&self) -> Result<(), MediaBackendError> {
        let playbin = self.playbin();
        self.run_on_gst_thread(move || playbin.set_state(gst::State::Ready))
            .await?
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
        let playbin = self.playbin();
        self.run_on_gst_thread(move || playbin.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, target))
            .await?
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
        // changes are done, same as a position change. `seek()` is a
        // blocking GStreamer call (it can take a while, or in principle
        // never return, e.g. against an uncooperative network source) -
        // `run_on_gst_thread` keeps that off this async runtime's worker
        // threads (on the dedicated pipeline thread instead), and the
        // timing log below is here specifically to confirm (or rule out)
        // that this call itself is what's slow if speed changes are ever
        // reported as hanging again.
        let started = Instant::now();
        info!(speed, "Speed-change seek: starting");
        let seek_result = self.run_on_gst_thread(move || {
            let position = playbin
                .query_position::<gst::ClockTime>()
                .unwrap_or(gst::ClockTime::ZERO);
            playbin.seek(
                speed,
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::SeekType::Set,
                position,
                gst::SeekType::None,
                gst::ClockTime::NONE,
            )
        })
        .await?;
        info!(speed, elapsed_ms = %started.elapsed().as_millis(), ok = seek_result.is_ok(), "Speed-change seek: returned");

        seek_result.map_err(|e| MediaBackendError::GStreamer(format!("Speed change failed: {}", e)))?;

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
}
