use std::str::Utf8Error;

use crate::services::error::{ServiceError, TransportError};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use quick_xml::DeError;
use thiserror::Error;

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

impl IntoResponse for SoapError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
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

        // Create a SOAP Fault response
        let soap_fault = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
    <s:Body>
        <s:Fault>
            <faultcode>s:Client</faultcode>
            <faultstring>{}</faultstring>
        </s:Fault>
    </s:Body>
</s:Envelope>"#,
            error_message
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
}
