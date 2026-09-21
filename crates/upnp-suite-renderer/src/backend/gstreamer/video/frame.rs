//! Extracting a displayable frame from an appsink sample.

use gstreamer as gst;
use gstreamer_video as gst_video;
use gstreamer_video::prelude::*;

/// Pixel layout of a [`VideoFrame`]'s planes - both are 4:2:0 (one full-res
/// luma plane, one half-res-in-both-dimensions interleaved chroma plane),
/// differing only in the byte width of each sample: [`PixelFormat::Nv12`] is
/// one byte per sample (SDR, 8-bit), [`PixelFormat::P010`] is two bytes per
/// sample with 10 significant bits (HDR/wide-gamut sources) - requested
/// instead of forcing a downsample so those sources don't lose precision to
/// banding. Colorimetry (BT.601 vs 709 vs 2020) and any HDR tone mapping are
/// not handled yet; [`super::renderer::Renderer`] currently treats both as
/// BT.709.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PixelFormat {
    Nv12,
    P010,
}

impl PixelFormat {
    /// Bytes per sample in either plane - 1 for 8-bit NV12, 2 for
    /// 10-bit-in-16-bit P010.
    pub fn bytes_per_sample(self) -> u32 {
        match self {
            PixelFormat::Nv12 => 1,
            PixelFormat::P010 => 2,
        }
    }
}

/// One decoded video frame as the two planes GStreamer produced it in - no
/// RGB conversion, so [`super::renderer::Renderer`] uploads both planes
/// straight to the GPU and does YUV -> RGB in the fragment shader instead of
/// paying for a CPU-side `videoconvert` on every frame.
#[derive(Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    /// Tightly packed (no row padding), full-resolution luma plane.
    pub y_plane: Vec<u8>,
    /// Tightly packed, interleaved chroma plane at half width and half
    /// height (rounded up) of the luma plane.
    pub uv_plane: Vec<u8>,
}

impl VideoFrame {
    /// Extracts and de-strides a frame from an appsink `pull-sample` result.
    /// The appsink built in [`super::sink`] only ever negotiates NV12 or
    /// P010_10LE caps, so this should always succeed on a real sample; `None`
    /// covers the sample/caps/mapping/format failing regardless, which
    /// callers just drop rather than propagate - one bad frame isn't worth
    /// interrupting playback over.
    pub(super) fn from_sample(sample: &gst::Sample) -> Option<Self> {
        let buffer = sample.buffer()?;
        let caps = sample.caps()?;
        let info = gst_video::VideoInfo::from_caps(caps).ok()?;
        let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info).ok()?;

        let format = match info.format() {
            gst_video::VideoFormat::Nv12 => PixelFormat::Nv12,
            gst_video::VideoFormat::P01010le => PixelFormat::P010,
            _ => return None,
        };

        let width = frame.width();
        let height = frame.height();
        let bytes_per_sample = format.bytes_per_sample();
        // Chroma plane row byte-width equals the luma plane's: half as many
        // sample pairs, each pair carrying both U and V, at the same byte
        // width per sample.
        let row_bytes = (width * bytes_per_sample) as usize;
        let chroma_height = height.div_ceil(2) as usize;

        let y_plane = destride_plane(&frame, 0, row_bytes, height as usize)?;
        let uv_plane = destride_plane(&frame, 1, row_bytes, chroma_height)?;

        Some(Self {
            width,
            height,
            format,
            y_plane,
            uv_plane,
        })
    }
}

/// Copies one plane out of `frame`, dropping any row-alignment padding
/// GStreamer's stride can carry beyond `row_bytes` - the wgpu upload assumes
/// a tightly-packed buffer.
fn destride_plane(
    frame: &gst_video::VideoFrameRef<&gst::BufferRef>,
    plane: u32,
    row_bytes: usize,
    rows: usize,
) -> Option<Vec<u8>> {
    let stride = frame.plane_stride().get(plane as usize).copied()? as usize;
    let data = frame.plane_data(plane).ok()?;

    if stride == row_bytes {
        Some(data.to_vec())
    } else {
        let mut out = Vec::with_capacity(row_bytes * rows);
        for row in 0..rows {
            let start = row * stride;
            out.extend_from_slice(&data[start..start + row_bytes]);
        }
        Some(out)
    }
}
