//! Builds the element `playbin`'s `video-sink` property is set to.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use tracing::{debug, info, warn};

use super::frame::VideoFrame;
use super::VideoOutput;
use crate::backend::MediaBackendError;

const APPSINK_NAME: &str = "upnp-receiver-video-appsink";

/// `glupload ! glcolorconvert ! gldownload` normalizes *any* decoder output,
/// including a hardware decoder's GL/IOSurface-backed frames (what macOS's
/// VideoToolbox, `vtdec`, used for H.264/H.265, produces by default), down
/// to plain system-memory raw video before the RGBA conversion appsink
/// needs. A bare `videoconvert` can't read GL-memory caps at all, so without
/// this a hardware-decoded stream fails to negotiate and the pipeline
/// errors out, while a software-decoded one (already in system memory)
/// would have worked fine - the inconsistency this is here to close.
const GL_AWARE_PIPELINE: &str = "glupload ! glcolorconvert ! gldownload ! videoconvert ! appsink";
/// Used if this machine's GStreamer has no OpenGL plugin at all (some
/// minimal/headless installs don't) - software-decoded video still works
/// here, just not GL-memory-backed hardware decode output.
const PLAIN_PIPELINE: &str = "videoconvert ! appsink";

fn appsink_pipeline_description(pipeline: &str) -> String {
    format!(
        "{pipeline} name={APPSINK_NAME} emit-signals=true sync=true max-buffers=1 drop=true caps=video/x-raw,format=RGBA"
    )
}

/// With `output`, an appsink-terminated bin that pushes every decoded frame
/// (converted to plain RGBA8) to the video window via
/// [`VideoOutput::send_frame`]. `max-buffers=1, drop=true` means the appsink
/// only ever holds the latest frame - this is a live display, not a queue,
/// so a slow renderer should skip frames rather than build a backlog.
///
/// Without one, it's a `fakesink`: video still decodes (so codec support is
/// real, not just detected) but nothing is ever displayed - what a backend
/// built with [`GStreamerBackend::new`](super::super::backend::GStreamerBackend::new)
/// gets, e.g. the unit tests in `backend.rs`, none of which touch a display.
pub(crate) fn build_video_sink(
    output: Option<VideoOutput>,
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
                    Some(frame) => output.send_frame(frame),
                    None => warn!(
                        "Video appsink produced a sample this renderer couldn't read as an RGBA frame"
                    ),
                }

                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );

    Ok(bin.upcast())
}

/// Tries [`GL_AWARE_PIPELINE`] first, falling back to [`PLAIN_PIPELINE`] if
/// this GStreamer install has no OpenGL plugin - `gst::parse::bin_from_description`
/// fails at construction time (unresolvable element names), not at runtime,
/// so this fallback never masks an actual playback-time negotiation error.
fn build_appsink_bin() -> Result<(gst::Bin, bool), MediaBackendError> {
    let gl_description = appsink_pipeline_description(GL_AWARE_PIPELINE);
    let (bin, gl_accelerated) = match gst::parse::bin_from_description(&gl_description, true) {
        Ok(bin) => (bin, true),
        Err(e) => {
            warn!(
                error = %e,
                "OpenGL video-conversion elements unavailable; falling back to a software-only \
                 video sink - hardware-decoded video (e.g. H.264/H.265 via VideoToolbox on macOS) \
                 may fail to negotiate and error out on this machine"
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
/// renderer's own logs, exactly which stage of `glupload ! glcolorconvert !
/// gldownload ! videoconvert ! appsink` a particular file's decoded output
/// failed to pass through.
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
