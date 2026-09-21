//! Builds the element `playbin`'s `video-sink` property is set to.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use tracing::{debug, info, warn};

use super::frame::VideoFrame;
use super::VideoOutput;
use crate::backend::MediaBackendError;

const APPSINK_NAME: &str = "upnp-receiver-video-appsink";

/// `glupload ! glcolorconvert ! gldownload` normalizes *any* decoder output,
/// including a hardware decoder's GL/IOSurface-backed frames, down to plain
/// system-memory raw video in one of the two formats the appsink caps below
/// allow. A bare `videoconvert` can't read GL-memory caps at all, so without
/// this a hardware-decoded stream that only ever produces GL memory fails to
/// negotiate and the pipeline errors out, while a software-decoded one
/// (already in system memory) would have worked fine - the inconsistency
/// this is here to close.
///
/// Not used on macOS: `vtdec` (VideoToolbox) turns out not to need this at
/// all, see [`PLAIN_PIPELINE`]'s doc. Whether other platforms' hardware
/// decoders (VAAPI on Linux, D3D11VA on Windows) have an equivalent
/// plain-memory output mode is unverified - no hardware to check it against
/// - so this stays the conservative default everywhere else.
#[cfg(not(target_os = "macos"))]
const GL_AWARE_PIPELINE: &str = "glupload ! glcolorconvert ! gldownload ! videoconvert ! appsink";
/// The whole pipeline, no GL elements at all - `vtdec` on macOS negotiates
/// hardware-decoded NV12/P010 straight into system memory through this exact
/// bin, no `glupload`/`glcolorconvert`/`gldownload` round-trip needed.
///
/// Verified empirically, not assumed: `gst-launch-1.0` with
/// `GST_DEBUG=vtdec:5`, `playbin` and this crate's real appsink bin string
/// against a real H.264 file logs
/// `vtdec ... negotiated output format video/x-raw, format=NV12 ...`
/// immediately followed by `vtdec ... using hardware decoder` - hardware
/// decode confirmed, zero GL elements involved. `vtdec`'s own pad template
/// (`gst-inspect-1.0 vtdec`) backs this up: its src pad advertises a second,
/// non-GL `video/x-raw` caps set including NV12 and P010_10LE directly.
///
/// The GL round-trip this replaces on macOS (see [`GL_AWARE_PIPELINE`]) was
/// only ever load-bearing here because this appsink used to request RGBA,
/// which isn't in `vtdec`'s non-GL caps set (only NV12/P010/AYUV64/etc. are),
/// so GL genuinely was required to let `glcolorconvert` produce it. Once the
/// appsink caps moved to NV12/P010 instead (see this module's format doc),
/// that requirement quietly stopped applying, but the pipeline was never
/// revisited to match - this is that fix.
///
/// Doubles as the fallback used if this machine's GStreamer has no OpenGL
/// plugin at all (some minimal/headless installs don't) on every other
/// platform - software-decoded video works fine here too, just not
/// GL-memory-backed hardware decode output from a decoder that has no
/// plain-memory mode of its own.
const PLAIN_PIPELINE: &str = "videoconvert ! appsink";

/// NV12 (8-bit) or P010_10LE (10-bit) - both are the raw 4:2:0 format most
/// decoders (hardware or software) already produce, so requesting either
/// instead of RGBA avoids forcing a CPU-side colorspace conversion on every
/// frame: [`super::renderer::Renderer`] does the YUV -> RGB conversion in
/// the fragment shader instead. Offering both lets negotiation pick
/// whichever matches the source's actual bit depth - an 8-bit source
/// shouldn't pay for 10-bit's larger planes, and a 10-bit/HDR source
/// shouldn't be truncated down to 8-bit and band.
fn appsink_pipeline_description(pipeline: &str) -> String {
    format!(
        "{pipeline} name={APPSINK_NAME} emit-signals=true sync=true max-buffers=1 drop=true caps=video/x-raw,format={{NV12,P010_10LE}}"
    )
}

/// With `output`, an appsink-terminated bin that pushes every decoded frame
/// (as NV12 or P010_10LE, whichever negotiation picked) to the video window
/// via [`VideoOutput::send_frame`]. `max-buffers=1, drop=true` means the appsink
/// only ever holds the latest frame - this is a live display, not a queue,
/// so a slow renderer should skip frames rather than build a backlog.
///
/// Without one, it's a `fakesink`: video still decodes (so codec support is
/// real, not just detected) but nothing is ever displayed - what a backend
/// built with [`GStreamerBackend::new`](super::super::backend::GStreamerBackend::new)
/// gets, e.g. the unit tests in `backend.rs`, none of which touch a display.
///
/// `frame_counter` is bumped on every frame actually delivered to the window -
/// the decoder-stall watchdog (`backend.rs`) compares it against a previous
/// reading to tell a genuinely stuck decoder from one that's merely paused.
pub(crate) fn build_video_sink(
    output: Option<VideoOutput>,
    frame_counter: Arc<AtomicU64>,
) -> Result<gst::Element, MediaBackendError> {
    let Some(output) = output else {
        debug!("No video output configured; video-sink is fakesink (video decodes but is never displayed)");
        return gst::ElementFactory::make("fakesink")
            .build()
            .map_err(|e| MediaBackendError::GStreamer(format!("Failed to create fakesink: {e}")));
    };

    let (bin, gl_accelerated) = build_appsink_bin()?;
    info!(
        gl_accelerated,
        "Video output enabled; video-sink feeds decoded frames to the video window"
    );

    let appsink = bin
        .by_name(APPSINK_NAME)
        .expect("appsink is named explicitly in the pipeline description")
        .downcast::<gst_app::AppSink>()
        .expect("named element is the appsink created by this same description");

    appsink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|e| {
                    warn!(error = %e, "Video appsink pull-sample failed");
                    gst::FlowError::Error
                })?;

                match VideoFrame::from_sample(&sample) {
                    Some(frame) => {
                        frame_counter.fetch_add(1, Ordering::Relaxed);
                        output.send_frame(frame);
                    }
                    None => warn!(
                        "Video appsink produced a sample this renderer couldn't read as an NV12/P010 frame"
                    ),
                }

                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );

    Ok(bin.upcast())
}

/// Always [`PLAIN_PIPELINE`] - see its doc for why `vtdec` doesn't need the
/// GL round-trip [`GL_AWARE_PIPELINE`] exists for on every other platform.
#[cfg(target_os = "macos")]
fn build_appsink_bin() -> Result<(gst::Bin, bool), MediaBackendError> {
    let description = appsink_pipeline_description(PLAIN_PIPELINE);
    let bin = gst::parse::bin_from_description(&description, true)
        .map_err(|e| MediaBackendError::GStreamer(format!("Failed to build video sink: {e}")))?;
    log_pad_caps_negotiation(&bin);
    Ok((bin, false))
}

/// Tries [`GL_AWARE_PIPELINE`] first, falling back to [`PLAIN_PIPELINE`] if
/// this GStreamer install has no OpenGL plugin - `gst::parse::bin_from_description`
/// fails at construction time (unresolvable element names), not at runtime,
/// so this fallback never masks an actual playback-time negotiation error.
#[cfg(not(target_os = "macos"))]
fn build_appsink_bin() -> Result<(gst::Bin, bool), MediaBackendError> {
    let gl_description = appsink_pipeline_description(GL_AWARE_PIPELINE);
    let (bin, gl_accelerated) = match gst::parse::bin_from_description(&gl_description, true) {
        Ok(bin) => (bin, true),
        Err(e) => {
            warn!(
                error = %e,
                "OpenGL video-conversion elements unavailable; falling back to a software-only \
                 video sink - some hardware-decoded video may fail to negotiate and error out on \
                 this machine"
            );
            let plain_description = appsink_pipeline_description(PLAIN_PIPELINE);
            let bin = gst::parse::bin_from_description(&plain_description, true).map_err(|e| {
                MediaBackendError::GStreamer(format!("Failed to build video sink: {e}"))
            })?;
            (bin, false)
        }
    };

    log_pad_caps_negotiation(&bin);
    Ok((bin, gl_accelerated))
}

/// Logs every pad's negotiated caps, in this bin, whenever they change.
/// This is the difference between a "not-negotiated" pipeline error meaning
/// something (a GStreamer bus error only ever names the *source* element
/// that a downstream rejection eventually bubbled back up to - see
/// `stop_after_unprompted_end`'s caller in `backend.rs` - never the element
/// that actually rejected the caps) and being able to tell, from this
/// renderer's own logs, exactly which stage of whichever bin
/// [`build_appsink_bin`] built - [`PLAIN_PIPELINE`] or, on non-macOS,
/// [`GL_AWARE_PIPELINE`] - a particular file's decoded output failed to pass
/// through.
fn log_pad_caps_negotiation(bin: &gst::Bin) {
    for element in bin.iterate_elements().into_iter().flatten() {
        let element_name = element.name();
        for pad in element.pads() {
            let pad_name = pad.name();
            let element_name = element_name.clone();
            pad.connect_notify(Some("caps"), move |pad, _| {
                let caps = pad
                    .current_caps()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "none".to_string());
                debug!(
                    element = %element_name,
                    pad = %pad_name,
                    %caps,
                    "Video sink pad caps negotiated"
                );
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{Duration, Instant};

    fn ensure_gst_initialized() {
        static INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        INIT.get_or_init(|| gst::init().expect("gstreamer::init should succeed"));
    }

    /// Real end-to-end proof, not just the `gst-launch-1.0` check this
    /// platform split was designed against: drives an actual `playbin`
    /// through the exact bin [`build_video_sink`] wires into its
    /// `video-sink` property, over a real encoded H.264 file, and confirms
    /// [`VideoFrame::from_sample`] successfully parses real decoded samples
    /// coming out the other end. No window/winit involved -
    /// `build_appsink_bin` is pure GStreamer, so this runs on any thread.
    #[test]
    #[ignore = "spins up a real GStreamer decode pipeline; run with \
                `cargo test --lib -- --ignored --nocapture appsink_bin_delivers_real_frames`"]
    fn appsink_bin_delivers_real_frames() {
        ensure_gst_initialized();

        let path = std::env::temp_dir().join(format!(
            "upnp_receiver_sink_test_{}.mp4",
            std::process::id()
        ));
        let encode_description = format!(
            "videotestsrc num-buffers=60 pattern=ball ! \
             video/x-raw,width=640,height=360,framerate=30/1 ! videoconvert ! \
             x264enc speed-preset=ultrafast ! h264parse ! mp4mux ! filesink location={}",
            path.display()
        );
        let encoder = gst::parse::launch(&encode_description)
            .unwrap()
            .downcast::<gst::Pipeline>()
            .unwrap();
        encoder.set_state(gst::State::Playing).unwrap();
        for msg in encoder.bus().unwrap().iter_timed(gst::ClockTime::NONE) {
            match msg.view() {
                gst::MessageView::Eos(_) => break,
                gst::MessageView::Error(e) => {
                    panic!("fixture encode failed: {} ({:?})", e.error(), e.debug())
                }
                _ => {}
            }
        }
        encoder.set_state(gst::State::Null).unwrap();

        let (bin, gl_accelerated) = build_appsink_bin().expect("appsink bin should build");
        #[cfg(target_os = "macos")]
        assert!(
            !gl_accelerated,
            "macOS should always use the plain, GL-free pipeline - see PLAIN_PIPELINE's doc"
        );

        let appsink = bin
            .by_name(APPSINK_NAME)
            .unwrap()
            .downcast::<gst_app::AppSink>()
            .unwrap();

        let frames_received = Arc::new(AtomicU64::new(0));
        let counter = frames_received.clone();
        appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Error)?;
                    if VideoFrame::from_sample(&sample).is_some() {
                        counter.fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        let playbin = gst::ElementFactory::make("playbin3")
            .build()
            .or_else(|_| gst::ElementFactory::make("playbin").build())
            .expect("playbin should be available");
        playbin.set_property("uri", format!("file://{}", path.display()));
        playbin.set_property("video-sink", &bin);
        playbin.set_state(gst::State::Playing).unwrap();

        let bus = playbin.bus().unwrap();
        for msg in bus.iter_timed(gst::ClockTime::from_seconds(10)) {
            match msg.view() {
                gst::MessageView::Eos(_) => break,
                gst::MessageView::Error(e) => {
                    panic!("playback failed: {} ({:?})", e.error(), e.debug())
                }
                _ => {}
            }
        }
        playbin.set_state(gst::State::Null).unwrap();
        let _ = std::fs::remove_file(&path);

        let frames = frames_received.load(Ordering::Relaxed);
        assert!(
            frames > 0,
            "expected at least one real decoded frame to reach the appsink, got 0"
        );
        println!(
            "appsink_bin_delivers_real_frames: {frames} real frames decoded and parsed \
             (gl_accelerated={gl_accelerated})"
        );
    }

    /// Not a regression gate - prints both wall-clock numbers side by side
    /// over the same real file, the "compare, don't gate" idiom
    /// `tests/gstreamer_performance_test.rs` uses for playbin/playbin3, so
    /// the point is seeing them together rather than failing CI on either.
    /// Run with `cargo test --lib -- --ignored --nocapture compares_plain_and_gl`.
    ///
    /// [`PLAIN_PIPELINE`] is what macOS actually uses in production (see its
    /// doc for why). The GL chain here is deliberately reconstructed inline
    /// rather than reused from [`GL_AWARE_PIPELINE`] - which doesn't exist
    /// at all in a macOS build, see its `#[cfg]` - purely to put a real
    /// number on the round-trip this change removes, on this machine.
    ///
    /// `sync=false, drop=false, max-buffers=0` (unbounded) on both, unlike
    /// production's `sync=true, drop=true, max-buffers=1`: a live player
    /// should drop frames to stay paced to real time, but that would hide a
    /// throughput difference behind frame-dropping instead of surfacing it
    /// as wall-clock time - this measures both pipelines flat-out,
    /// decode-bound, delivering every single frame.
    #[test]
    #[ignore = "spins up two real GStreamer decode pipelines; run with \
                `cargo test --lib -- --ignored --nocapture compares_plain_and_gl`"]
    fn compares_plain_and_gl_pipeline_decode_throughput() {
        ensure_gst_initialized();

        // 1080p60, 5s of real content - long enough for per-frame transfer
        // cost to dominate over one-time pipeline startup/preroll cost.
        let path = std::env::temp_dir().join(format!(
            "upnp_receiver_sink_bench_{}.mp4",
            std::process::id()
        ));
        let encode_description = format!(
            "videotestsrc num-buffers=300 pattern=ball ! \
             video/x-raw,width=1920,height=1080,framerate=60/1 ! videoconvert ! \
             x264enc speed-preset=ultrafast ! h264parse ! mp4mux ! filesink location={}",
            path.display()
        );
        let encoder = gst::parse::launch(&encode_description)
            .unwrap()
            .downcast::<gst::Pipeline>()
            .unwrap();
        encoder.set_state(gst::State::Playing).unwrap();
        for msg in encoder.bus().unwrap().iter_timed(gst::ClockTime::NONE) {
            match msg.view() {
                gst::MessageView::Eos(_) => break,
                gst::MessageView::Error(e) => {
                    panic!("fixture encode failed: {} ({:?})", e.error(), e.debug())
                }
                _ => {}
            }
        }
        encoder.set_state(gst::State::Null).unwrap();

        const GL_CHAIN: &str = "glupload ! glcolorconvert ! gldownload ! videoconvert ! appsink";
        // Order controlled for a same-process warm-up bias (decoder buffer
        // pools, plugin registry, disk cache all already warm by the second
        // run regardless of which pipeline it is) - run each pipeline
        // twice, alternating first/second position, and keep only the
        // second (warm) measurement of each.
        let _ = decode_wall_clock(&path, PLAIN_PIPELINE);
        let (gl_elapsed, gl_frames) = decode_wall_clock(&path, GL_CHAIN);
        let _ = decode_wall_clock(&path, GL_CHAIN);
        let (plain_elapsed, plain_frames) = decode_wall_clock(&path, PLAIN_PIPELINE);

        println!(
            "plain (no GL):  {plain_elapsed:?} to decode {plain_frames} frames at 1920x1080"
        );
        println!("GL round-trip:  {gl_elapsed:?} to decode {gl_frames} frames at 1920x1080");

        let _ = std::fs::remove_file(&path);
    }

    /// Plays `path` through a standalone appsink bin built from `pipeline`
    /// flat-out (no real-time pacing - see
    /// [`compares_plain_and_gl_pipeline_decode_throughput`]'s doc), and
    /// returns the wall-clock time to reach EOS plus how many frames this
    /// renderer's own [`VideoFrame::from_sample`] successfully parsed.
    fn decode_wall_clock(path: &Path, pipeline: &str) -> (Duration, u64) {
        let description = format!(
            "{pipeline} name={APPSINK_NAME} emit-signals=true sync=false max-buffers=0 \
             drop=false caps=video/x-raw,format={{NV12,P010_10LE}}"
        );
        let bin = gst::parse::bin_from_description(&description, true)
            .expect("benchmark pipeline should build");
        let appsink = bin
            .by_name(APPSINK_NAME)
            .unwrap()
            .downcast::<gst_app::AppSink>()
            .unwrap();

        let frames = Arc::new(AtomicU64::new(0));
        let counter = frames.clone();
        appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Error)?;
                    if VideoFrame::from_sample(&sample).is_some() {
                        counter.fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        let playbin = gst::ElementFactory::make("playbin3")
            .build()
            .or_else(|_| gst::ElementFactory::make("playbin").build())
            .expect("playbin should be available");
        playbin.set_property("uri", format!("file://{}", path.display()));
        playbin.set_property("video-sink", &bin);

        let start = Instant::now();
        playbin.set_state(gst::State::Playing).unwrap();
        let bus = playbin.bus().unwrap();
        for msg in bus.iter_timed(gst::ClockTime::from_seconds(30)) {
            match msg.view() {
                gst::MessageView::Eos(_) => break,
                gst::MessageView::Error(e) => {
                    panic!("decode failed: {} ({:?})", e.error(), e.debug())
                }
                _ => {}
            }
        }
        let elapsed = start.elapsed();
        playbin.set_state(gst::State::Null).unwrap();

        (elapsed, frames.load(Ordering::Relaxed))
    }
}
