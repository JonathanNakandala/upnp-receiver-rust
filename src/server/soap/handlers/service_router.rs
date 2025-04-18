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
    
        // Get just the action name without namespace
        let action_name = soap_action
        .split('#')
        .last()
        .unwrap_or(&soap_action);
    
        // Parse the envelope generically first
        let envelope: SoapEnvelope = parse_soap_request(&body)?;
        
        // Debug the content structure
        debug!("Body content: {:?}", envelope.body.content);
        
        // Look for the action in the content (without the namespace prefix)
        let action_content = envelope.body.content.get(action_name)
            .ok_or_else(|| SoapError::Soap(SoapProtocolError::MissingField("Action content")))?;
        
        // Parse the action content based on the action name
        let action: S::Action = S::parse_action(action_name, action_content)
            .map_err(|e| SoapError::Soap(SoapProtocolError::ParseError(format!("Failed to parse action: {}", e))))?;
        
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
