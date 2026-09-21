//! Headless throughput benchmark for the sink -> GPU upload step (buffer
//! mapping plus `write_texture`, both now happening together in
//! `upload_pixels` - see [`super::frame`]'s doc on why there's no separate
//! CPU-side de-stride copy anymore), isolated from real decode and from the
//! display's vsync - `Renderer::render`'s `PresentMode::AutoVsync` would
//! otherwise cap any measurement at the display's refresh rate regardless
//! of how fast the upload itself is, and real GStreamer decode/`appsink`
//! pacing (`sync=true`) has nothing to do with the cost being measured
//! here. A synthetic NV12 sample, sized like a real 4K frame, isolates
//! exactly the transfer step this module's callers care about comparing
//! across upload paths.
//!
//! Before this benchmark existed, `VideoFrame::from_sample` eagerly copied
//! into an owned `Vec<u8>` on the GStreamer thread, ahead of this step.
//! Measured in isolation, that copy was several times faster than this
//! benchmark's own write-upload ceiling, so it was never the bottleneck;
//! removing it was a cleanup for the ownership story crossing the
//! GStreamer thread -> render thread hop, not a throughput fix - this
//! benchmark's number is expected to stay roughly where it was across that
//! change. Re-run it after a change here to check that expectation still
//! holds; if the number moved, something upstream of `write_texture`
//! changed too.
//!
//! Lives as an in-crate unit test (not `tests/`) because it needs
//! [`super::frame_texture`]'s private items directly - those stay
//! implementation detail, not part of this crate's public API.
//!
//! `#[ignore]`d like the rest of this crate's real-hardware GStreamer suite
//! (see `tests/gstreamer_performance_test.rs`): run explicitly with
//! `cargo test --lib -- --ignored --nocapture throughput_bench`.

use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer_video as gst_video;

use super::frame::VideoFrame;
use super::frame_texture::{upload_pixels, FrameTexture};

const BENCH_WIDTH: u32 = 3840;
const BENCH_HEIGHT: u32 = 2160;
const BENCH_DURATION: Duration = Duration::from_secs(10);

fn ensure_gst_initialized() {
    static INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    INIT.get_or_init(|| gst::init().expect("gstreamer::init should succeed"));
}

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

/// A synthetic NV12 sample the same shape a real appsink delivers - a real
/// `gst::Buffer`/`gst::Sample` (so `VideoFrame::from_sample` and its later
/// `map()`, which this benchmark exists to measure, behave exactly as they
/// do for real decoded output), just with arbitrary pixel values and no
/// actual decode involved, since only transfer cost is being measured here,
/// not correctness.
fn synthetic_frame(width: u32, height: u32) -> VideoFrame {
    ensure_gst_initialized();

    let info = gst_video::VideoInfo::builder(gst_video::VideoFormat::Nv12, width, height)
        .build()
        .expect("valid NV12 VideoInfo");
    let mut buffer =
        gst::Buffer::with_size(info.size()).expect("buffer allocation should succeed");
    {
        let buffer_mut = buffer
            .get_mut()
            .expect("freshly created buffer should be writable");
        let mut map = buffer_mut
            .map_writable()
            .expect("buffer should map writable");
        map.as_mut_slice().fill(128u8);
    }
    let sample = gst::Sample::builder()
        .buffer(&buffer)
        .caps(&info.to_caps().expect("VideoInfo should produce valid caps"))
        .build();

    VideoFrame::from_sample(&sample).expect("synthetic sample should parse as a valid frame")
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
        // throughput.
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
