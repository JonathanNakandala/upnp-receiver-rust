use std::str::Utf8Error;

use crate::services::error::{ConnectionError, ServiceError, TransportError};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use quick_xml::escape::escape;
use quick_xml::DeError;
use thiserror::Error;

/// A UPnP `<UPnPError>` code/description pair, per the Control section of
/// the UPnP Device Architecture spec and the AVTransport:1 error table.
/// Control points branch on the numeric code (retry vs. surface an error,
/// which message to show) — the SOAP faultstring is for logs, not them.
struct UPnPErrorCode {
    code: u16,
    description: &'static str,
}

#[derive(Error, Debug)]
pub enum SoapError {
    #[error("SOAP error: {0}")]
    Soap(#[from] SoapProtocolError),

    #[error("Service error: {0}")]
    Service(#[from] ServiceError),

    #[error("Parsing error: {0}")]
    ParsingError(#[from] DeError),

    #[error("QuickXML error: {0}")]
    QuickXMLError(#[from] quick_xml::Error),

    #[error("UTF-8 error: {0}")]
    Utf8Error(#[from] Utf8Error),
}

impl From<TransportError> for SoapError {
    fn from(err: TransportError) -> Self {
        SoapError::Service(ServiceError::Transport(err))
    }
}

impl SoapError {
    fn upnp_error(&self) -> UPnPErrorCode {
        match self {
            SoapError::Soap(e) => e.upnp_error(),
            SoapError::Service(e) => e.upnp_error(),
            // These never reach an action handler, so there's no
            // action-specific error to report — "Invalid Args" is the
            // closest generic fit for a request UPnP couldn't even parse.
            SoapError::ParsingError(_) | SoapError::QuickXMLError(_) | SoapError::Utf8Error(_) => {
                UPnPErrorCode {
                    code: 402,
                    description: "Invalid Args",
                }
            }
        }
    }
}

impl IntoResponse for SoapError {
    fn into_response(self) -> Response {
        let upnp_error = self.upnp_error();

        let (status, error_message) = match &self {
            SoapError::Soap(e) => (
                StatusCode::BAD_REQUEST,
                format!("SOAP Protocol Error: {}", e),
            ),
            SoapError::Service(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Service Error: {}", e),
            ),
            SoapError::ParsingError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Parsing Error Error: {}", e),
            ),
            SoapError::QuickXMLError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Parsing Error Error: {}", e),
            ),
            SoapError::Utf8Error(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Parsing Error Error: {}", e),
            ),
        };

        // Create a SOAP Fault response, with the UPnPError detail control
        // points parse to decide retry/UI behavior. faultstring carries
        // free-form (possibly user-influenced, e.g. echoed request values)
        // text, so it's escaped; errorDescription is always one of our own
        // static strings.
        let soap_fault = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
    <s:Body>
        <s:Fault>
            <faultcode>s:Client</faultcode>
            <faultstring>{fault_string}</faultstring>
            <detail>
                <UPnPError xmlns="urn:schemas-upnp-org:control-1-0">
                    <errorCode>{code}</errorCode>
                    <errorDescription>{description}</errorDescription>
                </UPnPError>
            </detail>
        </s:Fault>
    </s:Body>
</s:Envelope>"#,
            fault_string = escape(&error_message),
            code = upnp_error.code,
            description = upnp_error.description,
        );

        (status, [("Content-Type", "text/xml")], soap_fault).into_response()
    }
}
#[derive(Error, Debug)]
pub enum SoapProtocolError {
    #[error("Invalid SOAP action: {0}")]
    InvalidAction(String),

    #[error("Not Implemented SOAP action: {0}")]
    NotImplemented(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Serialisation Error request: {0}")]
    SerializationError(String),
    #[error("Missing Fieldt: {0}")]
    MissingField(&'static str),

    #[error("Seek mode not supported: {0}")]
    SeekModeNotSupported(String),

    #[error("Illegal seek target: {0}")]
    IllegalSeekTarget(String),

    #[error("Play speed not supported: {0}")]
    PlaySpeedNotSupported(String),

    #[error("Invalid connection reference: {0}")]
    InvalidConnectionReference(String),

    #[error("Play mode not supported: {0}")]
    PlayModeNotSupported(String),

    #[error("Invalid preset name: {0}")]
    InvalidPresetName(String),
}

impl SoapProtocolError {
    fn upnp_error(&self) -> UPnPErrorCode {
        match self {
            // Covers both "no action by that name at all" and "action
            // exists elsewhere but this service's dispatcher doesn't route
            // it" (e.g. AVTransport actions we don't implement yet) — 401
            // is the "no such action" code. 602 is reserved for actions the
            // SCPD itself advertises as optional and then declines.
            SoapProtocolError::InvalidAction(_) | SoapProtocolError::NotImplemented(_) => {
                UPnPErrorCode {
                    code: 401,
                    description: "Invalid Action",
                }
            }
            SoapProtocolError::ParseError(_)
            | SoapProtocolError::MissingField(_)
            | SoapProtocolError::InvalidRequest(_) => UPnPErrorCode {
                code: 402,
                description: "Invalid Args",
            },
            SoapProtocolError::SerializationError(_) => UPnPErrorCode {
                code: 501,
                description: "Action Failed",
            },
            SoapProtocolError::SeekModeNotSupported(_) => UPnPErrorCode {
                code: 710,
                description: "Seek Mode Not Supported",
            },
            SoapProtocolError::IllegalSeekTarget(_) => UPnPErrorCode {
                code: 711,
                description: "Illegal Seek Target",
            },
            SoapProtocolError::PlaySpeedNotSupported(_) => UPnPErrorCode {
                code: 717,
                description: "Play Speed Not Supported",
            },
            SoapProtocolError::InvalidConnectionReference(_) => UPnPErrorCode {
                code: 706,
                description: "Invalid Connection Reference",
            },
            SoapProtocolError::PlayModeNotSupported(_) => UPnPErrorCode {
                code: 712,
                description: "Play Mode Not Supported",
            },
            SoapProtocolError::InvalidPresetName(_) => UPnPErrorCode {
                code: 701,
                description: "Invalid Preset Name",
            },
        }
    }
}

impl ServiceError {
    fn upnp_error(&self) -> UPnPErrorCode {
        match self {
            ServiceError::Transport(e) => e.upnp_error(),
            ServiceError::Connection(ConnectionError::UnsupportedProtocol(_)) => UPnPErrorCode {
                code: 714,
                description: "Illegal MIME-Type",
            },
            ServiceError::Connection(_)
            | ServiceError::Protocol(_)
            | ServiceError::Io(_)
            | ServiceError::Template(_)
            | ServiceError::InvalidValue(_)
            | ServiceError::IoError(_)
            | ServiceError::Network(_) => UPnPErrorCode {
                code: 501,
                description: "Action Failed",
            },
        }
    }
}

impl TransportError {
    fn upnp_error(&self) -> UPnPErrorCode {
        match self {
            TransportError::InvalidStateTransition { .. } => UPnPErrorCode {
                code: 701,
                description: "Transition Not Available",
            },
            TransportError::NoMedia => UPnPErrorCode {
                code: 702,
                description: "No Contents",
            },
            // Best-effort default: a media load failure could also be a
            // format problem (714), but ResourceNotFound is the safer
            // generic assumption since MediaLoadFailed doesn't currently
            // distinguish "couldn't fetch" from "couldn't decode".
            TransportError::MediaLoadFailed(_) => UPnPErrorCode {
                code: 716,
                description: "Resource Not Found",
            },
            TransportError::SeekError { .. } => UPnPErrorCode {
                code: 711,
                description: "Illegal Seek Target",
            },
            TransportError::UnsupportedOperation(_) => UPnPErrorCode {
                code: 701,
                description: "Transition Not Available",
            },
            TransportError::Internal(_) | TransportError::Other(_) => UPnPErrorCode {
                code: 501,
                description: "Action Failed",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use std::time::Duration;

    async fn fault_body(err: SoapError) -> String {
        let response = err.into_response();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn invalid_state_transition_reports_701() {
        let err = SoapError::from(TransportError::InvalidStateTransition {
            from: crate::server::soap::types::av_transport::TransportState::Stopped,
            to: crate::server::soap::types::av_transport::TransportState::PausedPlayback,
        });
        let body = fault_body(err).await;
        assert!(body.contains("<errorCode>701</errorCode>"));
        assert!(body.contains("urn:schemas-upnp-org:control-1-0"));
    }

    #[tokio::test]
    async fn no_media_reports_702() {
        let body = fault_body(SoapError::from(TransportError::NoMedia)).await;
        assert!(body.contains("<errorCode>702</errorCode>"));
    }

    #[tokio::test]
    async fn seek_error_reports_711() {
        let body = fault_body(SoapError::from(TransportError::SeekError {
            requested: Duration::from_secs(90),
            duration: Some(Duration::from_secs(30)),
        }))
        .await;
        assert!(body.contains("<errorCode>711</errorCode>"));
    }

    #[tokio::test]
    async fn illegal_seek_target_reports_711() {
        let body = fault_body(SoapError::Soap(SoapProtocolError::IllegalSeekTarget(
            "not a duration".to_string(),
        )))
        .await;
        assert!(body.contains("<errorCode>711</errorCode>"));
    }

    #[tokio::test]
    async fn seek_mode_not_supported_reports_710() {
        let body = fault_body(SoapError::Soap(SoapProtocolError::SeekModeNotSupported(
            "TRACK_NR".to_string(),
        )))
        .await;
        assert!(body.contains("<errorCode>710</errorCode>"));
    }

    #[tokio::test]
    async fn play_speed_not_supported_reports_717() {
        let body = fault_body(SoapError::Soap(SoapProtocolError::PlaySpeedNotSupported(
            "8".to_string(),
        )))
        .await;
        assert!(body.contains("<errorCode>717</errorCode>"));
    }

    #[tokio::test]
    async fn unknown_connection_id_reports_706() {
        let body = fault_body(SoapError::Soap(SoapProtocolError::InvalidConnectionReference(
            "99".to_string(),
        )))
        .await;
        assert!(body.contains("<errorCode>706</errorCode>"));
    }

    #[tokio::test]
    async fn unrecognized_action_reports_401() {
        let body = fault_body(SoapError::Soap(SoapProtocolError::InvalidAction(
            "Bogus".to_string(),
        )))
        .await;
        assert!(body.contains("<errorCode>401</errorCode>"));
    }

    #[tokio::test]
    async fn faultstring_escapes_embedded_markup() {
        let body = fault_body(SoapError::Soap(SoapProtocolError::IllegalSeekTarget(
            "<script>".to_string(),
        )))
        .await;
        assert!(!body.contains("<script>"));
        assert!(body.contains("&lt;script&gt;"));
    }
}
