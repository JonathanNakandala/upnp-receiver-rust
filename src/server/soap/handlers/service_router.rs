use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::handlers::av_transport::AVTransportService;
use crate::server::soap::types::soap::SoapEnvelope;
use crate::server::soap::types::UPnPService;
use crate::server::soap::{extract_soap_action, parse_soap_request};
use crate::services::MediaRenderer;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Response};
use axum::routing::post;
use axum::Router;
use std::sync::Arc;
use tracing::{debug, info, instrument, trace};

pub struct UPnPServiceRouter {
    renderer: Arc<MediaRenderer>,
}
use axum::extract::FromRequest;

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
        let soap_action = extract_soap_action(&headers)?;

        // Validate service type
        if !soap_action.contains(S::service_type()) {
            info!(soap_action=soap_action, service_type=S::service_type(), "Could not map service type");
            return Err(SoapError::Soap(SoapProtocolError::InvalidAction(
                "Invalid service type".into(),
            )));
        }

        let envelope: SoapEnvelope<S::Action> = parse_soap_request(&body)?;
        S::handle_action(envelope.body.action.inner, renderer).await
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
