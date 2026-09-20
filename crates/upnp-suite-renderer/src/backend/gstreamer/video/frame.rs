//! Extracting a displayable frame from an appsink sample.

use gstreamer as gst;
use gstreamer_video as gst_video;
use gstreamer_video::prelude::*;

/// One decoded video frame, already normalized to tightly-packed RGBA8 (no
/// row padding) - the layout [`super::renderer::Renderer`] uploads straight
/// into a wgpu texture with `bytes_per_row = width * 4`.
#[derive(Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl VideoFrame {
    /// Extracts and de-strides a frame from an appsink `pull-sample` result.
    /// The appsink built in [`super::sink`] only ever negotiates plain RGBA
    /// caps, so this should always succeed on a real sample; `None` covers
    /// the sample/caps/mapping failing regardless, which callers just drop
    /// rather than propagate - one bad frame isn't worth interrupting
    /// playback over.
    pub(super) fn from_sample(sample: &gst::Sample) -> Option<Self> {
        let buffer = sample.buffer()?;
        let caps = sample.caps()?;
        let info = gst_video::VideoInfo::from_caps(caps).ok()?;
        let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info).ok()?;

        let width = frame.width();
        let height = frame.height();
        let stride = frame.plane_stride().first().copied()? as usize;
        let plane = frame.plane_data(0).ok()?;
        let row_bytes = width as usize * 4;

        // GStreamer's row stride can be wider than `width * 4` (padding for
        // alignment); the wgpu upload assumes a tightly-packed buffer, so
        // copy row by row whenever they differ instead of the whole plane.
        let data = if stride == row_bytes {
            plane.to_vec()
        } else {
            let mut data = Vec::with_capacity(row_bytes * height as usize);
            for row in 0..height as usize {
                let start = row * stride;
                data.extend_from_slice(&plane[start..start + row_bytes]);
            }
            data
        };

        Some(Self {
            width,
            height,
            data,
        })
    }
}
