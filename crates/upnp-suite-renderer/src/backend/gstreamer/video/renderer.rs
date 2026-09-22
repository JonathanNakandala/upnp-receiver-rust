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
use bytemuck::{Pod, Zeroable};
use tracing::{info, warn};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use super::frame::{PixelFormat, VideoFrame};
use super::frame_pipeline::{select_upload_path, FrameUploadPath};
use super::overlay_frame::OverlayFrame;
use super::overlay_texture::{self, OverlayTexture};
use super::RenderParams;

/// `shader.wgsl`'s `RenderParams` uniform, precomputed from the UPnP-facing
/// [`RenderParams`] scales so the shader itself stays agnostic of them -
/// see the shader's doc for the three-stage composition order (gain/black
/// level, then color temperature, then contrast/brightness) and why this
/// is the very last thing computed in `fs_main`. Field order must exactly
/// match the WGSL struct - `bytemuck::bytes_of` just reinterprets these
/// bytes directly, there's no name-based binding. One trailing `f32` pads
/// 11 semantic fields (44 bytes) to 48 - a multiple of 16, matching common
/// uniform buffer alignment expectations; not strictly required for every
/// backend, but costs nothing here.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct RenderParamsUniform {
    red_gain: f32,
    green_gain: f32,
    blue_gain: f32,
    red_black_level: f32,
    green_black_level: f32,
    blue_black_level: f32,
    temp_red: f32,
    temp_green: f32,
    temp_blue: f32,
    contrast_factor: f32,
    brightness_offset: f32,
    _padding: f32,
}

/// 0-1000, 500 = neutral -> a 0.0x-2.0x multiplier. See [`RenderParams`]'s
/// doc for why Gain/BlackLevel get a wider range than Brightness/Contrast.
fn gain_factor(value: u16) -> f32 {
    value as f32 / 500.0
}

/// 0-1000, 500 = neutral -> a -0.2..+0.2 additive offset - deliberately a
/// small range: this adds directly to normalized [0, 1] RGB, so anything
/// close to the full -1..+1 span would clip immediately at either end,
/// unlike Brightness's offset which is meant to span the whole range.
fn black_level_offset(value: u16) -> f32 {
    (value as f32 - 500.0) / 500.0 * 0.2
}

/// 0-100, 50 = neutral -> 3000K-10000K, symmetric around 6500K (D65) - see
/// [`RenderParams::color_temperature`]'s doc for why 6500K is the neutral
/// point rather than an arbitrary one.
fn color_temperature_kelvin(value: u16) -> f32 {
    if value <= 50 {
        3000.0 + value as f32 * 70.0
    } else {
        6500.0 + (value as f32 - 50.0) * 70.0
    }
}

/// Tanner Helland's widely-used blackbody-radiation-to-RGB approximation
/// (valid roughly 1000K-40000K) - not photometrically exact, but
/// qualitatively correct (monotonic warm -> cool, roughly tracks real
/// blackbody chromaticity), which is what a cosmetic "color temperature"
/// slider needs. This is deliberately not presented as real display
/// calibration - see [`RenderParams::color_temperature`]'s doc.
fn kelvin_to_rgb(kelvin: f32) -> (f32, f32, f32) {
    // f64 throughout, only cast back to f32 at the end: this keeps the
    // published constants below at their real precision instead of
    // truncating them to fit f32 (which clippy would otherwise flag as
    // `excessive_precision`) - irrelevant for accuracy either way given
    // this is explicitly a cosmetic approximation, but there's no reason
    // to hand-truncate a widely-cited formula when the cost of not doing
    // so is zero (this runs once per `RenderParams` change, never
    // per-pixel).
    let temp: f64 = (kelvin as f64 / 100.0).clamp(10.0, 400.0);

    let red = if temp <= 66.0 {
        255.0
    } else {
        (329.698727446 * (temp - 60.0).powf(-0.1332047592)).clamp(0.0, 255.0)
    };

    let green = if temp <= 66.0 {
        (99.4708025861 * temp.ln() - 161.1195681661).clamp(0.0, 255.0)
    } else {
        (288.1221695283 * (temp - 60.0).powf(-0.0755148492)).clamp(0.0, 255.0)
    };

    let blue = if temp >= 66.0 {
        255.0
    } else if temp <= 19.0 {
        0.0
    } else {
        (138.5177312231 * (temp - 10.0).ln() - 305.0447927307).clamp(0.0, 255.0)
    };

    ((red / 255.0) as f32, (green / 255.0) as f32, (blue / 255.0) as f32)
}

/// D65's blackbody approximation, computed once - dividing by this (rather
/// than using [`kelvin_to_rgb`]'s raw output directly) is what guarantees
/// `color_temperature = 50` (6500K, neutral) produces exactly `(1, 1, 1)`,
/// an identity multiplier, regardless of the approximation's own small
/// imperfection at 6500K (a 6500K blackbody isn't quite the same
/// chromaticity as true CIE D65 - a known, accepted subtlety this
/// normalization sidesteps rather than needing to be exact about).
fn kelvin_to_rgb_relative_to_d65(kelvin: f32) -> (f32, f32, f32) {
    let (r, g, b) = kelvin_to_rgb(kelvin);
    let (d65_r, d65_g, d65_b) = kelvin_to_rgb(6500.0);
    (r / d65_r, g / d65_g, b / d65_b)
}

impl From<RenderParams> for RenderParamsUniform {
    fn from(params: RenderParams) -> Self {
        let (temp_red, temp_green, temp_blue) =
            kelvin_to_rgb_relative_to_d65(color_temperature_kelvin(params.color_temperature));

        Self {
            red_gain: gain_factor(params.red_gain),
            green_gain: gain_factor(params.green_gain),
            blue_gain: gain_factor(params.blue_gain),
            red_black_level: black_level_offset(params.red_black_level),
            green_black_level: black_level_offset(params.green_black_level),
            blue_black_level: black_level_offset(params.blue_black_level),
            temp_red,
            temp_green,
            temp_blue,
            // 50 (neutral) -> 1.0x. 0 -> 0x (flat gray). 100 -> 2.0x.
            contrast_factor: params.contrast as f32 / 50.0,
            // 50 (neutral) -> 0.0 offset. 0 -> -1.0, 100 -> +1.0.
            brightness_offset: (params.brightness as f32 / 100.0 - 0.5) * 2.0,
            _padding: 0.0,
        }
    }
}

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
    /// Backs `shader.wgsl`'s `@group(1)` `RenderParams` uniform - see
    /// [`Renderer::set_render_params`].
    render_params_buffer: wgpu::Buffer,
    render_params_bind_group: wgpu::BindGroup,
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
        render_params: RenderParams,
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

        let render_params_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("render-params-bind-group-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let render_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render-params-buffer"),
            size: std::mem::size_of::<RenderParamsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &render_params_buffer,
            0,
            bytemuck::bytes_of(&RenderParamsUniform::from(render_params)),
        );
        let render_params_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render-params-bind-group"),
            layout: &render_params_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: render_params_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("video-frame-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout), Some(&render_params_bind_group_layout)],
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
            render_params_buffer,
            render_params_bind_group,
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

    /// Pushes a new Brightness/Contrast value to the shader - see
    /// `shader.wgsl`'s `RenderParams` and [`RenderParamsUniform`]. Doesn't
    /// force a redraw itself; the caller (`app.rs`) does that, same as
    /// every other `update_*`/`set_*` method here.
    pub(super) fn set_render_params(&mut self, params: RenderParams) {
        self.queue.write_buffer(
            &self.render_params_buffer,
            0,
            bytemuck::bytes_of(&RenderParamsUniform::from(params)),
        );
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
            pass.set_bind_group(1, &self.render_params_bind_group, &[]);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_and_black_level_are_identity_at_neutral() {
        assert_eq!(gain_factor(500), 1.0);
        assert_eq!(black_level_offset(500), 0.0);
    }

    #[test]
    fn gain_and_black_level_span_the_expected_range() {
        assert_eq!(gain_factor(0), 0.0);
        assert_eq!(gain_factor(1000), 2.0);
        assert!((black_level_offset(0) - -0.2).abs() < 1e-6);
        assert!((black_level_offset(1000) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn color_temperature_neutral_is_6500k() {
        assert_eq!(color_temperature_kelvin(50), 6500.0);
    }

    #[test]
    fn color_temperature_spans_3000k_to_10000k() {
        assert_eq!(color_temperature_kelvin(0), 3000.0);
        assert_eq!(color_temperature_kelvin(100), 10000.0);
    }

    /// The property the whole `_relative_to_d65` normalization exists for -
    /// see its doc. Without it, `kelvin_to_rgb(6500.0)` alone wouldn't be
    /// exactly `(1, 1, 1)` (a real 6500K blackbody isn't bit-identical to
    /// CIE D65), which would mean "neutral" silently applied a tint.
    #[test]
    fn color_temperature_neutral_is_an_identity_multiplier() {
        let (r, g, b) = kelvin_to_rgb_relative_to_d65(6500.0);
        assert!((r - 1.0).abs() < 1e-6, "r = {r}");
        assert!((g - 1.0).abs() < 1e-6, "g = {g}");
        assert!((b - 1.0).abs() < 1e-6, "b = {b}");
    }

    /// Qualitative sanity check on the blackbody curve's direction, the
    /// other thing worth checking per the RenderingControl design notes:
    /// warmer (lower Kelvin) should boost red relative to blue, cooler
    /// (higher Kelvin) the opposite - matches "lower = warmer/orange,
    /// higher = cooler/blue" in the UPnP spec's own `ColorTemperature` doc.
    #[test]
    fn color_temperature_warms_and_cools_in_the_right_direction() {
        let (warm_r, _, warm_b) = kelvin_to_rgb_relative_to_d65(color_temperature_kelvin(0));
        let (cool_r, _, cool_b) = kelvin_to_rgb_relative_to_d65(color_temperature_kelvin(100));

        assert!(warm_r > warm_b, "3000K should be red-biased relative to blue");
        assert!(cool_b > cool_r, "10000K should be blue-biased relative to red");
    }

    #[test]
    fn render_params_uniform_matches_defaults_at_neutral() {
        let uniform = RenderParamsUniform::from(RenderParams::default());
        assert_eq!(uniform.red_gain, 1.0);
        assert_eq!(uniform.green_gain, 1.0);
        assert_eq!(uniform.blue_gain, 1.0);
        assert_eq!(uniform.red_black_level, 0.0);
        assert_eq!(uniform.green_black_level, 0.0);
        assert_eq!(uniform.blue_black_level, 0.0);
        assert!((uniform.temp_red - 1.0).abs() < 1e-6);
        assert!((uniform.temp_green - 1.0).abs() < 1e-6);
        assert!((uniform.temp_blue - 1.0).abs() < 1e-6);
        assert_eq!(uniform.contrast_factor, 1.0);
        assert_eq!(uniform.brightness_offset, 0.0);
    }
}
