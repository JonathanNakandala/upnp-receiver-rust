//! The GPU-side copy of the current video frame's two planes, and the
//! upload path that keeps it in sync with a freshly decoded [`VideoFrame`].
//! Split out of [`super::renderer`] so that file stays the render
//! orchestrator (device/pipeline setup, resize, the render pass) while the
//! texture bookkeeping for each thing actually drawn - video frame here,
//! and eventually a subtitle overlay - gets its own file.

use winit::dpi::PhysicalSize;

use super::frame::{PixelFormat, VideoFrame};

/// The GPU-side copy of the current frame's two planes (luma + interleaved
/// chroma - see [`VideoFrame`]), plus the bind group that lets the shader
/// sample them and do YUV -> RGB itself. Recreated wholesale whenever the
/// frame's resolution or [`PixelFormat`] changes (a new track with a
/// different video size, or switching between an 8-bit and a 10-bit
/// source); resized only through [`super::renderer::Renderer::update_frame`]'s
/// ordinary same-size-and-format path otherwise.
pub(super) struct FrameTexture {
    y_texture: wgpu::Texture,
    uv_texture: wgpu::Texture,
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) size: PhysicalSize<u32>,
    pub(super) format: PixelFormat,
}

/// The wgpu texture formats for a frame's luma and chroma planes - one byte
/// per channel for 8-bit NV12, two bytes per channel (10 significant bits)
/// for P010, so 10-bit sources keep their extra precision instead of being
/// truncated on upload.
fn plane_formats(format: PixelFormat) -> (wgpu::TextureFormat, wgpu::TextureFormat) {
    match format {
        PixelFormat::Nv12 => (wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::Rg8Unorm),
        PixelFormat::P010 => (wgpu::TextureFormat::R16Unorm, wgpu::TextureFormat::Rg16Unorm),
    }
}

impl FrameTexture {
    pub(super) fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        width: u32,
        height: u32,
        format: PixelFormat,
    ) -> Self {
        let (y_format, uv_format) = plane_formats(format);
        let uv_width = width.div_ceil(2);
        let uv_height = height.div_ceil(2);

        let make_texture = |label, tex_format, w, h| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: tex_format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };

        let y_texture = make_texture("video-frame-y-texture", y_format, width, height);
        let uv_texture = make_texture("video-frame-uv-texture", uv_format, uv_width, uv_height);
        let y_view = y_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let uv_view = uv_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("video-frame-bind-group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&y_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&uv_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        Self {
            y_texture,
            uv_texture,
            bind_group,
            size: PhysicalSize::new(width, height),
            format,
        }
    }
}

pub(super) fn upload_pixels(queue: &wgpu::Queue, texture: &FrameTexture, frame: &VideoFrame) {
    let bytes_per_sample = frame.format.bytes_per_sample();
    let uv_width = frame.width.div_ceil(2);
    let uv_height = frame.height.div_ceil(2);

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture.y_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &frame.y_plane,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(frame.width * bytes_per_sample),
            rows_per_image: Some(frame.height),
        },
        wgpu::Extent3d {
            width: frame.width,
            height: frame.height,
            depth_or_array_layers: 1,
        },
    );

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture.uv_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &frame.uv_plane,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            // Two interleaved channels, `bytes_per_sample` each.
            bytes_per_row: Some(uv_width * 2 * bytes_per_sample),
            rows_per_image: Some(uv_height),
        },
        wgpu::Extent3d {
            width: uv_width,
            height: uv_height,
            depth_or_array_layers: 1,
        },
    );
}
