use crate::server::soap::error::SoapError;
use crate::services::MediaRenderer;
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::sync::Arc;

#[async_trait]
pub trait SoapAction {
    type Request: DeserializeOwned;
    type Response: Serialize;
    const ACTION_NAME: &'static str;
    const NAMESPACE: &'static str = "urn:schemas-upnp-org:service:AVTransport:1";

    async fn execute(
        request: Self::Request,
        renderer: Arc<MediaRenderer>,
    ) -> Result<Self::Response, SoapError>;
}
