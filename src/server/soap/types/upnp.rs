use crate::server::soap::error::SoapError;
use crate::services::MediaRenderer;
use async_trait::async_trait;
use axum::body::Body;
use axum::http::Response;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::sync::Arc;

#[async_trait]
pub trait UPnPService: Send + Sync + 'static {
    type Action: UPnPAction;

    fn service_type() -> &'static str;
    fn service_id() -> &'static str;
    fn service_name() -> &'static str;

    async fn handle_action(
        action: Self::Action,
        renderer: Arc<MediaRenderer>,
    ) -> Result<Response<Body>, SoapError>;
}

/// Represents a UPnP action within a service
pub trait UPnPAction: DeserializeOwned + std::fmt::Debug {
    type Response: Serialize;

    fn action_name(&self) -> &'static str;
    fn namespace(&self) -> &'static str;
    async fn handle(&self, renderer: Arc<MediaRenderer>) -> Result<Self::Response, SoapError>;
}
