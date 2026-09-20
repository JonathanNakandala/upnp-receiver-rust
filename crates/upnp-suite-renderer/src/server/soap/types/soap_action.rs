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

/// The numeric version segment of a `SOAPAction` header (e.g. `3` in
/// `...:ConnectionManager:3#GetFeatureList`). Not gated against what a
/// service actually implements — UPnP backward compatibility requires a
/// device to keep answering older-versioned requests even after bumping a
/// service's declared version (see the MediaRenderer/MediaServer device
/// specs, "shall respond to all actions defined by ... or lower"), and
/// nothing downstream of parsing currently branches on this value, so any
/// well-formed version number is accepted rather than only the one currently
/// advertised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpnpServiceVersion(pub u32);

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
