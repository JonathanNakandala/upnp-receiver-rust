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
use crate::services::av_transport::AVTransport;

/// Everything an AVTransport `LastChange` NOTIFY reports. Gathered in one
/// place (rather than each call site pulling `TransportState` alone) so a
/// gapless track transition — where the transport state itself doesn't
/// change, only the URI does — still produces a NOTIFY a subscriber can see,
/// and so a client relying on eventing rather than polling `GetMediaInfo`
/// can follow `AVTransportURI`/`NextAVTransportURI` in real time.
pub struct AVTransportEventState {
    pub transport_state: TransportState,
    pub current_uri: String,
    pub current_metadata: Option<String>,
    pub next_uri: String,
    pub next_metadata: Option<String>,
}

impl AVTransportEventState {
    /// Builds the current snapshot from the live `AVTransport` — the state
    /// changing is what triggers a NOTIFY, but the NOTIFY body always
    /// reflects a fresh read rather than carrying stale values forward.
    pub async fn snapshot(av_transport: &dyn AVTransport) -> Self {
        let transport_state = av_transport
            .get_state()
            .await
            .unwrap_or(TransportState::NoMediaPresent);
        let media = av_transport.get_media_info().await.ok().flatten();

        Self {
            transport_state,
            current_uri: media.as_ref().map(|m| m.uri.clone()).unwrap_or_default(),
            current_metadata: media.as_ref().and_then(|m| m.metadata.clone()),
            next_uri: media.as_ref().map(|m| m.next_uri.clone()).unwrap_or_default(),
            next_metadata: media.as_ref().and_then(|m| m.next_metadata.clone()),
        }
    }
}

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
pub fn av_transport_last_change(event: &AVTransportEventState) -> String {
    let inner = format!(
        concat!(
            r#"<Event xmlns="urn:schemas-upnp-org:metadata-1-0/AVT/">"#,
            r#"<InstanceID val="0">"#,
            r#"<TransportState val="{state}"/>"#,
            r#"<CurrentTransportActions val="{actions}"/>"#,
            r#"<AVTransportURI val="{uri}"/>"#,
            r#"<AVTransportURIMetaData val="{metadata}"/>"#,
            r#"<NextAVTransportURI val="{next_uri}"/>"#,
            r#"<NextAVTransportURIMetaData val="{next_metadata}"/>"#,
            r#"</InstanceID>"#,
            r#"</Event>"#
        ),
        state = transport_state_str(event.transport_state),
        actions = transport_actions_for(event.transport_state),
        uri = escape(&event.current_uri),
        metadata = escape(event.current_metadata.as_deref().unwrap_or("")),
        next_uri = escape(&event.next_uri),
        next_metadata = escape(event.next_metadata.as_deref().unwrap_or("")),
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

    fn event_state(transport_state: TransportState) -> AVTransportEventState {
        AVTransportEventState {
            transport_state,
            current_uri: String::new(),
            current_metadata: None,
            next_uri: String::new(),
            next_metadata: None,
        }
    }

    #[test]
    fn av_transport_event_reports_playing_state_and_actions() {
        let body = av_transport_last_change(&event_state(TransportState::Playing));

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
        let body = av_transport_last_change(&event_state(TransportState::Stopped));
        assert!(body.contains("TransportState val=&quot;STOPPED&quot;"));
        assert!(body.contains("CurrentTransportActions val=&quot;Play&quot;"));
    }

    #[test]
    fn av_transport_event_reports_uris() {
        let body = av_transport_last_change(&AVTransportEventState {
            transport_state: TransportState::Playing,
            current_uri: "http://example.com/a.mp3".to_string(),
            current_metadata: Some("<DIDL-Lite>\"quoted\"</DIDL-Lite>".to_string()),
            next_uri: "http://example.com/b.mp3".to_string(),
            next_metadata: None,
        });

        assert!(body.contains("AVTransportURI val=&quot;http://example.com/a.mp3&quot;"));
        assert!(body.contains("NextAVTransportURI val=&quot;http://example.com/b.mp3&quot;"));
        // Metadata containing its own quotes/angle brackets must not break
        // the inner XML's attribute syntax — this is the double-escape
        // (once for the attribute, once for embedding as LastChange text).
        assert!(body.contains(
            "AVTransportURIMetaData val=&quot;&amp;lt;DIDL-Lite&amp;gt;&amp;quot;quoted&amp;quot;&amp;lt;/DIDL-Lite&amp;gt;&quot;"
        ));
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
