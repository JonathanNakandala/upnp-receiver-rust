use crate::server::soap::error::SoapError;
use crate::server::soap::parser::soap_action;
use async_trait::async_trait;
use axum::{extract::FromRequestParts, http};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSoapAction {
    pub service: UpnpService,
    pub version: UpnpServiceVersion,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpnpService {
    AVTransport,
    ConnectionManager,
    RenderingControl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpnpServiceVersion {
    V1,
}

#[async_trait]
impl<S> FromRequestParts<S> for ParsedSoapAction
where
    S: Send + Sync,
{
    type Rejection = SoapError;

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        soap_action::parse_soap_action(&parts.headers)
    }
}
