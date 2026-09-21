//! Draws the latest decoded frame into a window's wgpu surface.
//!
//! There's no scene graph: the video is drawn over a single triangle sized
//! to a viewport that preserves the frame's aspect ratio within the surface
//! (letterboxed/pillarboxed with black bars otherwise), and the window
//! itself is kept sized to the video's native resolution (see
//! [`super::app`]) so ordinarily that viewport fills the whole surface -
//! aspect correction only becomes visible once the surface's aspect ratio
//! diverges from the video's, e.g. after toggling fullscreen.

use std::sync::Arc;
use tracing::{debug, info, warn};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use super::frame::{PixelFormat, VideoFrame};

pub(super) struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    texture: FrameTexture,
    /// Whether this adapter supports the 16-bit-normalized texture formats
    /// `PixelFormat::P010` needs (`wgpu::Features::TEXTURE_FORMAT_16BIT_NORM`).
    supports_10bit: bool,
}

/// The GPU-side copy of the current frame's two planes (luma + interleaved
/// chroma - see [`VideoFrame`]), plus the bind group that lets the shader
/// sample them and do YUV -> RGB itself. Recreated wholesale whenever the
/// frame's resolution or [`PixelFormat`] changes (a new track with a
/// different video size, or switching between an 8-bit and a 10-bit
/// source); resized only through [`Renderer::update_frame`]'s ordinary
/// same-size-and-format path otherwise.
struct FrameTexture {
    y_texture: wgpu::Texture,
    uv_texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    size: PhysicalSize<u32>,
    format: PixelFormat,
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
    fn new(
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

fn upload_pixels(queue: &wgpu::Queue, texture: &FrameTexture, frame: &VideoFrame) {
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

/// Largest rect of `video_aspect` centered within a `surface_width` x
/// `surface_height` surface, as `(x, y, width, height)` - letterboxed
/// (bars top/bottom) if the surface is relatively taller than the video,
/// pillarboxed (bars left/right) if relatively wider.
fn letterboxed_viewport(
    video_width: u32,
    video_height: u32,
    surface_width: u32,
    surface_height: u32,
) -> (f32, f32, f32, f32) {
    let video_aspect = video_width as f32 / video_height as f32;
    let surface_aspect = surface_width as f32 / surface_height as f32;

    let (width, height) = if video_aspect > surface_aspect {
        (surface_width as f32, surface_width as f32 / video_aspect)
    } else {
        (surface_height as f32 * video_aspect, surface_height as f32)
    };

    let x = (surface_width as f32 - width) / 2.0;
    let y = (surface_height as f32 - height) / 2.0;
    (x, y, width, height)
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

        // P010 (10-bit HDR/wide-gamut sources) needs 16-bit-normalized
        // texture formats (R16Unorm/Rg16Unorm), which wgpu gates behind this
        // native-only feature - request it when the adapter has it so the
        // first P010 frame doesn't fail texture creation; on adapters
        // without it (e.g. some WebGL-class backends), a P010 frame is
        // logged and dropped in `update_frame` rather than uploaded.
        let supports_10bit = adapter
            .features()
            .contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: if supports_10bit {
                    wgpu::Features::TEXTURE_FORMAT_16BIT_NORM
                } else {
                    wgpu::Features::empty()
                },
                ..Default::default()
            })
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
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
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

        if frame.format == PixelFormat::P010 && !supports_10bit {
            // Vanishingly unlikely on any adapter this app actually targets
            // (Metal/Vulkan/D3D12 all support 16-bit-normalized textures),
            // but this would otherwise be a wgpu validation error on the
            // very first texture, so it's worth a clear log line.
            warn!(
                adapter = %adapter_info.name,
                "First video frame is 10-bit but this adapter lacks 16-bit-normalized texture \
                 support; texture creation below will fail"
            );
        }
        let texture = FrameTexture::new(
            &device,
            &bind_group_layout,
            &sampler,
            frame.width,
            frame.height,
            frame.format,
        );
        upload_pixels(&queue, &texture, frame);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            bind_group_layout,
            sampler,
            texture,
            supports_10bit,
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
    /// resolution or [`PixelFormat`] changed since the last frame (e.g. a
    /// new track with a different video size, or an SDR/HDR switch).
    /// Drops the frame (logging) if it's 10-bit and this adapter can't
    /// create 16-bit-normalized textures - see the check in
    /// [`Renderer::new`].
    pub(super) fn update_frame(&mut self, frame: &VideoFrame) {
        if frame.format == PixelFormat::P010 && !self.supports_10bit {
            warn!("Dropping 10-bit video frame; this adapter lacks 16-bit-normalized texture support");
            return;
        }

        let size = PhysicalSize::new(frame.width, frame.height);
        if self.texture.size != size || self.texture.format != frame.format {
            debug!(
                old_width = self.texture.size.width,
                old_height = self.texture.size.height,
                new_width = frame.width,
                new_height = frame.height,
                old_format = ?self.texture.format,
                new_format = ?frame.format,
                "Video resolution or format changed; recreating GPU texture"
            );
            self.texture = FrameTexture::new(
                &self.device,
                &self.bind_group_layout,
                &self.sampler,
                frame.width,
                frame.height,
                frame.format,
            );
        }
        upload_pixels(&self.queue, &self.texture, frame);
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
            // Shrink the viewport to the video's aspect ratio so the frame
            // isn't stretched when the window/surface has a different one
            // (e.g. after toggling fullscreen) - the rest of the
            // already-black-cleared attachment shows through as letterbox
            // bars.
            let (x, y, width, height) = letterboxed_viewport(
                self.texture.size.width,
                self.texture.size.height,
                self.config.width,
                self.config.height,
            );
            pass.set_viewport(x, y, width, height, 0.0, 1.0);
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
