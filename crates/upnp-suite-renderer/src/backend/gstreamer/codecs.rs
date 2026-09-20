//! Enumerates the audio and video formats the installed GStreamer decodes,
//! so [`ConnectionManager::GetProtocolInfo`] can advertise what this
//! renderer can actually play instead of a single hardcoded entry —
//! different machines have different plugin sets installed (e.g.
//! `gstreamer1-libav` for AAC/MP4 pulls in decoders a bare
//! `gst-plugins-base`/`-good` install doesn't have).
//!
//! GStreamer doesn't have a notion of "supported file mime type" — it has
//! elements (decoders, demuxers, parsers) with sink pad templates describing
//! the caps they accept. We walk every "decodable" element of the relevant
//! media type (`ElementFactoryType::DECODABLE`: decoders, demuxers, parsers,
//! depayloaders, decryptors) the registry knows about, collect the caps
//! structure names on their sink pads, and map the ones we recognize to the
//! mime type a UPnP/DLNA control point expects to see in `protocolInfo`.
//! That mapping is necessarily approximate: GStreamer's caps naming doesn't
//! line up 1:1 with IANA/DLNA mime types (most notably `audio/mpeg` covers
//! both MP3 and raw AAC depending on `mpegversion`/`layer`).
//!
//! Video mime types are only ever included if [`video::can_render`] says
//! this machine can actually show them (see [`video`] for why): a codec
//! being installed only means GStreamer can decode it, not that there's
//! anywhere to display the result. Still images aren't included at all yet
//! — the video pipeline has no `imagefreeze` stage, so a decoded still would
//! flash for one frame and disappear rather than stay on screen, and
//! advertising that as "supported" would be actively misleading to a
//! control point.
//!
//! [`ConnectionManager::GetProtocolInfo`]: crate::services::ConnectionManager::get_protocol_info

use gstreamer as gst;
use std::collections::BTreeSet;

use super::backend::ensure_initialized;
use super::video;

/// Maps one GStreamer caps structure (as seen on a sink pad template) to the
/// mime type a UPnP control point would recognize, or `None` if it's not a
/// format we know how to name (e.g. an internal/raw caps type that's never
/// itself a playable file, like `audio/x-raw`).
fn canonical_audio_mime_for_structure(structure: &gst::StructureRef) -> Option<&'static str> {
    match structure.name().as_str() {
        // GStreamer uses "audio/mpeg" for both MPEG-1 Layer III (MP3) and
        // raw MPEG-2/4 AAC streams, distinguished only by `mpegversion`.
        "audio/mpeg" => {
            let mpegversion: i32 = structure.get("mpegversion").unwrap_or(1);
            if mpegversion >= 2 {
                Some("audio/mp4")
            } else {
                Some("audio/mpeg")
            }
        }
        "audio/x-flac" => Some("audio/x-flac"),
        "audio/x-wav" => Some("audio/x-wav"),
        "audio/ogg" => Some("audio/ogg"),
        "audio/x-vorbis" | "audio/x-opus" | "audio/opus" => Some("audio/ogg"),
        "audio/x-m4a" | "audio/x-alac" | "video/quicktime" => Some("audio/mp4"),
        "audio/x-ac3" | "audio/ac3" => Some("audio/x-ac3"),
        _ => None,
    }
}

/// Same idea as [`canonical_audio_mime_for_structure`], for video codecs.
/// Keyed on codec-level caps only - what an actual decoder element
/// (`avdec_h264`, `vp9dec`, etc.) declares on its sink pad - since a codec
/// being decodable is the real signal that some content is playable.
/// Container mime types are handled separately, by
/// [`canonical_video_container_mime_for_structure`] - see its docs for why
/// they can't share this function's `MEDIA_VIDEO` factory filter.
fn canonical_video_codec_mime_for_structure(structure: &gst::StructureRef) -> Option<&'static str> {
    match structure.name().as_str() {
        "video/x-h264" | "video/x-h265" => Some("video/mp4"),
        // GStreamer uses "video/mpeg" for both MPEG-1/2 video and MPEG-4
        // part 2, distinguished only by `mpegversion` - same ambiguity as
        // "audio/mpeg" above.
        "video/mpeg" => {
            let mpegversion: i32 = structure.get("mpegversion").unwrap_or(2);
            if mpegversion >= 4 {
                Some("video/mp4")
            } else {
                Some("video/mpeg")
            }
        }
        "video/x-vp8" | "video/x-vp9" => Some("video/webm"),
        "video/x-theora" => Some("video/ogg"),
        _ => None,
    }
}

/// Video containers, detected purely by their demuxer's sink caps and
/// deliberately *not* filtered by `MEDIA_VIDEO` (unlike
/// [`canonical_video_codec_mime_for_structure`]'s search) - GStreamer
/// classifies most container demuxers (`matroskademux`, `qtdemux`,
/// `avidemux`, ...) as plain `Codec/Demuxer` with no Audio/Video media-type
/// tag at all, since the same demuxer handles audio-only, video-only or
/// mixed files. Filtering by `MEDIA_VIDEO` would silently miss every one of
/// them - which is exactly what happened before this split existed: a
/// machine with a working H.265 decoder still never advertised
/// `video/x-matroska`, so a DLNA-strict control point refused to even try
/// sending an .mkv file, despite the renderer being able to play it.
fn canonical_video_container_mime_for_structure(
    structure: &gst::StructureRef,
) -> Option<&'static str> {
    match structure.name().as_str() {
        "video/x-matroska" => Some("video/x-matroska"),
        "video/quicktime" => Some("video/quicktime"),
        "video/x-msvideo" => Some("video/x-msvideo"),
        "video/x-flv" => Some("video/x-flv"),
        _ => None,
    }
}

/// Walks every decodable element of `media_type` the registry knows about
/// and collects the mime types `canonical_mime` recognizes among their sink
/// pad caps - the shared enumeration logic behind the audio, video-codec and
/// video-container searches in [`supported_mime_types`]. Pass
/// `ElementFactoryType::empty()` for `media_type` to search every decodable
/// element regardless of its declared media type - needed for container
/// detection, see [`canonical_video_container_mime_for_structure`].
fn mime_types_for(
    media_type: gst::ElementFactoryType,
    canonical_mime: fn(&gst::StructureRef) -> Option<&'static str>,
) -> BTreeSet<&'static str> {
    let factories =
        gst::ElementFactory::factories_with_type(gst::ElementFactoryType::DECODABLE | media_type, gst::Rank::MARGINAL);

    let mut mime_types = BTreeSet::new();

    for factory in factories {
        for template in factory.static_pad_templates() {
            if template.direction() != gst::PadDirection::Sink {
                continue;
            }

            for structure in template.caps().iter() {
                if let Some(mime) = canonical_mime(structure) {
                    mime_types.insert(mime);
                }
            }
        }
    }

    mime_types
}

/// The mime types this machine can actually play back end to end -
/// decoded by the installed GStreamer *and*, for video, actually
/// displayable (see the module docs) - deduplicated and sorted. Empty if
/// GStreamer itself failed to initialize.
pub fn supported_mime_types() -> Vec<String> {
    if ensure_initialized().is_err() {
        return Vec::new();
    }

    let mut mime_types = mime_types_for(
        gst::ElementFactoryType::MEDIA_AUDIO,
        canonical_audio_mime_for_structure,
    );

    if video::can_render() {
        mime_types.extend(mime_types_for(
            gst::ElementFactoryType::MEDIA_VIDEO,
            canonical_video_codec_mime_for_structure,
        ));
        mime_types.extend(mime_types_for(
            gst::ElementFactoryType::empty(),
            canonical_video_container_mime_for_structure,
        ));
    }

    mime_types.into_iter().map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_at_least_mp3_support() {
        // Every GStreamer install this project targets ships an MP3 decoder
        // (mpg123 or avdec_mp3) via gst-plugins-good/-ugly/-bad, so this
        // should never come back empty on a dev machine or CI image with
        // GStreamer installed.
        let mime_types = supported_mime_types();
        assert!(
            mime_types.iter().any(|m| m == "audio/mpeg"),
            "expected audio/mpeg among detected mime types, got {:?}",
            mime_types
        );
    }

    #[test]
    fn only_advertises_video_when_it_can_actually_be_displayed() {
        // Can't assert either direction outright: CI images legitimately
        // have no GPU adapter (`video::can_render()` false), and even a
        // real display doesn't guarantee any video *codec* is installed
        // (h264/vp8/vp9 decoders aren't as universally bundled as mp3). What
        // must always hold is the gating itself - no video mime type should
        // ever appear when nothing could render it.
        let mime_types = supported_mime_types();
        let any_video = mime_types.iter().any(|m| m.starts_with("video/"));

        if any_video {
            assert!(
                video::can_render(),
                "advertised {:?} but video::can_render() is false",
                mime_types
            );
        }
    }
}
