use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::handlers::av_transport::AVTransportService;
use crate::server::soap::parser::parse_soap_action;
use crate::server::soap::types::UPnPService;
use crate::services::MediaRenderer;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Response};
use axum::routing::post;
use axum::Router;
use std::sync::Arc;
use tracing::{debug, instrument, trace};

pub struct UPnPServiceRouter {
    renderer: Arc<MediaRenderer>,
}

impl UPnPServiceRouter {
    pub fn new(renderer: Arc<MediaRenderer>) -> Self {
        Self { renderer }
    }
    pub fn router(self) -> Router<Arc<MediaRenderer>> {
        trace!("Creating router");
        Router::new().route(
            "/services/AVTransport/control",
            post(Self::handle_av_transport),
        )
    }

    #[instrument(skip(renderer, body))]
    async fn handle_service<S: UPnPService>(
        State(renderer): State<Arc<MediaRenderer>>,
        headers: HeaderMap,
        body: String,
    ) -> Result<Response<Body>, SoapError> {
        // Extract SOAP action first
        debug!("Extracting Soap Action");
        let soap_action = parse_soap_action(&headers)?;

        // Parse the action content based on the action name
        let action: S::Action = S::parse_action(&soap_action, &body).map_err(|e| {
            SoapError::Soap(SoapProtocolError::ParseError(format!(
                "Failed to parse action: {}",
                e
            )))
        })?;

        // Handle the action
        S::handle_action(action, renderer).await
    }

    async fn handle_av_transport(
        State(renderer): State<Arc<MediaRenderer>>,
        headers: HeaderMap,
        body: String,
    ) -> Result<Response<Body>, SoapError> {
        trace!(body = body, headers = ?headers, "AV Transport Handler Activated");
        Self::handle_service::<AVTransportService>(State(renderer), headers, body).await
    }
}
