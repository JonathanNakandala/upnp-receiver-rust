use crate::server::soap::error::SoapError;
use crate::server::soap::utils::create_soap_response;
use crate::services::MediaRenderer;
use async_trait::async_trait;
use axum::body::Body;
use axum::http::Response;
use axum::response::IntoResponse;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::sync::Arc;

use super::soap_action::ParsedSoapAction;

#[async_trait]
pub trait UPnPService: Send + Sync + 'static {
    type Action: UPnPAction;

    fn service_type() -> &'static str;
    fn service_id() -> &'static str;
    fn service_name() -> &'static str;

    /// Runs the action and serializes its response as a SOAP HTTP response.
    ///
    /// Per-action logging lives in each `UPnPAction::handle` implementation
    /// instead of here, so log lines keep the right component name (e.g.
    /// `AVTransport`, `RenderingControl`) and can pick which fields matter
    /// for that specific action.
    async fn handle_action(
        action: Self::Action,
        renderer: Arc<MediaRenderer>,
    ) -> Result<Response<Body>, SoapError> {
        let response = action.handle(renderer).await?;
        create_soap_response(Self::service_type(), response).map(IntoResponse::into_response)
    }

    fn parse_action(
        action_name: &ParsedSoapAction,
        content: &String,
    ) -> Result<Self::Action, SoapError>;
}

/// Represents a UPnP action within a service
pub trait UPnPAction: DeserializeOwned + std::fmt::Debug + Send {
    type Response: Serialize + std::fmt::Debug;

    fn action_name(&self) -> &'static str;
    fn namespace(&self) -> &'static str;
    fn handle(
        &self,
        renderer: Arc<MediaRenderer>,
    ) -> impl std::future::Future<Output = Result<Self::Response, SoapError>> + Send;
}
