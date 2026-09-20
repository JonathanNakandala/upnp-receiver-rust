//! Parses GENA `LastChange` NOTIFY bodies - the first place in this
//! codebase anything *deserializes* one; `upnp-suite-renderer`'s own
//! `server::eventing::last_change` only ever serializes it.
//!
//! A NOTIFY body is a `propertyset` whose one property, `LastChange`,
//! carries a second, escaped XML document as its text - an `Event` element
//! whose `InstanceID` child holds every changed state variable as a
//! `<VariableName val="..."/>` (AVTransport) or
//! `<VariableName channel="Master" val="..."/>` (RenderingControl,
//! `Volume`/`VolumeDB`/`Mute`) element. Parsing happens in two passes:
//! unwrap the outer `propertyset` to get the inner XML string, then parse
//! that string's `val` attributes into a flat name→value map.

use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::ClientError;
use crate::xml::local_name;

/// What an AVTransport `LastChange` NOTIFY reports - mirrors the fields
/// `upnp-suite-renderer`'s own `AVTransportEventState` snapshots, since
/// that's the same information travelling over the wire either way.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AVTransportSnapshot {
    pub transport_state: String,
    pub transport_status: String,
    /// The raw `TransportPlaySpeed` string (e.g. `"1"`, `"1/2"`).
    pub play_speed: String,
    pub current_uri: String,
    pub current_metadata: Option<String>,
    pub next_uri: String,
    pub next_metadata: Option<String>,
    pub play_mode: String,
}

/// What a RenderingControl `LastChange` NOTIFY reports.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RenderingControlSnapshot {
    pub volume: u16,
    pub muted: bool,
}

pub fn parse_av_transport_last_change(
    notify_body: &str,
) -> Result<AVTransportSnapshot, ClientError> {
    let fields = extract_val_attributes(notify_body)?;

    Ok(AVTransportSnapshot {
        transport_state: fields.get("TransportState").cloned().unwrap_or_default(),
        transport_status: fields.get("TransportStatus").cloned().unwrap_or_default(),
        play_speed: fields.get("TransportPlaySpeed").cloned().unwrap_or_default(),
        current_uri: fields.get("AVTransportURI").cloned().unwrap_or_default(),
        current_metadata: non_empty(fields.get("AVTransportURIMetaData")),
        next_uri: fields.get("NextAVTransportURI").cloned().unwrap_or_default(),
        next_metadata: non_empty(fields.get("NextAVTransportURIMetaData")),
        play_mode: fields.get("CurrentPlayMode").cloned().unwrap_or_default(),
    })
}

pub fn parse_rendering_control_last_change(
    notify_body: &str,
) -> Result<RenderingControlSnapshot, ClientError> {
    let fields = extract_val_attributes(notify_body)?;

    let volume = fields
        .get("Volume")
        .and_then(|v| v.parse().ok())
        .unwrap_or_default();
    // Same "true"/"false" vs "0"/"1" looseness as the SOAP client - see
    // `RenderingControlClient::get_mute`.
    let muted = fields
        .get("Mute")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    Ok(RenderingControlSnapshot { volume, muted })
}

fn non_empty(value: Option<&String>) -> Option<String> {
    value.filter(|s| !s.is_empty()).cloned()
}

/// Unwraps the `propertyset`/`property`/`LastChange` envelope and returns
/// the inner `Event` document's `val` attributes, keyed by element local
/// name. Element names are assumed unique within one event (true for every
/// AVTransport/RenderingControl `LastChange` this renderer or a spec-
/// compliant one emits — `Volume`/`Mute` each appear once per channel, and
/// this renderer only ever has one, `Master`).
fn extract_val_attributes(notify_body: &str) -> Result<HashMap<String, String>, ClientError> {
    let inner = extract_last_change_text(notify_body)?;

    let mut reader = Reader::from_str(&inner);
    reader.config_mut().trim_text(true);

    let mut fields = HashMap::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = local_name(e.name().into_inner()).to_string();
                if name == "Event" || name == "InstanceID" {
                    continue;
                }
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"val" {
                        let value = attr.unescape_value().unwrap_or_default().into_owned();
                        fields.insert(name.clone(), value);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ClientError::MalformedResponse(format!(
                    "failed to parse LastChange event XML: {e}"
                )))
            }
            _ => {}
        }
    }

    Ok(fields)
}

/// Finds the `LastChange` property in a NOTIFY body's `propertyset` and
/// returns its (unescaped) text - itself a full XML document, escaped once
/// as ordinary element text by the sender.
fn extract_last_change_text(xml: &str) -> Result<String, ClientError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut in_last_change = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                if local_name(e.name().into_inner()) == "LastChange" {
                    in_last_change = true;
                }
            }
            Ok(Event::Text(t)) if in_last_change => {
                return t.unescape().map(|s| s.into_owned()).map_err(|e| {
                    ClientError::MalformedResponse(format!("failed to unescape LastChange: {e}"))
                });
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ClientError::MalformedResponse(format!(
                    "failed to parse NOTIFY body XML: {e}"
                )))
            }
            _ => {}
        }
    }

    Err(ClientError::MalformedResponse(
        "NOTIFY body has no LastChange property".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notify_body(inner_event_xml: &str) -> String {
        format!(
            concat!(
                r#"<?xml version="1.0" encoding="utf-8"?>"#,
                r#"<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">"#,
                r#"<e:property><LastChange>{}</LastChange></e:property>"#,
                r#"</e:propertyset>"#
            ),
            quick_xml::escape::escape(inner_event_xml)
        )
    }

    #[test]
    fn parses_av_transport_last_change() {
        let body = notify_body(concat!(
            r#"<Event xmlns="urn:schemas-upnp-org:metadata-1-0/AVT/">"#,
            r#"<InstanceID val="0">"#,
            r#"<TransportState val="PLAYING"/>"#,
            r#"<TransportStatus val="OK"/>"#,
            r#"<TransportPlaySpeed val="1"/>"#,
            r#"<CurrentPlayMode val="NORMAL"/>"#,
            r#"<AVTransportURI val="http://example.com/a.mp3"/>"#,
            r#"<AVTransportURIMetaData val=""/>"#,
            r#"<NextAVTransportURI val=""/>"#,
            r#"<NextAVTransportURIMetaData val=""/>"#,
            r#"</InstanceID>"#,
            r#"</Event>"#
        ));

        let snapshot = parse_av_transport_last_change(&body).unwrap();
        assert_eq!(snapshot.transport_state, "PLAYING");
        assert_eq!(snapshot.transport_status, "OK");
        assert_eq!(snapshot.play_speed, "1");
        assert_eq!(snapshot.current_uri, "http://example.com/a.mp3");
        assert_eq!(snapshot.current_metadata, None);
        assert_eq!(snapshot.play_mode, "NORMAL");
    }

    #[test]
    fn parses_rendering_control_last_change() {
        let body = notify_body(concat!(
            r#"<Event xmlns="urn:schemas-upnp-org:metadata-1-0/RCS/">"#,
            r#"<InstanceID val="0">"#,
            r#"<Volume channel="Master" val="42"/>"#,
            r#"<VolumeDB channel="Master" val="-20"/>"#,
            r#"<Mute channel="Master" val="1"/>"#,
            r#"</InstanceID>"#,
            r#"</Event>"#
        ));

        let snapshot = parse_rendering_control_last_change(&body).unwrap();
        assert_eq!(snapshot.volume, 42);
        assert!(snapshot.muted);
    }

    #[test]
    fn rejects_a_body_with_no_last_change_property() {
        let body = r#"<?xml version="1.0"?><e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0"></e:propertyset>"#;
        assert!(parse_av_transport_last_change(body).is_err());
    }
}
