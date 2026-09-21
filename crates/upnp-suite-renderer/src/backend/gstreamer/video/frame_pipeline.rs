//! The sink -> GPU transfer step abstracted behind [`FrameUploadPath`], so
//! [`super::renderer::Renderer`] doesn't need to know which one is active.
//! [`CpuCopyUploadPath`] (wrapping [`super::frame_texture`] verbatim) is the
//! only implementation today, and stays the unconditional default: it's the
//! one guaranteed to work on every platform and every adapter, so
//! [`select_upload_path`] always falls back to it rather than ever failing
//! playback over a faster path not panning out.
//!
//! Two platform-native, zero-copy paths are the reason this trait exists
//! rather than just keeping [`super::frame_texture::FrameTexture`] inline in
//! `Renderer` - see this crate's `metal_upload_path` module (macOS, once
//! added) and the Vulkan note on [`select_upload_path`] below.

use tracing::debug;
use winit::dpi::PhysicalSize;

use super::frame::VideoFrame;
use super::frame_texture::{upload_pixels, FrameTexture};

/// One frame's GPU-resident planes, plus however they got there. Every
/// implementation must leave the GPU with a texture matching `frame`'s
/// current resolution and pixel format after [`FrameUploadPath::ensure_uploaded`]
/// returns - `Renderer` never inspects which concrete path is active, only
/// this contract.
pub(super) trait FrameUploadPath {
    /// Makes sure the GPU has a bindable texture matching `frame`'s
    /// resolution/format - recreating it first if either changed since the
    /// last call, or if this is the first call - then uploads `frame`'s
    /// pixels into it.
    fn ensure_uploaded(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        frame: &VideoFrame,
    );

    /// Panics if called before the first [`FrameUploadPath::ensure_uploaded`].
    fn size(&self) -> PhysicalSize<u32>;

    /// Panics if called before the first [`FrameUploadPath::ensure_uploaded`].
    fn bind_group(&self) -> &wgpu::BindGroup;
}

/// The always-available fallback: decoded planes arrive as CPU bytes (see
/// `sink.rs`/`frame.rs`) and are copied to the GPU with
/// `queue.write_texture`, unchanged from how this renderer has always
/// worked. No platform requirements, no adapter feature requirements beyond
/// what [`super::renderer::Renderer::new`] already asks for.
#[derive(Default)]
pub(super) struct CpuCopyUploadPath {
    texture: Option<FrameTexture>,
}

impl FrameUploadPath for CpuCopyUploadPath {
    fn ensure_uploaded(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        frame: &VideoFrame,
    ) {
        let size = PhysicalSize::new(frame.width, frame.height);
        let needs_new_texture = match &self.texture {
            Some(texture) => texture.size != size || texture.format != frame.format,
            None => true,
        };
        if needs_new_texture {
            if let Some(old) = &self.texture {
                debug!(
                    old_width = old.size.width,
                    old_height = old.size.height,
                    new_width = frame.width,
                    new_height = frame.height,
                    old_format = ?old.format,
                    new_format = ?frame.format,
                    "Video resolution or format changed; recreating GPU texture"
                );
            }
            self.texture = Some(FrameTexture::new(
                device,
                layout,
                sampler,
                frame.width,
                frame.height,
                frame.format,
            ));
        }
        upload_pixels(queue, self.texture.as_ref().expect("just created above if absent"), frame);
    }

    fn size(&self) -> PhysicalSize<u32> {
        self.texture
            .as_ref()
            .expect("ensure_uploaded must be called before size()")
            .size
    }

    fn bind_group(&self) -> &wgpu::BindGroup {
        &self
            .texture
            .as_ref()
            .expect("ensure_uploaded must be called before bind_group()")
            .bind_group
    }
}

/// Picks the fastest available upload path for this platform, falling back
/// to [`CpuCopyUploadPath`] on any failure - never a hard error, since a
/// slower upload path is always preferable to failing playback outright
/// (the same philosophy `Renderer::update_frame` already applies when an
/// adapter lacks 10-bit texture support).
///
/// # Platform paths
/// - macOS: a Metal path (`MTLTexture` imported straight from the
///   decoder's `IOSurface`, skipping the CPU round-trip entirely) belongs
///   here, tried before falling back to [`CpuCopyUploadPath`].
/// - Linux/Windows/Android: a Vulkan path (DMA-BUF import) is the intended
///   future addition - deliberately not implemented here, since it can't be
///   built or verified without that hardware. This function is the one
///   place it plugs in, mirroring the macOS branch, so adding it later
///   doesn't require touching [`super::renderer::Renderer`] again.
pub(super) fn select_upload_path() -> Box<dyn FrameUploadPath> {
    Box::new(CpuCopyUploadPath::default())
}
