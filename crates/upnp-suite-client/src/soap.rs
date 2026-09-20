//! Generic SOAP action invocation — the one place that builds a UPnP
//! control request and parses its response, shared by every typed service
//! client (`AVTransportClient`, `RenderingControlClient`, ...) rather than
//! each hand-rolling its own request/response XML.

use std::collections::HashMap;

use quick_xml::escape::escape;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use reqwest::Url;

use crate::error::{ClientError, SoapFault};
use crate::xml::local_name;

/// Sends `action` to `control_url` for `service_type` (e.g.
/// `"urn:schemas-upnp-org:service:AVTransport:1"`) with `args` as the
/// action's in-arguments, and returns its out-arguments as a flat
/// name→value map — every AVTransport/RenderingControl response is a flat
/// list of scalar elements, never nested, so a map is enough to represent
/// any of them without a bespoke struct per action.
pub async fn invoke_action(
    http: &reqwest::Client,
    control_url: &Url,
    service_type: &str,
    action: &str,
    args: &[(&str, &str)],
) -> Result<HashMap<String, String>, ClientError> {
    let body = build_request_body(service_type, action, args);
    let soap_action = format!("\"{service_type}#{action}\"");

    let response = http
        .post(control_url.clone())
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .header("SOAPAction", soap_action)
        .body(body)
        .send()
        .await?;

    let status = response.status();
    let text = response.text().await?;

    if !status.is_success() {
        return Err(match parse_fault(&text) {
            Some(fault) => ClientError::Fault(fault),
            None => ClientError::MalformedResponse(format!(
                "action {action} failed with HTTP {status} and an unparseable body"
            )),
        });
    }

    let response_tag = format!("{action}Response");
    parse_response_body(&text, &response_tag)
}

fn build_request_body(service_type: &str, action: &str, args: &[(&str, &str)]) -> String {
    let mut inner = String::new();
    for (name, value) in args {
        inner.push_str(&format!("<{name}>{}</{name}>", escape(*value)));
    }

    format!(
        concat!(
            r#"<?xml version="1.0" encoding="utf-8"?>"#,
            r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" "#,
            r#"s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">"#,
            r#"<s:Body><u:{action} xmlns:u="{service_type}">{inner}</u:{action}></s:Body>"#,
            r#"</s:Envelope>"#
        ),
        action = action,
        service_type = service_type,
        inner = inner,
    )
}

/// Walks the response body for the element ending in `response_tag`
/// (namespace-prefix-insensitive, same convention the renderer's own
/// request parser uses) and collects its immediate children as a
/// name→value map. Falls back to reporting a SOAP fault, if the body has
/// one, rather than a generic "tag not found" error.
fn parse_response_body(
    xml: &str,
    response_tag: &str,
) -> Result<HashMap<String, String>, ClientError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut values = HashMap::new();
    let mut in_response = false;
    let mut current_field: Option<String> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = local_name(e.name().into_inner());
                if !in_response && name.ends_with(response_tag) {
                    in_response = true;
                } else if in_response {
                    current_field = Some(name.to_string());
                }
            }
            // A zero-argument response (e.g. `<SetVolumeResponse/>`) is
            // commonly self-closing rather than a Start/End pair - mirrors
            // the same allowance the renderer's own request parser makes
            // for zero-argument actions.
            Ok(Event::Empty(e)) => {
                let name = local_name(e.name().into_inner());
                if !in_response && name.ends_with(response_tag) {
                    return Ok(values);
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(field) = &current_field {
                    let text = t.unescape().unwrap_or_default().into_owned();
                    values.insert(field.clone(), text);
                }
            }
            Ok(Event::End(e)) => {
                let name = local_name(e.name().into_inner());
                if in_response && name.ends_with(response_tag) {
                    return Ok(values);
                }
                current_field = None;
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ClientError::MalformedResponse(format!(
                    "failed to parse SOAP response XML: {e}"
                )))
            }
            _ => {}
        }
    }

    match parse_fault(xml) {
        Some(fault) => Err(ClientError::Fault(fault)),
        None => Err(ClientError::MalformedResponse(format!(
            "response did not contain a {response_tag} element"
        ))),
    }
}

/// Best-effort extraction of `errorCode`/`errorDescription` from a UPnP
/// `s:Fault`'s `UPnPError` detail. Returns `Some(SoapFault::default())`
/// (no code/description) if the body is a fault but doesn't carry the
/// UPnP-specific detail, rather than `None` — callers should still know
/// the action failed at all, even without a code to show.
fn parse_fault(xml: &str) -> Option<SoapFault> {
    if !xml.contains("Fault") {
        return None;
    }

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut fault = SoapFault::default();
    let mut current_field: Option<String> = None;
    let mut saw_fault = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = local_name(e.name().into_inner()).to_string();
                if name == "Fault" {
                    saw_fault = true;
                }
                current_field = Some(name);
            }
            Ok(Event::Text(t)) => {
                let text = t.unescape().unwrap_or_default().into_owned();
                match current_field.as_deref() {
                    Some("errorCode") => fault.error_code = text.parse().ok(),
                    Some("errorDescription") => fault.error_description = Some(text),
                    _ => {}
                }
            }
            Ok(Event::End(_)) => current_field = None,
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    saw_fault.then_some(fault)
}
