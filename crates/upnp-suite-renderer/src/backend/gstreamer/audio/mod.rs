//! Audio-side RenderingControl state and the GStreamer EQ stage backing it,
//! the audio counterpart to [`super::video`]'s picture state, at a much
//! smaller scale (no window/event-loop hop: `equalizer-nbands`'s band gains
//! can be changed live from any thread, so there's no need for `video`'s
//! `VideoOutput`/`VideoEvent` proxy machinery here).
//!
//! Deliberately its own module rather than folded into `backend.rs` or
//! `video`, so a later audio-EQ profile system (should one ever exist) has
//! an obvious, separate place to grow without touching video code - the
//! same per-domain split `video`/`gstreamer` already follow.

use gstreamer as gst;
use gstreamer::prelude::*;

use crate::backend::MediaBackendError;

/// UPnP RenderingControl's audio EQ state - currently just `Loudness`. Its
/// own struct (distinct from `video::RenderParams`, which is picture-only)
/// so this can grow independently later.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct AudioParams {
    /// UPnP's `Loudness` - a boolean, unlike every other RenderingControl
    /// field this renderer implements. See [`apply_loudness`]'s doc for what
    /// turning it on actually does.
    pub loudness: bool,
}

/// The two `equalizer-nbands` bands `build_audio_filter` sets up - index,
/// not name: `equalizer-nbands`'s bands are addressed positionally via
/// `GstChildProxy` (`band0`, `band1`, ...), it has no by-name lookup.
const BASS_BAND_INDEX: u32 = 0;
const TREBLE_BAND_INDEX: u32 = 1;

/// Center frequencies for the two compensation bands, chosen from where
/// equal-loudness contours (Fletcher-Munson) show the ear's sensitivity
/// dropping most at quiet listening levels: bass far more than treble, which
/// is why `MAX_TREBLE_BOOST_DB` below is smaller than `MAX_BASS_BOOST_DB`.
const BASS_BAND_FREQ_HZ: f64 = 100.0;
const BASS_BAND_BANDWIDTH_HZ: f64 = 80.0;
const TREBLE_BAND_FREQ_HZ: f64 = 8_000.0;
const TREBLE_BAND_BANDWIDTH_HZ: f64 = 4_000.0;

/// Boost applied at full compensation strength (quietest volume) - a real
/// shelf filter would hold this boost flat below its corner frequency, but
/// `equalizer-nbands` only offers peaking bands, so this is a peaking
/// approximation of the classic hi-fi Loudness curve, not a true shelf.
const MAX_BASS_BOOST_DB: f64 = 9.0;
const MAX_TREBLE_BOOST_DB: f64 = 4.0;

/// Linear volume (`playbin`'s own 0.0-1.0 `volume` property scale) at and
/// below which loudness compensation runs at full strength, and above which
/// it's fully faded out. Real loudness buttons compensate hardest near the
/// bottom of the volume range and taper toward flat as volume rises, because
/// the ear's own frequency response flattens out at higher listening levels;
/// a fixed boost regardless of volume is exactly what makes a naive
/// "always +6dB bass" toggle sound boomy once turned up, unlike a real one.
const TAPER_START_VOLUME: f64 = 0.0;
const TAPER_END_VOLUME: f64 = 0.5;

/// Builds the audio EQ stage plugged into `playbin`'s `audio-filter`
/// property (see `Pipeline::build` in `backend.rs`, the same insertion point
/// already used for `text-sink`) - a small `audioconvert ! equalizer-nbands
/// ! audioconvert` bin, bands initialized flat (0dB) until [`apply_loudness`]
/// is called.
///
/// Returns both the bin (to hand to `audio-filter`) and the equalizer
/// element inside it (for [`apply_loudness`] to tweak live) - the bin itself
/// is never touched again once handed to `playbin`.
pub(super) fn build_audio_filter() -> Result<(gst::Element, gst::Element), MediaBackendError> {
    let description = format!(
        "audioconvert ! equalizer-nbands name=loudness-eq num-bands=2 \
         band0::freq={bass_freq} band0::bandwidth={bass_bw} band0::gain=0.0 \
         band1::freq={treble_freq} band1::bandwidth={treble_bw} band1::gain=0.0 \
         ! audioconvert",
        bass_freq = BASS_BAND_FREQ_HZ,
        bass_bw = BASS_BAND_BANDWIDTH_HZ,
        treble_freq = TREBLE_BAND_FREQ_HZ,
        treble_bw = TREBLE_BAND_BANDWIDTH_HZ,
    );
    let bin = gst::parse::bin_from_description(&description, true)
        .map_err(|e| MediaBackendError::GStreamer(format!("Failed to build audio EQ filter: {e}")))?;
    let equalizer = bin
        .by_name("loudness-eq")
        .expect("bin was just built from a description naming this element \"loudness-eq\"");

    Ok((bin.upcast(), equalizer))
}

/// Recomputes and applies both compensation bands' gain from `params` and
/// the current linear `volume` (`playbin`'s own 0.0-1.0 scale) - called from
/// both `SetLoudness` and `SetVolume`, since either can change the target
/// curve. Live: `equalizer-nbands`'s band gains take effect immediately,
/// with no pipeline state change or rebuild needed, unlike video's
/// `RenderParams` which needs a redraw.
pub(super) fn apply_loudness(equalizer: &gst::Element, params: AudioParams, volume: f64) {
    let strength = if params.loudness {
        let volume = volume.clamp(0.0, 1.0);
        ((TAPER_END_VOLUME - volume) / (TAPER_END_VOLUME - TAPER_START_VOLUME)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    set_band_gain(equalizer, BASS_BAND_INDEX, MAX_BASS_BOOST_DB * strength);
    set_band_gain(equalizer, TREBLE_BAND_INDEX, MAX_TREBLE_BOOST_DB * strength);
}

fn set_band_gain(equalizer: &gst::Element, band_index: u32, gain_db: f64) {
    let Some(child_proxy) = equalizer.dynamic_cast_ref::<gst::ChildProxy>() else {
        // Can't happen with a real `equalizer-nbands` element (it implements
        // `GstChildProxy`) - only reachable if this is ever called with a
        // different element, which would be a bug at the call site, not a
        // runtime condition worth propagating as an error.
        debug_assert!(false, "equalizer element does not implement GstChildProxy");
        return;
    };
    let Some(band) = child_proxy.child_by_index(band_index) else {
        debug_assert!(false, "equalizer-nbands has no band at index {band_index}");
        return;
    };
    band.set_property("gain", gain_db);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init() {
        let _ = gst::init();
    }

    #[test]
    fn build_audio_filter_succeeds() {
        init();
        let (bin, equalizer) = build_audio_filter().expect("audio filter should build");
        assert_eq!(bin.type_().name(), "GstBin");
        assert_eq!(equalizer.name(), "loudness-eq");
    }

    #[test]
    fn apply_loudness_off_zeroes_gain() {
        init();
        let (_bin, equalizer) = build_audio_filter().unwrap();
        apply_loudness(&equalizer, AudioParams { loudness: true }, 0.0);
        apply_loudness(&equalizer, AudioParams { loudness: false }, 0.0);

        let child_proxy = equalizer.dynamic_cast_ref::<gst::ChildProxy>().unwrap();
        let bass: f64 = child_proxy.child_by_index(BASS_BAND_INDEX).unwrap().property("gain");
        assert_eq!(bass, 0.0);
    }

    #[test]
    fn apply_loudness_on_at_low_volume_boosts_bass() {
        init();
        let (_bin, equalizer) = build_audio_filter().unwrap();
        apply_loudness(&equalizer, AudioParams { loudness: true }, 0.0);

        let child_proxy = equalizer.dynamic_cast_ref::<gst::ChildProxy>().unwrap();
        let bass: f64 = child_proxy.child_by_index(BASS_BAND_INDEX).unwrap().property("gain");
        assert_eq!(bass, MAX_BASS_BOOST_DB);
    }

    #[test]
    fn apply_loudness_on_at_high_volume_tapers_to_zero() {
        init();
        let (_bin, equalizer) = build_audio_filter().unwrap();
        apply_loudness(&equalizer, AudioParams { loudness: true }, 1.0);

        let child_proxy = equalizer.dynamic_cast_ref::<gst::ChildProxy>().unwrap();
        let bass: f64 = child_proxy.child_by_index(BASS_BAND_INDEX).unwrap().property("gain");
        assert_eq!(bass, 0.0);
    }

    #[test]
    fn apply_loudness_tapers_linearly_between_volume_bounds() {
        init();
        let (_bin, equalizer) = build_audio_filter().unwrap();
        let midpoint_volume = (TAPER_START_VOLUME + TAPER_END_VOLUME) / 2.0;
        apply_loudness(&equalizer, AudioParams { loudness: true }, midpoint_volume);

        let child_proxy = equalizer.dynamic_cast_ref::<gst::ChildProxy>().unwrap();
        let bass: f64 = child_proxy.child_by_index(BASS_BAND_INDEX).unwrap().property("gain");
        assert!((bass - MAX_BASS_BOOST_DB / 2.0).abs() < 1e-9);
    }
}
