use crate::services::error::{ServiceError, TransportError};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SoapError {
    #[error("SOAP error: {0}")]
    Soap(#[from] SoapProtocolError),

    #[error("Service error: {0}")]
    Service(#[from] ServiceError),
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

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Serialisation Error request: {0}")]
    SerializationError(String),
    #[error("Missing Fieldt: {0}")]
    MissingField(&'static str),
}
