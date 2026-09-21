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

/// One decoded video frame - a cheap handle (a refcounted [`gst::Sample`]
/// clone, no pixel data touched yet) rather than an owned copy of its
/// planes. [`super::frame_texture::upload_pixels`] maps the sample's buffer
/// directly at upload time and hands GStreamer's own strided plane data
/// straight to `wgpu` (which accepts an arbitrary source stride via
/// `bytes_per_row` natively), so no CPU-side de-stride copy happens at all,
/// which used to cost real CPU time for no benefit - see `throughput_bench`'s
/// module doc for the before/after numbers. [`super::renderer::Renderer`]
/// still does the YUV -> RGB conversion in the fragment shader, not here.
#[derive(Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    sample: gst::Sample,
}

impl VideoFrame {
    /// Reads a frame's format/dimensions from `sample`'s caps and keeps a
    /// cheap clone of the sample itself (a refcount bump - [`gst::Sample`]
    /// and the buffer it holds are `Send + Sync`, safe to carry across the
    /// GStreamer streaming thread -> winit main thread hop this crosses).
    /// The buffer itself is never mapped here - see [`VideoFrame::map`].
    ///
    /// The appsink built in [`super::sink`] only ever negotiates NV12 or
    /// P010_10LE caps, so this should always succeed on a real sample;
    /// `None` covers the sample/caps/format failing regardless, which
    /// callers just drop rather than propagate - one bad frame isn't worth
    /// interrupting playback over.
    pub(super) fn from_sample(sample: &gst::Sample) -> Option<Self> {
        let caps = sample.caps()?;
        let info = gst_video::VideoInfo::from_caps(caps).ok()?;

        let format = match info.format() {
            gst_video::VideoFormat::Nv12 => PixelFormat::Nv12,
            gst_video::VideoFormat::P01010le => PixelFormat::P010,
            _ => return None,
        };

        Some(Self {
            width: info.width(),
            height: info.height(),
            format,
            sample: sample.clone(),
        })
    }

    /// Maps this frame's buffer read-only, for a caller (the render thread,
    /// via `upload_pixels`) to read its planes' real bytes and strides
    /// directly - held only as long as the returned [`MappedVideoFrame`]
    /// lives, then unmapped.
    pub(super) fn map(&self) -> Option<MappedVideoFrame<'_>> {
        let buffer = self.sample.buffer()?;
        let caps = self.sample.caps()?;
        let info = gst_video::VideoInfo::from_caps(caps).ok()?;
        let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info).ok()?;
        Some(MappedVideoFrame { frame })
    }
}

/// A [`VideoFrame`]'s buffer, mapped read-only - gives direct access to
/// GStreamer's own plane bytes and strides, exactly as decoded, with no
/// intermediate copy.
pub(super) struct MappedVideoFrame<'a> {
    frame: gst_video::VideoFrameRef<&'a gst::BufferRef>,
}

impl MappedVideoFrame<'_> {
    /// `plane` 0 is luma (Y), `plane` 1 is interleaved chroma (UV) - see
    /// [`VideoFrame`]'s doc. Both `None`s only if `plane` is out of range,
    /// which never happens for the NV12/P010 frames this renderer handles.
    pub(super) fn plane(&self, plane: u32) -> Option<(&[u8], u32)> {
        let stride = self.frame.plane_stride().get(plane as usize).copied()?;
        let data = self.frame.plane_data(plane).ok()?;
        Some((data, stride as u32))
    }
}
