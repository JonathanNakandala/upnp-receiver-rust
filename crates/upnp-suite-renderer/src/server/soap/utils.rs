use crate::server::soap::error::{SoapError, SoapProtocolError};

use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;

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
                {}
            </s:Body>
        </s:Envelope>"#,
        namespace,
        quick_xml::se::to_string(&response)
            .map_err(|e| SoapError::Soap(SoapProtocolError::SerializationError(e.to_string())))?
    );

    Ok((StatusCode::OK, [("Content-Type", "text/xml")], xml))
}
