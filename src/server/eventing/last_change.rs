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

use crate::server::soap::handlers::av_transport::utils::{format_duration, format_play_speed};
use crate::server::soap::types::av_transport::{PlayMode, TransportState, TransportStatus};
use crate::services::av_transport::AVTransport;

/// Everything an AVTransport `LastChange` NOTIFY reports. Gathered in one
/// place (rather than each call site pulling `TransportState` alone) so a
/// gapless track transition — where the transport state itself doesn't
/// change, only the URI does — still produces a NOTIFY a subscriber can see,
/// and so a client relying on eventing rather than polling `GetMediaInfo`
/// can follow `AVTransportURI`/`NextAVTransportURI` in real time.
pub struct AVTransportEventState {
    pub transport_state: TransportState,
    pub transport_status: TransportStatus,
    pub play_speed: f64,
    pub current_uri: String,
    pub current_metadata: Option<String>,
    pub next_uri: String,
    pub next_metadata: Option<String>,
    pub track_duration: String,
    pub play_mode: PlayMode,
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
        let transport_status = av_transport
            .get_status()
            .await
            .unwrap_or(TransportStatus::OK);
        let play_speed = av_transport.get_play_speed().await.unwrap_or(1.0);
        let play_mode = av_transport.get_play_mode().await.unwrap_or(PlayMode::Normal);
        let media = av_transport.get_media_info().await.ok().flatten();

        Self {
            transport_state,
            transport_status,
            play_speed,
            current_uri: media.as_ref().map(|m| m.uri.clone()).unwrap_or_default(),
            current_metadata: media.as_ref().and_then(|m| m.metadata.clone()),
            next_uri: media.as_ref().map(|m| m.next_uri.clone()).unwrap_or_default(),
            next_metadata: media.as_ref().and_then(|m| m.next_metadata.clone()),
            track_duration: media
                .as_ref()
                .and_then(|m| m.duration)
                .map(format_duration)
                .unwrap_or_else(|| "00:00:00".to_string()),
            play_mode,
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

fn transport_status_str(status: TransportStatus) -> &'static str {
    match status {
        TransportStatus::OK => "OK",
        TransportStatus::ErrorOccurred => "ERROR_OCCURRED",
    }
}

/// The `CurrentTransportActions` values a control point can rely on the
/// renderer accepting given `state`. Also the source of truth for the
/// `GetCurrentTransportActions` action's `Actions` return value — see
/// `crate::server::soap::handlers::av_transport::action`.
pub fn transport_actions_for(state: TransportState) -> &'static str {
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
    // Single-track player: there's no playlist, so NumberOfTracks/CurrentTrack
    // are always 0 (no media) or 1 (media loaded), and CurrentTrackURI/
    // CurrentTrackMetaData/CurrentTrackDuration mirror the AVTransportURI
    // trio rather than tracking a separate "now playing within the queue"
    // position.
    let track_count = if event.current_uri.is_empty() { 0 } else { 1 };

    let inner = format!(
        concat!(
            r#"<Event xmlns="urn:schemas-upnp-org:metadata-1-0/AVT/">"#,
            r#"<InstanceID val="0">"#,
            r#"<TransportState val="{state}"/>"#,
            r#"<TransportStatus val="{status}"/>"#,
            r#"<TransportPlaySpeed val="{speed}"/>"#,
            r#"<CurrentPlayMode val="{play_mode}"/>"#,
            r#"<NumberOfTracks val="{track_count}"/>"#,
            r#"<CurrentTrack val="{track_count}"/>"#,
            r#"<CurrentTrackDuration val="{track_duration}"/>"#,
            r#"<CurrentMediaDuration val="{track_duration}"/>"#,
            r#"<CurrentTrackURI val="{uri}"/>"#,
            r#"<CurrentTrackMetaData val="{metadata}"/>"#,
            r#"<CurrentTransportActions val="{actions}"/>"#,
            r#"<AVTransportURI val="{uri}"/>"#,
            r#"<AVTransportURIMetaData val="{metadata}"/>"#,
            r#"<NextAVTransportURI val="{next_uri}"/>"#,
            r#"<NextAVTransportURIMetaData val="{next_metadata}"/>"#,
            r#"</InstanceID>"#,
            r#"</Event>"#
        ),
        state = transport_state_str(event.transport_state),
        status = transport_status_str(event.transport_status),
        speed = format_play_speed(event.play_speed),
        play_mode = event.play_mode.as_str(),
        track_count = track_count,
        track_duration = escape(&event.track_duration),
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

/// Builds the NOTIFY body for ConnectionManager's one evented state
/// variable, `CurrentConnectionIDs`. Unlike AVTransport/RenderingControl,
/// ConnectionManager's SCPD doesn't bundle its evented variable into a
/// synthetic `LastChange` string — it's `sendEvents="yes"` directly, so it's
/// its own `<e:property>` rather than XML-escaped text inside one.
pub fn connection_manager_current_ids_event(connection_ids: &str) -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="utf-8"?>"#,
            r#"<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">"#,
            r#"<e:property><CurrentConnectionIDs>{}</CurrentConnectionIDs></e:property>"#,
            r#"</e:propertyset>"#
        ),
        escape(connection_ids)
    )
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
            transport_status: TransportStatus::OK,
            play_speed: 1.0,
            current_uri: String::new(),
            current_metadata: None,
            next_uri: String::new(),
            next_metadata: None,
            track_duration: "00:00:00".to_string(),
            play_mode: PlayMode::Normal,
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
            transport_status: TransportStatus::OK,
            play_speed: 1.0,
            current_uri: "http://example.com/a.mp3".to_string(),
            current_metadata: Some("<DIDL-Lite>\"quoted\"</DIDL-Lite>".to_string()),
            next_uri: "http://example.com/b.mp3".to_string(),
            next_metadata: None,
            track_duration: "00:03:30".to_string(),
            play_mode: PlayMode::RepeatAll,
        });

        assert!(body.contains("AVTransportURI val=&quot;http://example.com/a.mp3&quot;"));
        assert!(body.contains("NextAVTransportURI val=&quot;http://example.com/b.mp3&quot;"));
        assert!(body.contains("CurrentTrackURI val=&quot;http://example.com/a.mp3&quot;"));
        assert!(body.contains("CurrentTrackDuration val=&quot;00:03:30&quot;"));
        assert!(body.contains("NumberOfTracks val=&quot;1&quot;"));
        assert!(body.contains("CurrentTrack val=&quot;1&quot;"));
        assert!(body.contains("CurrentPlayMode val=&quot;REPEAT_ALL&quot;"));
        // Metadata containing its own quotes/angle brackets must not break
        // the inner XML's attribute syntax — this is the double-escape
        // (once for the attribute, once for embedding as LastChange text).
        assert!(body.contains(
            "AVTransportURIMetaData val=&quot;&amp;lt;DIDL-Lite&amp;gt;&amp;quot;quoted&amp;quot;&amp;lt;/DIDL-Lite&amp;gt;&quot;"
        ));
        assert!(body.contains(
            "CurrentTrackMetaData val=&quot;&amp;lt;DIDL-Lite&amp;gt;&amp;quot;quoted&amp;quot;&amp;lt;/DIDL-Lite&amp;gt;&quot;"
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

    #[test]
    fn connection_manager_event_reports_current_ids_as_a_direct_property() {
        let body = connection_manager_current_ids_event("0");

        assert!(body.starts_with(r#"<?xml version="1.0" encoding="utf-8"?>"#));
        assert!(body.contains("urn:schemas-upnp-org:event-1-0"));
        // No LastChange wrapper/double-escaping here — CurrentConnectionIDs
        // is evented directly, so its value appears as plain element text.
        assert!(body.contains("<CurrentConnectionIDs>0</CurrentConnectionIDs>"));
        assert!(!body.contains("LastChange"));
    }
}
