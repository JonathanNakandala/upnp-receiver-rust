//! The GPU-side copy of the current subtitle overlay canvas - the same
//! upload-on-change pattern as [`super::frame_texture`], but a single plain
//! RGBA8 plane (see [`OverlayFrame`]) instead of two YUV planes, and
//! recreated far less often since it only needs to match its own fixed
//! canvas size, not the video's (see `subtitle_sink`'s module docs).

use winit::dpi::PhysicalSize;

use super::overlay_frame::OverlayFrame;

pub(super) struct OverlayTexture {
    texture: wgpu::Texture,
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) size: PhysicalSize<u32>,
}

impl OverlayTexture {
    pub(super) fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        width: u32,
        height: u32,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("subtitle-overlay-texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("subtitle-overlay-bind-group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        Self {
            texture,
            bind_group,
            size: PhysicalSize::new(width, height),
        }
    }
}

pub(super) fn upload_pixels(queue: &wgpu::Queue, texture: &OverlayTexture, frame: &OverlayFrame) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &frame.data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(frame.width * 4),
            rows_per_image: Some(frame.height),
        },
        wgpu::Extent3d {
            width: frame.width,
            height: frame.height,
            depth_or_array_layers: 1,
        },
    );
}
