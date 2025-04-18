use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::types::soap_action::SoapEnvelope;

use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use serde::Serialize;

pub fn parse_soap_request(body: &str) -> Result<SoapEnvelope, SoapError> {
    tracing::debug!("Attempting to parse SOAP request: {}", body);

    // Log the raw body for debugging
    tracing::debug!("Raw SOAP body:\n{}", body);

    match quick_xml::de::from_str::<SoapEnvelope>(body) {
        Ok(envelope) => {
            tracing::debug!("Successfully parsed SOAP envelope");
            Ok(envelope)
        }
        Err(e) => {
            // More detailed error logging
            tracing::error!("XML deserialization error: {}", e);
            tracing::error!("Error details: {:?}", e);

            // Return appropriate error
            Err(SoapError::Soap(SoapProtocolError::ParseError(format!(
                "Failed to parse SOAP request: {}",
                e
            ))))
        }
    }
}

pub fn create_soap_response<T: Serialize>(
    namespace: &str,
    response: T,
) -> Result<impl IntoResponse, SoapError> {
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
        <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" 
                   s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"
                   xmlns:u="{}">
            <s:Body>
                {}"#,
        namespace,
        quick_xml::se::to_string(&response)
            .map_err(|e| SoapError::Soap(SoapProtocolError::SerializationError(e.to_string())))?
    );

    Ok((StatusCode::OK, [("Content-Type", "text/xml")], xml))
}

pub fn create_empty_soap_response(
    action_name: &str,
    service_type: &str,
) -> Result<impl IntoResponse, SoapError> {
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:{}Response xmlns:u="{}"></u:{}Response>
                </s:Body>
            </s:Envelope>"#,
        action_name, service_type, action_name
    );

    Ok((StatusCode::OK, [("Content-Type", "text/xml")], xml))
}
