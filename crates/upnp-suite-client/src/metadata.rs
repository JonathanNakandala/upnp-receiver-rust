//! Reading hints out of a track's DIDL-Lite metadata - just enough to
//! answer questions a control point UI needs, not a full DIDL-Lite parser.

/// Whether `metadata`'s DIDL-Lite hints at video content, checked as a
/// plain substring search rather than XML parsing - mirrors
/// `upnp-suite-renderer`'s own `video::metadata_suggests_video`, which
/// decides whether that renderer opens a video window for a track; this is
/// the same question asked from a control point's side, e.g. to decide
/// whether picture controls (`Brightness`/`Contrast`) make sense to offer
/// for the currently playing item.
pub fn suggests_video(metadata: Option<&str>) -> bool {
    metadata.is_some_and(|metadata| {
        metadata.contains(":video/") || metadata.contains("object.item.videoItem")
    })
}

#[cfg(test)]
mod tests {
    use super::suggests_video;

    #[test]
    fn detects_video_from_res_protocol_info() {
        let metadata = r#"<res protocolInfo="http-get:*:video/x-matroska:*">http://x</res>"#;
        assert!(suggests_video(Some(metadata)));
    }

    #[test]
    fn detects_video_from_upnp_class() {
        let metadata = "<upnp:class>object.item.videoItem</upnp:class>";
        assert!(suggests_video(Some(metadata)));
    }

    #[test]
    fn does_not_flag_audio_metadata_as_video() {
        let metadata = r#"<res protocolInfo="http-get:*:audio/mpeg:*">http://x</res>"#;
        assert!(!suggests_video(Some(metadata)));
        assert!(!suggests_video(None));
    }
}
