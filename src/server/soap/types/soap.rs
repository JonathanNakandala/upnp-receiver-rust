use crate::server::soap::error::{SoapError, SoapProtocolError};
use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SoapEnvelope<T> {
    #[serde(rename = "@xmlns:s")]
    _xmlns: Option<String>,

    #[serde(rename = "@s:encodingStyle")]
    _encoding_style: Option<String>,

    #[serde(rename = "Body")]
    pub body: SoapBody<T>,
}

#[derive(Debug, Deserialize)]
pub struct SoapBody<T> {
    #[serde(flatten)]
    pub action: NamespacedAction<T>,
}

#[derive(Debug, Deserialize)]
pub struct NamespacedAction<T> {
    #[serde(rename = "@xmlns:u")]
    _xmlns: Option<String>,

    #[serde(flatten)]
    pub inner: T,
}

pub struct ParsedSoapAction<T> {
    pub action: T,
}

pub fn parse_soap_action(headers: &HeaderMap) -> Result<String, SoapError> {
    // Extract and validate the `SOAPAction` header
    let soap_action = headers
        .get("SOAPAction")
        .or_else(|| headers.get("SOAPACTION"))
        .and_then(|v| v.to_str().ok())
        .ok_or({ SoapError::Soap(SoapProtocolError::MissingField("SOAPAction header missing")) })?;

    // Clean up the SOAPAction header value and extract the action name
    soap_action
        .trim_matches('"')
        .split('#')
        .last()
        .ok_or_else(|| {
            SoapError::Soap(SoapProtocolError::InvalidAction(
                "Invalid SOAPAction format".into(),
            ))
        })
        .map(|s| s.to_string())
}

#[async_trait]
impl<T, S> FromRequestParts<S> for ParsedSoapAction<T>
where
    T: TryFrom<String, Error = SoapError> + Send + Sync,
    S: Send + Sync,
{
    type Rejection = SoapError;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let action = parse_soap_action(&parts.headers)?;
        T::try_from(action).map(|action| ParsedSoapAction { action })
    }
}
