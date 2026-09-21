//! The winit [`ApplicationHandler`] that owns the (lazily created) video
//! window, its renderer, and the native playback menu for the life of the
//! process.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Fullscreen, Window, WindowId};

use super::frame::VideoFrame;
use super::menu::{DispatchResult, PlaybackMenu};
use super::overlay_frame::OverlayFrame;
use super::renderer::Renderer;
use super::{MenuState, PlaybackControls, RenderParams, VideoEvent};

/// How often to drain `muda::MenuEvent::receiver()` and refresh the menu
/// via [`ControlFlow::WaitUntil`] - a menu click needs to be picked up
/// promptly even when no video frame is arriving to otherwise wake the
/// event loop (e.g. before playback has started at all).
const MENU_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// There's only ever one video window: this renderer drives a single
/// playback pipeline, so there's only ever one thing to show. The window is
/// created on the first frame (so it can be sized to the video's actual
/// resolution, per the module docs) and torn down - by the user closing it,
/// or by `VideoEvent::Hide` when the backend has nothing to show (stopped,
/// switched to audio) - without stopping playback or exiting the process;
/// the next video frame reopens it either way.
#[derive(Default)]
pub(super) struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    menu: Option<PlaybackMenu>,
    controls: Option<PlaybackControls>,
    fullscreen: bool,
    /// The latest Brightness/Contrast, regardless of whether a renderer
    /// currently exists to apply it to - unlike `MenuState`, nothing resends
    /// this periodically (see `VideoOutput::send_render_params`'s doc), so
    /// it has to be remembered here rather than just forwarded, the same
    /// way `open_window` needs *some* value to hand a brand new `Renderer`.
    render_params: RenderParams,
}

impl ApplicationHandler<VideoEvent> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // Nothing to do here - the window is opened lazily on the first
        // frame in `user_event`, once its native resolution is known.
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: VideoEvent) {
        match event {
            VideoEvent::Frame(frame) => self.handle_frame(event_loop, &frame),
            VideoEvent::SubtitleFrame(frame) => self.handle_subtitle_frame(&frame),
            VideoEvent::ClearSubtitleOverlay => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.clear_subtitle_overlay();
                }
            }
            VideoEvent::Controls(controls) => {
                info!(
                    supported_speeds = ?controls.supported_speeds,
                    "Controls event received on main thread; building video window playback menu"
                );
                self.menu = Some(PlaybackMenu::build(&controls));
                self.controls = Some(controls);
            }
            VideoEvent::State(state) => self.apply_state(&state),
            VideoEvent::Hide => self.close_window(),
            VideoEvent::RenderParams(params) => self.apply_render_params(params),
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(window) = &self.window else {
            return;
        };
        if window.id() != id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => self.close_window(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = &self.renderer {
                    renderer.render();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        while let Ok(event) = muda::MenuEvent::receiver().try_recv() {
            self.handle_menu_event(&event);
        }
        // Re-armed every tick rather than set once: `ControlFlow` doesn't
        // persist a "repeat every N ms" mode, only a next wake time.
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + MENU_POLL_INTERVAL));
    }
}

impl App {
    fn handle_frame(&mut self, event_loop: &ActiveEventLoop, frame: &VideoFrame) {
        if self.window.is_none() {
            if let Err(err) = self.open_window(event_loop, frame) {
                error!(error = %err, "Failed to open video window");
                return;
            }
        }

        let (Some(window), Some(renderer)) = (&self.window, &mut self.renderer) else {
            return;
        };

        let target_size = PhysicalSize::new(frame.width, frame.height);
        if window.inner_size() != target_size {
            let _ = window.request_inner_size(target_size);
            renderer.resize(target_size);
        }

        renderer.update_frame(frame);
        window.request_redraw();
    }

    /// Drops the subtitle frame if the window hasn't opened yet (no video
    /// frame has arrived at all) - same reasoning as a subtitle-only stream
    /// making no sense without a video track behind it.
    fn handle_subtitle_frame(&mut self, frame: &OverlayFrame) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        renderer.update_overlay_frame(frame);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn apply_state(&mut self, state: &MenuState) {
        if let Some(menu) = &self.menu {
            menu.update(state);
        }
        if let Some(renderer) = &mut self.renderer {
            renderer.set_subtitles_enabled(state.subtitles_enabled);
        }
    }

    /// Remembers `params` regardless of whether a window is currently open
    /// (see the field doc on `render_params`), and pushes it to the
    /// renderer immediately if one exists - there's no periodic resend to
    /// fall back on the way `apply_state` has, so a live window needs this
    /// applied and redrawn right here.
    fn apply_render_params(&mut self, params: RenderParams) {
        self.render_params = params;
        if let Some(renderer) = &mut self.renderer {
            renderer.set_render_params(params);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    /// Drops the window/renderer, which is what actually closes the OS
    /// window - shared by the user clicking its close button and
    /// `VideoEvent::Hide` (nothing to show right now). Either way, the next
    /// video frame reopens it via `handle_frame`.
    fn close_window(&mut self) {
        if self.window.is_some() {
            info!("Closing video window");
        }
        self.window = None;
        self.renderer = None;
    }

    fn handle_menu_event(&mut self, event: &muda::MenuEvent) {
        let (Some(menu), Some(controls)) = (&self.menu, &self.controls) else {
            return;
        };

        info!(id = %event.id().as_ref(), "Video window menu item clicked");
        if let DispatchResult::ToggleFullscreen = menu.dispatch(event, controls) {
            self.toggle_fullscreen();
        }
    }

    fn toggle_fullscreen(&mut self) {
        let Some(window) = &self.window else {
            // Nothing to fullscreen before a video window has ever opened -
            // the checkbox stays unchecked since nothing changed.
            return;
        };

        self.fullscreen = !self.fullscreen;
        window.set_fullscreen(self.fullscreen.then_some(Fullscreen::Borderless(None)));
        if let Some(menu) = &self.menu {
            menu.set_fullscreen_checked(self.fullscreen);
        }
    }

    /// Opens the window at the video's native resolution and blocks (via
    /// [`futures::executor::block_on`]) until its wgpu renderer is ready -
    /// device/adapter setup is asynchronous, but nothing else on the main
    /// thread has anything useful to do until it's done, since this
    /// renderer has no window to show before the first frame arrives.
    fn open_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        frame: &VideoFrame,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let attrs = Window::default_attributes()
            .with_title("UPnP Media Renderer")
            .with_inner_size(PhysicalSize::new(frame.width, frame.height))
            .with_resizable(false);
        let window = Arc::new(event_loop.create_window(attrs)?);
        let renderer = futures::executor::block_on(Renderer::new(
            window.clone(),
            frame,
            self.render_params,
        ))?;

        self.window = Some(window);
        self.renderer = Some(renderer);
        Ok(())
    }
}
