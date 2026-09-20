//! Displays decoded video frames in their own window, sized to the video's
//! native resolution - a bare rendering surface with no playback chrome.
//! Transport control stays entirely with AVTransport/RenderingControl, same
//! as audio-only playback today; this only adds a picture where audio-only
//! playback has none.
//!
//! Two things run on different threads and have to meet in the middle:
//! GStreamer decodes and hands off frames from one of its own streaming
//! threads (via the appsink built in [`sink`]), while the window and its
//! wgpu renderer have to live on the process's main thread - a winit/AppKit
//! requirement, strict on macOS. [`VideoOutput`] is the bridge: a cheaply
//! cloned handle the media backend calls from wherever GStreamer calls it,
//! which wakes the event loop [`new`] hands back to run on the main thread.
//!
//! Frames are uploaded as a plain RGBA8 texture and drawn with a fixed
//! full-viewport quad ([`renderer`]) - no scaling, no letterboxing, no GPU
//! interop with GStreamer's own decode output. That's deliberately the
//! simplest correct thing rather than the fastest: a per-platform zero-copy
//! path (e.g. importing VideoToolbox's decoded `CVPixelBuffer` straight into
//! a Metal texture on macOS) is a worthwhile later optimization once this
//! works end to end, not a precondition for it.

mod app;
mod capability;
mod frame;
mod renderer;
mod sink;

pub use frame::VideoFrame;
pub(crate) use capability::can_render;
pub(crate) use sink::build_video_sink;

use winit::event_loop::{EventLoop, EventLoopProxy};

/// Delivered from GStreamer's streaming thread into the event loop running
/// on the main thread.
enum VideoEvent {
    Frame(VideoFrame),
}

/// Handle the media backend feeds decoded frames into. Cloning and sending
/// are both safe from any thread - the appsink callback in [`sink`] runs on
/// one of GStreamer's own streaming threads, never the main thread this
/// handle's other end (the window/renderer in [`app`]) runs on.
#[derive(Clone)]
pub struct VideoOutput {
    proxy: EventLoopProxy<VideoEvent>,
}

impl VideoOutput {
    fn send_frame(&self, frame: VideoFrame) {
        // Only fails if the event loop has already shut down (process
        // exiting) - nothing to do about that from a GStreamer streaming
        // thread, so the frame is just dropped.
        let _ = self.proxy.send_event(VideoEvent::Frame(frame));
    }
}

/// Builds the video window's event loop and a handle to feed it frames.
/// Construction is split from running it (the returned closure) so `main`
/// can hand [`VideoOutput`] to the media backend before anything blocks.
///
/// # Platform requirement
/// The returned closure must be called on the process's main thread and
/// blocks until the application exits - winit only supports window/event
/// handling there on macOS. This project's `main` runs it after moving
/// everything else (the async server, SSDP advertising, ...) onto its own
/// thread, since only one of the two can own the main thread.
pub fn new() -> (
    VideoOutput,
    impl FnOnce() -> Result<(), winit::error::EventLoopError>,
) {
    let event_loop = EventLoop::<VideoEvent>::with_user_event()
        .build()
        .expect("failed to create the video window's event loop");
    let proxy = event_loop.create_proxy();

    let run = move || {
        let mut app = app::App::default();
        event_loop.run_app(&mut app)
    };

    (VideoOutput { proxy }, run)
}
