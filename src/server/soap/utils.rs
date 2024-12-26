use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::types::soap::SoapEnvelope;
use crate::server::soap::types::soap_response::{
    SoapResponse, SoapResponseBody, SoapResponseEnvelope,
};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::debug;

pub fn extract_soap_action(headers: &HeaderMap) -> Result<String, SoapError> {
    let raw_action = headers
        .get("SOAPAction")
        .or_else(|| headers.get("SOAPACTION"))
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            SoapError::Soap(SoapProtocolError::InvalidAction(
                "Missing SOAPAction header".into(),
            ))
        })?;

    // Remove quotes if present
    let action = raw_action.trim_matches('"');
    debug!("Received SOAP action header: {}", action);
    Ok(action.to_string())
}
pub fn parse_soap_request<T>(body: &str) -> Result<SoapEnvelope<T>, SoapError>
where
    T: DeserializeOwned,
{
    tracing::debug!("Attempting to parse SOAP request: {}", body);
    quick_xml::de::from_str::<SoapEnvelope<T>>(body).map_err(|e| {
        SoapError::Soap(SoapProtocolError::ParseError(format!(
            "Failed to parse SOAP request: {}",
            e
        )))
    })
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
