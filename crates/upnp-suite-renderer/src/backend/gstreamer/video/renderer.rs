//! Draws the latest decoded frame into a window's wgpu surface.
//!
//! There's no scene graph and no scaling/letterboxing: the video is stretched
//! over a single full-viewport triangle, and the window itself is kept sized
//! to the video's native resolution (see [`super::app`]) so "stretch to fill"
//! and "display at native size" mean the same thing in practice.

use std::sync::Arc;
use tracing::{debug, info, warn};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use super::frame::VideoFrame;

pub(super) struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    texture: FrameTexture,
}

/// The GPU-side copy of the current frame, plus the bind group that lets the
/// shader sample it. Recreated wholesale whenever the frame's resolution
/// changes (a new track with a different video size); resized only through
/// [`Renderer::update_frame`]'s ordinary same-size path otherwise.
struct FrameTexture {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    size: PhysicalSize<u32>,
}

impl FrameTexture {
    fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        width: u32,
        height: u32,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("video-frame-texture"),
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
            label: Some("video-frame-bind-group"),
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

fn upload_pixels(queue: &wgpu::Queue, texture: &wgpu::Texture, frame: &VideoFrame) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
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

impl Renderer {
    pub(super) async fn new(
        window: Arc<Window>,
        frame: &VideoFrame,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await?;

        let adapter_info = adapter.get_info();
        info!(
            adapter = %adapter_info.name,
            backend = ?adapter_info.backend,
            width = frame.width,
            height = frame.height,
            "Opening video window"
        );

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await?;

        let caps = surface.get_capabilities(&adapter);
        // Plain (non-sRGB) format: the frame's bytes are uploaded and
        // displayed as-is, with no implicit gamma curve applied on sample or
        // on store - simplest match for "show the decoded pixels".
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("video-frame-bind-group-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("video-frame-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("video-frame-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("video-frame-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let texture = FrameTexture::new(&device, &bind_group_layout, &sampler, frame.width, frame.height);
        upload_pixels(&queue, &texture.texture, frame);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            bind_group_layout,
            sampler,
            texture,
        })
    }

    /// Reconfigures the surface for a new window size. A no-op on a
    /// momentarily-zero size (e.g. minimized), since wgpu rejects that.
    pub(super) fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    /// Uploads `frame`'s pixels, recreating the GPU texture first if its
    /// resolution changed since the last frame (e.g. a new track with a
    /// different video size).
    pub(super) fn update_frame(&mut self, frame: &VideoFrame) {
        let size = PhysicalSize::new(frame.width, frame.height);
        if self.texture.size != size {
            debug!(
                old_width = self.texture.size.width,
                old_height = self.texture.size.height,
                new_width = frame.width,
                new_height = frame.height,
                "Video resolution changed; recreating GPU texture"
            );
            self.texture = FrameTexture::new(
                &self.device,
                &self.bind_group_layout,
                &self.sampler,
                frame.width,
                frame.height,
            );
        }
        upload_pixels(&self.queue, &self.texture.texture, frame);
    }

    /// Draws the latest uploaded frame and presents it. Logs and returns on
    /// anything other than a normal frame - there's always another frame
    /// along shortly to retry with, so nothing here is worth propagating as
    /// a hard error.
    pub(super) fn render(&self) {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                // The window was resized (or similar) since the surface was
                // last configured - reconfigure with the size `resize`
                // already recorded and pick this frame up next time.
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Validation => {
                warn!("Video surface lost or invalid; will retry on the next frame");
                return;
            }
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("video-frame-encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("video-frame-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.texture.bind_group, &[]);
            // A single triangle that covers the whole viewport (positions
            // computed in the vertex shader from `vertex_index`) - cheaper
            // than a textured quad and just as correct for a full-screen
            // blit.
            pass.draw(0..3, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        self.queue.present(surface_texture);
    }
}
