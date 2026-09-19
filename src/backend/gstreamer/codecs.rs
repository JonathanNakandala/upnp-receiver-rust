//! Enumerates the audio formats the installed GStreamer decodes, so
//! [`ConnectionManager::GetProtocolInfo`] can advertise what this renderer
//! can actually play instead of a single hardcoded entry — different
//! machines have different plugin sets installed (e.g. `gstreamer1-libav`
//! for AAC/MP4 pulls in decoders a bare `gst-plugins-base`/`-good` install
//! doesn't have).
//!
//! GStreamer doesn't have a notion of "supported file mime type" — it has
//! elements (decoders, demuxers, parsers) with sink pad templates describing
//! the caps they accept. We walk every audio-capable "decodable" element
//! (`ElementFactoryType::DECODABLE`: decoders, demuxers, parsers,
//! depayloaders, decryptors) the registry knows about, collect the caps
//! structure names on their sink pads, and map the ones we recognize to the
//! mime type a UPnP/DLNA control point expects to see in `protocolInfo`.
//! That mapping is necessarily approximate: GStreamer's caps naming doesn't
//! line up 1:1 with IANA/DLNA mime types (most notably `audio/mpeg` covers
//! both MP3 and raw AAC depending on `mpegversion`/`layer`).
//!
//! [`ConnectionManager::GetProtocolInfo`]: crate::services::ConnectionManager::get_protocol_info

use gstreamer as gst;
use gstreamer::prelude::*;
use std::collections::BTreeSet;

use super::backend::ensure_initialized;

/// Maps one GStreamer caps structure (as seen on a sink pad template) to the
/// mime type a UPnP control point would recognize, or `None` if it's not a
/// format we know how to name (e.g. an internal/raw caps type that's never
/// itself a playable file, like `audio/x-raw`).
fn canonical_mime_for_structure(structure: &gst::StructureRef) -> Option<&'static str> {
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

/// The mime types this machine's GStreamer install can actually decode,
/// deduplicated and sorted. Empty if GStreamer itself failed to initialize.
pub fn supported_mime_types() -> Vec<String> {
    if ensure_initialized().is_err() {
        return Vec::new();
    }

    let factories = gst::ElementFactory::factories_with_type(
        gst::ElementFactoryType::DECODABLE | gst::ElementFactoryType::MEDIA_AUDIO,
        gst::Rank::MARGINAL,
    );

    let mut mime_types = BTreeSet::new();

    for factory in factories {
        for template in factory.static_pad_templates() {
            if template.direction() != gst::PadDirection::Sink {
                continue;
            }

            for structure in template.caps().iter() {
                if let Some(mime) = canonical_mime_for_structure(structure) {
                    mime_types.insert(mime);
                }
            }
        }
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
}
