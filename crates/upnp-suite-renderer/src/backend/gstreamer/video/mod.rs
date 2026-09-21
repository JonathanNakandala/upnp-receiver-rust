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
//! Frames are uploaded as a plain RGBA8 texture and drawn with a triangle
//! sized to an aspect-correct viewport ([`renderer`]) - no GPU interop with
//! GStreamer's own decode output. That's deliberately the
//! simplest correct thing rather than the fastest: a per-platform zero-copy
//! path (e.g. importing VideoToolbox's decoded `CVPixelBuffer` straight into
//! a Metal texture on macOS) is a worthwhile later optimization once this
//! works end to end, not a precondition for it.

mod app;
mod capability;
mod frame;
mod menu;
mod renderer;
mod sink;

pub use frame::VideoFrame;
pub(crate) use capability::can_render;
pub(crate) use sink::build_video_sink;

use crate::server::soap::types::av_transport::TransportState;
use winit::event_loop::{EventLoop, EventLoopBuilder, EventLoopProxy};

/// Makes explicit what winit already does by default for an unbundled
/// binary (no Info.plist/bundle identifier): become the regular,
/// foreground app and activate immediately, rather than relying on that
/// bundle-identifier heuristic - see the module docs on why the app's menu
/// bar visibility should not depend on an inferred default.
#[cfg(target_os = "macos")]
fn configure_macos_activation(builder: &mut EventLoopBuilder<VideoEvent>) {
    use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
    builder
        .with_activation_policy(ActivationPolicy::Regular)
        .with_activate_ignoring_other_apps(true);
}

#[cfg(not(target_os = "macos"))]
fn configure_macos_activation(_builder: &mut EventLoopBuilder<VideoEvent>) {}

/// Delivered from GStreamer's streaming thread, or from `main`'s async side,
/// into the event loop running on the main thread.
enum VideoEvent {
    Frame(VideoFrame),
    /// Sent once, shortly after the renderer starts - see [`PlaybackControls`].
    Controls(PlaybackControls),
    /// Sent on a timer from `main`'s async side, so the menu's Play/Pause
    /// label, checked speed and subtitle checkbox stay accurate regardless
    /// of what changed them (a SOAP action, this same menu, or the backend
    /// reaching end-of-stream on its own).
    State(MenuState),
}

/// What the video window's native menu displays - not pushed reactively,
/// since transport state can change from several independent sources, but
/// polled on a timer from the single source of truth (`AVTransport::get_state`
/// and friends) by whoever calls [`VideoOutput::send_state`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuState {
    pub transport_state: TransportState,
    pub speed: f64,
    pub subtitles_enabled: bool,
}

/// The video window's native menu (play/pause, stop, speed, fullscreen,
/// subtitles) acts on these rather than on `AVTransport`/`MediaBackend`
/// directly, so this module doesn't need to depend on the services/eventing
/// layer - whoever sends this (`main`) wires each closure to the real
/// AVTransport call plus the matching GENA `LastChange` publish, the same
/// pair a SOAP action does, so a menu click is indistinguishable from a
/// control point's to any subscriber.
pub struct PlaybackControls {
    pub play_pause: Box<dyn Fn() + Send + Sync>,
    pub stop: Box<dyn Fn() + Send + Sync>,
    pub set_speed: Box<dyn Fn(f64) + Send + Sync>,
    /// Synchronous, unlike the others: the subtitle track is a local
    /// GStreamer pipeline property, not a UPnP action - no async call or
    /// `LastChange` event involved. Returns the new enabled state, so the
    /// menu checkbox can update immediately rather than waiting for the
    /// next state poll.
    pub toggle_subtitles: Box<dyn Fn() -> bool + Send + Sync>,
    /// Populates the Speed submenu, e.g. `[0.25, 0.5, 1.0, 2.0, 4.0]`.
    pub supported_speeds: Vec<f64>,
}

/// Handle the media backend feeds decoded frames into, and `main`'s async
/// side feeds playback controls and state updates into. Cloning and sending
/// are both safe from any thread - callers run on GStreamer's own streaming
/// threads or on the async runtime's, never the main thread this handle's
/// other end (the window/menu in [`app`]) runs on.
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

    /// Sent once, as soon as the AVTransport service exists - well before
    /// any video window opens, since that's lazy on the first frame (see
    /// the module docs).
    pub fn send_controls(&self, controls: PlaybackControls) {
        let _ = self.proxy.send_event(VideoEvent::Controls(controls));
    }

    pub fn send_state(&self, state: MenuState) {
        let _ = self.proxy.send_event(VideoEvent::State(state));
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
    let mut builder = EventLoop::<VideoEvent>::with_user_event();
    configure_macos_activation(&mut builder);
    let event_loop = builder
        .build()
        .expect("failed to create the video window's event loop");
    let proxy = event_loop.create_proxy();

    let run = move || {
        let mut app = app::App::default();
        event_loop.run_app(&mut app)
    };

    (VideoOutput { proxy }, run)
}
