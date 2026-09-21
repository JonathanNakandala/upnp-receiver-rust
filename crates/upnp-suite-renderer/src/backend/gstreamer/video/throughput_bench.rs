//! Headless throughput benchmark for the sink -> GPU upload step, isolated
//! from real decode and from the display's vsync - `Renderer::render`'s
//! `PresentMode::AutoVsync` would otherwise cap any measurement at the
//! display's refresh rate regardless of how fast the upload itself is, and
//! real GStreamer decode/`appsink` pacing (`sync=true`) has nothing to do
//! with the cost being measured here. A synthetic in-memory NV12 buffer,
//! sized like a real 4K frame, isolates exactly the transfer step this
//! module's callers care about comparing across upload paths.
//!
//! Lives as an in-crate unit test (not `tests/`) because it needs
//! [`super::frame_texture`]'s private items directly - those stay
//! implementation detail, not part of this crate's public API.
//!
//! `#[ignore]`d like the rest of this crate's real-hardware GStreamer suite
//! (see `tests/gstreamer_performance_test.rs`): run explicitly with
//! `cargo test --lib -- --ignored --nocapture throughput_bench`.

use std::time::{Duration, Instant};

use super::frame::{PixelFormat, VideoFrame};
use super::frame_texture::{upload_pixels, FrameTexture};

const BENCH_WIDTH: u32 = 3840;
const BENCH_HEIGHT: u32 = 2160;
const BENCH_DURATION: Duration = Duration::from_secs(10);

async fn headless_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        })
        .await
        .expect("a GPU adapter should be available on this machine");
    adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .expect("device request should succeed for the default adapter")
}

/// A synthetic NV12 frame the same shape [`super::frame`] produces from a
/// real decoded sample - arbitrary pixel values, since only transfer cost is
/// being measured here, not correctness.
fn synthetic_frame(width: u32, height: u32) -> VideoFrame {
    let y_len = (width * height) as usize;
    let uv_len = (width.div_ceil(2) * height.div_ceil(2) * 2) as usize;
    VideoFrame {
        width,
        height,
        format: PixelFormat::Nv12,
        y_plane: vec![128u8; y_len],
        uv_plane: vec![128u8; uv_len],
    }
}

/// Matches [`super::renderer::Renderer`]'s real bind group layout (two
/// sampled textures + a sampler) closely enough for `FrameTexture::new` to
/// build a real bind group - the bench never draws anything, so the exact
/// shader-visibility details don't matter, only that the call succeeds.
fn bench_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("throughput-bench-bind-group-layout"),
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
    })
}

#[test]
#[ignore = "opens a real GPU device on this machine; run with `cargo test --lib -- --ignored --nocapture throughput_bench`"]
fn cpu_copy_upload_path_throughput_over_ten_seconds() {
    let (device, queue) = futures::executor::block_on(headless_device());
    let frame = synthetic_frame(BENCH_WIDTH, BENCH_HEIGHT);

    let bind_group_layout = bench_bind_group_layout(&device);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
    let texture = FrameTexture::new(
        &device,
        &bind_group_layout,
        &sampler,
        frame.width,
        frame.height,
        frame.format,
    );

    let mut frames = 0u64;
    let start = Instant::now();
    while start.elapsed() < BENCH_DURATION {
        upload_pixels(&queue, &texture, &frame);
        // Block until the GPU-side copy actually completes - without this,
        // `write_texture` returning doesn't mean the work finished, so the
        // count would measure CPU call overhead instead of real transfer
        // throughput, which stops being a fair number once a zero-copy path
        // (no staging copy at all) is benchmarked the same way.
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("device poll should succeed");
        frames += 1;
    }
    let elapsed = start.elapsed();
    let fps = frames as f64 / elapsed.as_secs_f64();
    println!(
        "CpuCopyUploadPath (current path): {frames} frames in {elapsed:?} ({fps:.1} fps) \
         at {BENCH_WIDTH}x{BENCH_HEIGHT} NV12"
    );
}
