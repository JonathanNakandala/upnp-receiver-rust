//! Builds the element `playbin3`'s `text-sink` property is set to.
//!
//! `text-sink` (docs: "the text output element to use (NULL = default
//! subtitleoverlay)") replaces playbin3's own default subtitle handling,
//! which inserts a `GstSubtitleOverlay` *inline in the video path*,
//! between the decoder and `video-sink` - burning subtitles into every
//! decoded video frame on the CPU before this renderer ever sees them
//! (confirmed via `GST_DEBUG` pad-caps tracing: with the default text-sink,
//! `playsink`'s internal `tbin` holds a live `GstSubtitleOverlay` sitting
//! between the decoder and the video branch; overriding `text-sink` removes
//! it from that position entirely).
//!
//! Overriding it hands this bin the *raw* decoded subtitle stream (verified
//! live: an `application/x-ass` buffer, ASS script header included) instead,
//! completely out of the video path - `video-sink` (see [`super::sink`])
//! gets the decoder's output untouched. This bin then renders that raw
//! stream itself, using the same `subtitleoverlay` element GStreamer would
//! have used anyway (so ASS/SSA styling - karaoke, positioning, color
//! overrides, fonts - renders exactly as authored, nothing reimplemented),
//! but onto a fully transparent canvas instead of the real video, so the
//! result is a small alpha-channel overlay this renderer composites on top
//! of the video texture itself in a second draw pass (see
//! [`super::renderer::Renderer`]) rather than a full-frame CPU blend.
//!
//! Subtitle track selection/on-off toggling stays exactly as today -
//! `backend.rs`'s `apply_stream_selection`/`select-streams` already decides
//! which text stream (if any) feeds playbin3's text pad at all, regardless
//! of where that pad's output goes.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use tracing::{debug, warn};

use super::overlay_frame::OverlayFrame;
use super::VideoOutput;
use crate::backend::MediaBackendError;

const SUBTITLE_APPSINK_NAME: &str = "upnp-receiver-subtitle-appsink";

/// Fixed render resolution for the transparent subtitle canvas - chosen
/// independently of the real video's resolution (which isn't known yet when
/// this bin is built, before any frame has decoded) since the overlay is
/// composited onto the video's own aspect-corrected viewport regardless of
/// the canvas's own size, the same way the video texture itself is. 1080p
/// gives ASS rendering (font hinting, outline widths) plenty of resolution
/// to look correct at. Matches most sources' 16:9 aspect ratio; a source
/// with a substantially different aspect ratio will have its subtitle
/// positioning slightly off relative to the real video - a known,
/// acceptable limitation for now rather than plumbing the real resolution
/// through before it's known.
const SUBTITLE_CANVAS_WIDTH: u32 = 1920;
const SUBTITLE_CANVAS_HEIGHT: u32 = 1080;
/// How often the transparent canvas source re-renders through
/// `subtitleoverlay`, independent of the video's own frame rate - subtitle
/// text changes at most a few times a second, so this just needs to be
/// responsive enough that a new/cleared subtitle line doesn't feel laggy,
/// not anywhere near video frame rate.
const SUBTITLE_CANVAS_FRAMERATE: u32 = 10;

/// With `output`, an appsink-terminated bin, set as playbin3's `text-sink`,
/// that renders the raw decoded subtitle stream onto a transparent canvas
/// via `subtitleoverlay` and pushes each resulting frame to the video
/// window via [`VideoOutput::send_subtitle_frame`].
///
/// Without one (no video output configured at all), this is a `fakesink`,
/// same reasoning as [`super::sink::build_video_sink`]'s fakesink case.
pub(crate) fn build_subtitle_sink(output: Option<VideoOutput>) -> Result<gst::Element, MediaBackendError> {
    let Some(output) = output else {
        return gst::ElementFactory::make("fakesink")
            .build()
            .map_err(|e| MediaBackendError::GStreamer(format!("Failed to create fakesink: {e}")));
    };

    let description = format!(
        // Deliberately *not* `is-live=true`: that was tried first (a
        // continuously-running canvas source seemed like the "live" case
        // it's meant for) but empirically causes this branch to wedge
        // after certain flushing seeks - `so.video_sink` simply stops
        // receiving buffers a moment after the seek, silently, with no
        // error, and never resumes (reproduced directly, isolated from
        // subtitle rendering entirely, by probing the pad: buffer count
        // freezes within ~1s of a seek with `is-live=true`, but climbs
        // steadily at the target framerate with it removed - same seek,
        // same file, only that property differed). The appsink's
        // `sync=true` below already paces this source to the pipeline
        // clock on its own (there's no `queue` breaking this into a
        // separate thread, so the sink's sync blocking naturally
        // back-pressures all the way to `videotestsrc`), so `is-live`
        // was both redundant and the actual cause of subtitles
        // permanently disappearing after a seek.
        // `async=false`: a non-live source has to actually produce its
        // first buffer before the *whole pipeline* can finish prerolling
        // to PAUSED, and `subtitleoverlay`/`assrender`'s first frame does
        // real work (font loading, glyph-fallback search across installed
        // fonts) - `is-live=true` used to hide that cost from the load
        // path entirely (live sources are exempt from preroll), but
        // caused the seek bug above. `async=false` gets the same
        // "don't block the pipeline's state change on this branch"
        // result without needing `is-live` at all: this appsink completes
        // its own PAUSED transition immediately rather than waiting for
        // that first (potentially slow) frame.
        "videotestsrc pattern=solid-color foreground-color=0x00000000 \
         ! video/x-raw,width={SUBTITLE_CANVAS_WIDTH},height={SUBTITLE_CANVAS_HEIGHT},format=RGBA,framerate={SUBTITLE_CANVAS_FRAMERATE}/1 \
         ! so.video_sink \
         subtitleoverlay name=so \
         ! videoconvert \
         ! appsink name={SUBTITLE_APPSINK_NAME} emit-signals=true sync=true max-buffers=1 drop=true async=false caps=video/x-raw,format=RGBA"
    );
    // `ghost_unlinked_pads=true`: `subtitleoverlay`'s `subtitle_sink` pad is
    // deliberately left unconnected in `description` above - this is what
    // exposes it as the bin's own external sink pad, which is what
    // playbin3 links its raw text stream into. Verified live (via
    // `GST_DEBUG` pad tracing) that playbin3 finds and links it correctly
    // despite the pad not being named plain "sink".
    let bin = gst::parse::bin_from_description(&description, true)
        .map_err(|e| MediaBackendError::GStreamer(format!("Failed to build subtitle sink: {e}")))?;

    let appsink = bin
        .by_name(SUBTITLE_APPSINK_NAME)
        .expect("appsink is named explicitly in the pipeline description")
        .downcast::<gst_app::AppSink>()
        .expect("named element is the appsink created by this same description");

    appsink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|e| {
                    warn!(error = %e, "Subtitle appsink pull-sample failed");
                    gst::FlowError::Error
                })?;

                match OverlayFrame::from_sample(&sample) {
                    Some(frame) => output.send_subtitle_frame(frame),
                    None => warn!(
                        "Subtitle appsink produced a sample this renderer couldn't read as an RGBA frame"
                    ),
                }

                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );

    debug!("Subtitle output enabled; text-sink renders subtitles onto a transparent overlay canvas");
    Ok(bin.upcast())
}
