//! Extracting a subtitle overlay frame from the subtitle appsink's sample -
//! see [`super::subtitle_sink`] for where these come from.

use gstreamer as gst;
use gstreamer_video as gst_video;
use gstreamer_video::prelude::*;

/// One rendered subtitle frame: a tightly-packed RGBA8 canvas, transparent
/// everywhere except wherever [`super::subtitle_sink`]'s `subtitleoverlay`
/// drew text - composited on top of the video's own texture as a second,
/// alpha-blended draw pass rather than baked into the video's own pixels.
/// Always plain RGBA (never NV12/P010 like [`super::frame::VideoFrame`]):
/// it's a synthetic canvas this renderer itself asked for at a fixed size,
/// not a real decoder's native output.
#[derive(Clone)]
pub struct OverlayFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl OverlayFrame {
    /// Extracts and de-strides a frame from the subtitle appsink's
    /// `pull-sample` result. `None` covers the sample/caps/mapping failing,
    /// which the caller just drops - one bad subtitle frame isn't worth
    /// interrupting playback over, same as a bad video frame.
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
