//! Builds the `LastChange` event payloads for AVTransport and
//! RenderingControl, and wraps them in the outer GENA `propertyset` envelope
//! every NOTIFY body uses regardless of service.
//!
//! Per the AVTransport/RenderingControl eventing model, individual state
//! variables (`TransportState`, `Volume`, ...) are *not* evented directly —
//! everything that changed is bundled into one `LastChange` string, itself
//! XML, escaped and carried as the text of a single evented property. See
//! the `sendEvents="no"` vs `sendEvents="yes"` split in the service SCPDs.

use quick_xml::escape::escape;

use crate::server::soap::types::av_transport::TransportState;

fn transport_state_str(state: TransportState) -> &'static str {
    match state {
        TransportState::Stopped => "STOPPED",
        TransportState::Playing => "PLAYING",
        TransportState::Transitioning => "TRANSITIONING",
        TransportState::PausedPlayback => "PAUSED_PLAYBACK",
        TransportState::PausedRecording => "PAUSED_RECORDING",
        TransportState::Recording => "RECORDING",
        TransportState::NoMediaPresent => "NO_MEDIA_PRESENT",
    }
}

/// The `CurrentTransportActions` values a control point can rely on the
/// renderer accepting given `state` — informational only, no code here reads
/// it back.
fn transport_actions_for(state: TransportState) -> &'static str {
    use TransportState::*;
    match state {
        Playing => "Pause,Stop,Seek",
        PausedPlayback | PausedRecording => "Play,Stop,Seek",
        Stopped => "Play",
        NoMediaPresent | Transitioning | Recording => "",
    }
}

/// Builds the `LastChange` inner XML for an AVTransport state change and
/// wraps it in the GENA `propertyset` envelope, ready to send as a NOTIFY
/// body.
pub fn av_transport_last_change(state: TransportState) -> String {
    let inner = format!(
        concat!(
            r#"<Event xmlns="urn:schemas-upnp-org:metadata-1-0/AVT/">"#,
            r#"<InstanceID val="0">"#,
            r#"<TransportState val="{state}"/>"#,
            r#"<CurrentTransportActions val="{actions}"/>"#,
            r#"</InstanceID>"#,
            r#"</Event>"#
        ),
        state = transport_state_str(state),
        actions = transport_actions_for(state),
    );

    wrap_property_set(&inner)
}

/// Builds the `LastChange` inner XML for a RenderingControl state change
/// (volume and/or mute) and wraps it in the GENA `propertyset` envelope.
pub fn rendering_control_last_change(volume: u16, muted: bool) -> String {
    let inner = format!(
        concat!(
            r#"<Event xmlns="urn:schemas-upnp-org:metadata-1-0/RCS/">"#,
            r#"<InstanceID val="0">"#,
            r#"<Volume channel="Master" val="{volume}"/>"#,
            r#"<Mute channel="Master" val="{muted}"/>"#,
            r#"</InstanceID>"#,
            r#"</Event>"#
        ),
        volume = volume,
        muted = if muted { "1" } else { "0" },
    );

    wrap_property_set(&inner)
}

fn wrap_property_set(last_change_inner_xml: &str) -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="utf-8"?>"#,
            r#"<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">"#,
            r#"<e:property><LastChange>{}</LastChange></e:property>"#,
            r#"</e:propertyset>"#
        ),
        escape(last_change_inner_xml)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn av_transport_event_reports_playing_state_and_actions() {
        let body = av_transport_last_change(TransportState::Playing);

        assert!(body.starts_with(r#"<?xml version="1.0" encoding="utf-8"?>"#));
        assert!(body.contains("urn:schemas-upnp-org:event-1-0"));
        assert!(body.contains("<LastChange>"));
        // The inner Event XML is escaped as text, so its angle brackets show
        // up entity-encoded in the outer body.
        assert!(body.contains("&lt;Event xmlns=&quot;urn:schemas-upnp-org:metadata-1-0/AVT/&quot;&gt;"));
        assert!(body.contains("TransportState val=&quot;PLAYING&quot;"));
        assert!(body.contains("CurrentTransportActions val=&quot;Pause,Stop,Seek&quot;"));
    }

    #[test]
    fn av_transport_event_reports_stopped_actions() {
        let body = av_transport_last_change(TransportState::Stopped);
        assert!(body.contains("TransportState val=&quot;STOPPED&quot;"));
        assert!(body.contains("CurrentTransportActions val=&quot;Play&quot;"));
    }

    #[test]
    fn rendering_control_event_reports_volume_and_mute() {
        let body = rendering_control_last_change(42, true);

        assert!(body.contains("urn:schemas-upnp-org:metadata-1-0/RCS/"));
        assert!(body.contains("Volume channel=&quot;Master&quot; val=&quot;42&quot;"));
        assert!(body.contains("Mute channel=&quot;Master&quot; val=&quot;1&quot;"));
    }

    #[test]
    fn rendering_control_event_reports_unmuted_as_zero() {
        let body = rendering_control_last_change(10, false);
        assert!(body.contains("Mute channel=&quot;Master&quot; val=&quot;0&quot;"));
    }
}
