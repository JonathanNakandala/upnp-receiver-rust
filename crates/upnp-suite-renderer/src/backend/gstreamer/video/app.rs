//! The winit [`ApplicationHandler`] that owns the (lazily created) video
//! window and its renderer for the life of the process.

use std::sync::Arc;
use tracing::{error, info};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use super::frame::VideoFrame;
use super::renderer::Renderer;
use super::VideoEvent;

/// There's only ever one video window: this renderer drives a single
/// playback pipeline, so there's only ever one thing to show. The window is
/// created on the first frame (so it can be sized to the video's actual
/// resolution, per the module docs) and torn down if the user closes it -
/// closing it doesn't stop playback or exit the renderer process, it just
/// stops displaying; the next frame reopens it.
#[derive(Default)]
pub(super) struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
}

impl ApplicationHandler<VideoEvent> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // Nothing to do here - the window is opened lazily on the first
        // frame in `user_event`, once its native resolution is known.
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: VideoEvent) {
        let VideoEvent::Frame(frame) = event;

        if self.window.is_none() {
            if let Err(err) = self.open_window(event_loop, &frame) {
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

        renderer.update_frame(&frame);
        window.request_redraw();
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(window) = &self.window else {
            return;
        };
        if window.id() != id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                info!("Video window closed; will reopen on the next video frame");
                self.window = None;
                self.renderer = None;
            }
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
}

impl App {
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
        let renderer = futures::executor::block_on(Renderer::new(window.clone(), frame))?;

        self.window = Some(window);
        self.renderer = Some(renderer);
        Ok(())
    }
}
