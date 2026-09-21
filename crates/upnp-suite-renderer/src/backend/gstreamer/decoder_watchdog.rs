//! Detects and recovers from a wedged hardware video decoder.
//!
//! Apple's VideoToolbox decoder (`vtdec`) has been observed to leave its
//! hardware decode session corrupted after a *backward* flushing seek: from
//! that point on every frame comes back flagged as an error
//! (`GstVideoDecoder`'s "Got frame N with an error flag", logged under the
//! `videodecoder` category) and no further frames ever reach the appsink -
//! the picture just freezes, silently, with no bus error to react to.
//! There's nothing to fix on this side of that bug, so recovery is
//! pragmatic: notice the video-sink has gone quiet while a video track is
//! supposed to be playing, and rebuild the pipeline (which recreates
//! `vtdec`'s session from scratch) at the same position - same as
//! `ensure_playbin_version` already does for an audio/video switch, just
//! forced rather than gated on the [`PlaybinVersion`] actually changing.

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{error, warn};

use super::backend::{state_change_error, stop_after_unprompted_end, BackendHandles, PlaybinVersion, Pipeline};
use crate::backend::MediaBackendError;
use crate::server::soap::types::av_transport::{TransportState, TransportStatus};

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
/// recover, in place, at the position it got stuck - see the module docs.
pub(super) async fn run_decoder_watchdog(pipeline: Arc<Mutex<Pipeline>>, handles: BackendHandles) {
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
            stop_after_unprompted_end(
                &playbin,
                &handles.state,
                &handles.state_tx,
                &handles.video_output,
                TransportStatus::ErrorOccurred,
            );
            continue;
        }
        consecutive_recoveries += 1;

        warn!(
            attempt = consecutive_recoveries,
            stalled_for = ?(DECODER_STALL_CHECK_INTERVAL * stall_threshold),
            "Video decoder appears stuck; rebuilding the pipeline to recover"
        );

        if let Err(err) = recover_stuck_video_pipeline(&pipeline, &handles).await {
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
///
/// Runs entirely inside [`tokio::task::spawn_blocking`], not directly on
/// the calling task's own thread: `playbin.set_state(Paused)` below
/// synchronously triggers `upnpcachehttpsrc`'s `start()`, which blocks on
/// its own dedicated Tokio runtime (`cache_src::imp::RUNTIME`) to probe the
/// track - and calling *any* runtime's `block_on` from a thread already
/// driving a different runtime's async tasks (this function's caller,
/// `run_decoder_watchdog`, is itself a `tokio::spawn`ed task on the
/// application's main runtime) panics with "Cannot start a runtime from
/// within a runtime". `spawn_blocking` moves this onto a plain blocking-pool
/// thread, the same fix `load_media()`'s `run_on_gst_thread` already
/// applies to every other blocking GStreamer call in this backend - this
/// one just wasn't running through that mechanism yet.
async fn recover_stuck_video_pipeline(
    pipeline: &Arc<Mutex<Pipeline>>,
    handles: &BackendHandles,
) -> Result<(), MediaBackendError> {
    let pipeline = pipeline.clone();
    let handles = handles.clone();

    tokio::task::spawn_blocking(move || {
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

        let replacement = Pipeline::build(PlaybinVersion::V3, &handles)?;
        replacement.playbin.set_property("volume", volume);
        replacement.playbin.set_property("mute", mute);

        // Swap the replacement in before touching its state/URI/seek, same
        // as `ensure_playbin_version` - whatever happens below, there's
        // only ever one tracked pipeline, and the old (stuck) one is never
        // left running - unlike leaving it in `replacement` and only
        // swapping in on success, which would leak this element (and its
        // `bus_watcher` task) on any error from here on.
        let playbin = {
            let mut pipeline = pipeline.lock().unwrap();
            let old = std::mem::replace(&mut *pipeline, replacement);
            old.shutdown();
            pipeline.playbin.clone()
        };

        playbin.set_property("uri", &uri);
        playbin.set_state(gst::State::Paused).map_err(state_change_error)?;
        // `set_state` only requests the transition - block for it to
        // actually finish (pads created, caps negotiated) before seeking.
        // Seeking any earlier has no linked pads to seek through yet and
        // fails outright, which is what caused the pad-mismatch warning
        // flood an earlier version of this recovery path produced.
        let (result, _, _) = playbin.state(gst::ClockTime::from_seconds(5));
        result.map_err(state_change_error)?;
        playbin
            .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, position)
            .map_err(|e| MediaBackendError::GStreamer(format!("Post-recovery seek failed: {e}")))?;
        playbin.set_state(gst::State::Playing).map_err(state_change_error)?;

        // Same reasoning as `GStreamerBackend::seek`'s call to this: this
        // rebuild-and-reseek is itself a seek as far as the fresh
        // `subtitleoverlay` instance is concerned, so whatever was drawn
        // from the *old* pipeline needs clearing rather than sitting
        // frozen on screen indefinitely (this path doesn't go through
        // `seek()` itself, so that call never otherwise runs here).
        if let Some(video_output) = &handles.video_output {
            video_output.clear_subtitle_overlay();
        }
        Ok(())
    })
    .await
    .unwrap_or_else(|join_err| Err(MediaBackendError::GStreamer(format!("Recovery task panicked: {join_err}"))))
}
