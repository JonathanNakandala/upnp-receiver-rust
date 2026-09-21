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
use tracing::{info, warn};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use super::frame::{PixelFormat, VideoFrame};
use super::frame_pipeline::{select_upload_path, FrameUploadPath};
use super::overlay_frame::OverlayFrame;
use super::overlay_texture::{self, OverlayTexture};

pub(super) struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// The active sink -> GPU transfer implementation - see
    /// [`super::frame_pipeline`] for why this is behind a trait object
    /// rather than a bare [`super::frame_texture::FrameTexture`].
    texture: Box<dyn FrameUploadPath>,
    /// Whether this adapter supports the 16-bit-normalized texture formats
    /// `PixelFormat::P010` needs (`wgpu::Features::TEXTURE_FORMAT_16BIT_NORM`).
    supports_10bit: bool,
    overlay_pipeline: wgpu::RenderPipeline,
    overlay_bind_group_layout: wgpu::BindGroupLayout,
    /// `None` until the first subtitle frame ever arrives - there may never
    /// be one (no subtitle track, or none selected), in which case
    /// [`Renderer::render`] just never draws a second pass.
    overlay_texture: Option<OverlayTexture>,
    /// Mirrors `PlayerState::subtitles_enabled` (`backend.rs`), pushed
    /// through on every [`super::MenuState`] update - deselecting the
    /// subtitle stream (`apply_stream_selection`) only stops *new* overlay
    /// frames from arriving, it doesn't clear whatever was drawn last, so
    /// [`Renderer::render`] checks this rather than just "is there a
    /// texture" to decide whether to draw the overlay pass at all.
    subtitles_enabled: bool,
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
        let mut texture = select_upload_path();
        texture.ensure_uploaded(&device, &queue, &bind_group_layout, &sampler, frame);

        let overlay_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("subtitle-overlay-bind-group-layout"),
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

        let overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("subtitle-overlay-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("overlay_shader.wgsl").into()),
        });

        let overlay_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("subtitle-overlay-pipeline-layout"),
                bind_group_layouts: &[Some(&overlay_bind_group_layout)],
                immediate_size: 0,
            });

        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("subtitle-overlay-pipeline"),
            layout: Some(&overlay_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &overlay_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &overlay_shader,
                entry_point: Some("fs_main"),
                // Alpha-blended "over" compositing on top of whatever the
                // video draw call already wrote - the video pass itself
                // stays opaque (`None` blend state, its default), only this
                // second pass blends.
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

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
            overlay_pipeline,
            overlay_bind_group_layout,
            overlay_texture: None,
            subtitles_enabled: true,
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

        self.texture.ensure_uploaded(
            &self.device,
            &self.queue,
            &self.bind_group_layout,
            &self.sampler,
            frame,
        );
    }

    /// Uploads a freshly rendered subtitle overlay canvas, recreating the
    /// GPU texture first if its size changed (it won't, in practice - the
    /// canvas is a fixed size, see `subtitle_sink`'s module docs - but a
    /// mid-playback GStreamer version/config change is cheap to stay
    /// correct against regardless).
    pub(super) fn update_overlay_frame(&mut self, frame: &OverlayFrame) {
        let size = PhysicalSize::new(frame.width, frame.height);
        let needs_new_texture = match &self.overlay_texture {
            Some(texture) => texture.size != size,
            None => true,
        };
        if needs_new_texture {
            self.overlay_texture = Some(OverlayTexture::new(
                &self.device,
                &self.overlay_bind_group_layout,
                &self.sampler,
                frame.width,
                frame.height,
            ));
        }
        overlay_texture::upload_pixels(&self.queue, self.overlay_texture.as_ref().unwrap(), frame);
    }

    /// Mirrors `PlayerState::subtitles_enabled` - see the field doc on why
    /// [`Renderer::render`] needs this instead of just checking whether an
    /// overlay texture exists.
    pub(super) fn set_subtitles_enabled(&mut self, enabled: bool) {
        self.subtitles_enabled = enabled;
    }

    /// Drops whatever was last drawn as the subtitle overlay - called right
    /// after a seek. `subtitleoverlay`/`assrender` has been observed (empirically,
    /// not just in this app) to not correctly resync which subtitle cue is
    /// active after a flushing seek: it keeps rendering whatever was on
    /// screen before the seek, sometimes for several seconds, rather than
    /// either the correct new cue or nothing. There's no fix on this side
    /// for *that* - it's upstream `assrender` behavior - but showing nothing
    /// until the next genuinely new cue arrives is a lot less wrong than
    /// showing stale, mismatched text, so this trades a brief gap for never
    /// showing something false.
    pub(super) fn clear_subtitle_overlay(&mut self) {
        self.overlay_texture = None;
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
            pass.set_bind_group(0, self.texture.bind_group(), &[]);
            // Shrink the viewport to the video's aspect ratio so the frame
            // isn't stretched when the window/surface has a different one
            // (e.g. after toggling fullscreen) - the rest of the
            // already-black-cleared attachment shows through as letterbox
            // bars.
            let (x, y, width, height) = letterboxed_viewport(
                self.texture.size().width,
                self.texture.size().height,
                self.config.width,
                self.config.height,
            );
            pass.set_viewport(x, y, width, height, 0.0, 1.0);
            // A single triangle that covers the whole viewport (positions
            // computed in the vertex shader from `vertex_index`) - cheaper
            // than a textured quad and just as correct for a full-screen
            // blit.
            pass.draw(0..3, 0..1);

            // Second draw call, same pass: the subtitle overlay (if any),
            // alpha-blended on top of the video just drawn above, at the
            // same aspect-corrected viewport so it lines up with the video
            // regardless of window size/fullscreen - not a second,
            // separate render pass, since there's no need to pay for
            // another load/store of the color attachment just to add one
            // more draw call to it.
            if self.subtitles_enabled {
                if let Some(overlay_texture) = &self.overlay_texture {
                    pass.set_pipeline(&self.overlay_pipeline);
                    pass.set_bind_group(0, &overlay_texture.bind_group, &[]);
                    pass.draw(0..3, 0..1);
                }
            }
        }

        self.queue.submit(Some(encoder.finish()));
        self.queue.present(surface_texture);
    }
}
