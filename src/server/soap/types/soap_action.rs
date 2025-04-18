use super::av_transport_request::{
    GetMediaInfoRequest, GetTransportInfoRequest, SetAVTransportURIRequest,
};
use crate::server::soap::error::SoapError;
use crate::server::soap::parser::soap_action;
use async_trait::async_trait;
use axum::{extract::FromRequestParts, http};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SoapEnvelope {
    #[serde(rename = "@xmlns:s")]
    _xmlns: Option<String>,

    #[serde(rename = "@s:encodingStyle")]
    _encoding_style: Option<String>,

    #[serde(rename = "Body")]
    pub body: SoapBody,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum SoapBody {
    #[serde(rename = "u:SetAVTransportURI")]
    SetAVTransportURI(SetAVTransportURIRequest),
    GetMediaInfo(GetMediaInfoRequest),
    GetTransportInfo(GetTransportInfoRequest),
}
#[derive(Debug, Deserialize)]
pub struct NamespacedAction<T> {
    #[serde(rename = "@xmlns:u")]
    _xmlns: Option<String>,

    #[serde(flatten)]
    pub inner: T,
}

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
