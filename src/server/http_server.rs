use crate::server::soap::handlers::UPnPServiceRouter;
use crate::server::templates::{
    MediaRendererAvTransportTemplate, MediaRendererConnectionManagerTemplate,
    MediaRendererDeviceTemplate, MediaRendererRenderingControlTemplate,
};
use crate::services::MediaRenderer;
use crate::services::ServiceError;
use askama::Template;
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, error, info, instrument, trace};
use tower_http::trace::{self, TraceLayer};
#[derive(Debug)]
pub struct XmlResponse(String);

impl IntoResponse for XmlResponse {
    fn into_response(self) -> axum::response::Response {
        trace!("Converting XML response to HTTP response");
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/xml")],
            self.0,
        )
            .into_response()
    }
}

pub struct HttpServer {
    renderer: Arc<MediaRenderer>,
}

#[derive(Debug)]
enum TemplateError {
    RenderError(askama::Error),
}

impl From<askama::Error> for TemplateError {
    fn from(err: askama::Error) -> Self {
        error!(error = %err, "Template rendering failed");
        Self::RenderError(err)
    }
}

impl IntoResponse for TemplateError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::RenderError(e) => {
                error!(
                    error = %e,
                    "Failed to render template response"
                );
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
        }
    }
}

impl HttpServer {
    pub fn new(renderer: Arc<MediaRenderer>) -> Self {
        debug!(
            ip = %renderer.ip_addr(),
            port = %renderer.port(),
            name = %renderer.friendly_name(),
            "Creating new HTTP server instance"
        );
        Self { renderer }
    }

    pub fn ip_addr(&self) -> IpAddr {
        self.renderer.ip_addr()
    }

    pub fn port(&self) -> u16 {
        self.renderer.port()
    }

    pub fn router(&self) -> Router {
        Router::new()
            // Template routes
            .route("/description.xml", get(handle_device_description))
            .route(
                "/services/AVTransport/description.xml",
                get(handle_av_transport_description),
            )
            .route(
                "/services/RenderingControl/description.xml",
                get(handle_rendering_control_description),
            )
            .route(
                "/services/ConnectionManager/description.xml",
                get(handle_connection_manager_description),
            )
            // SOAP control routes
            .merge(UPnPServiceRouter::new(Arc::clone(&self.renderer)).router())
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(trace::DefaultMakeSpan::new().level(tracing::Level::INFO))
                    .on_request(trace::DefaultOnRequest::new().level(tracing::Level::INFO))
                    .on_response(trace::DefaultOnResponse::new().level(tracing::Level::INFO))
            )
            .with_state(Arc::clone(&self.renderer))
    }

    #[instrument(skip(self))]
    pub async fn run(self, bind_addr: String) -> Result<(), ServiceError> {
        info!("Starting HTTP server on {}", bind_addr);

        debug!("Parsing socket address");
        let addr = match SocketAddr::from_str(&bind_addr) {
            Ok(addr) => addr,
            Err(e) => {
                error!(
                    error = %e,
                    address = %bind_addr,
                    "Failed to parse socket address"
                );
                return Err(ServiceError::InvalidValue(format!(
                    "Invalid address: {}",
                    e
                )));
            }
        };

        info!(address = %addr, "Binding HTTP server");
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                error!(
                    error = %e,
                    address = %addr,
                    "Failed to bind TCP listener"
                );
                return Err(e.into());
            }
        };

        info!("HTTP server started successfully");
        match axum::serve(listener, self.router()).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!(error = %e, "HTTP server encountered an error");
                Err(ServiceError::IoError(e.to_string()))
            }
        }
    }
}
// Template handlers
#[instrument(skip(renderer))]
async fn handle_device_description(
    State(renderer): State<Arc<MediaRenderer>>,
) -> Result<XmlResponse, TemplateError> {
    debug!(
        device_name = %renderer.friendly_name(),
        "Handling device description request"
    );

    let template = MediaRendererDeviceTemplate {
        friendly_name: renderer.friendly_name().to_string(),
        manufacturer: renderer.manufacturer().to_string(),
        manufacturer_url: renderer.manufacturer_url().to_string(),
        model_description: renderer.model_description().to_string(),
        model_name: renderer.model_name().to_string(),
        model_number: renderer.model_number().to_string(),
        model_url: renderer.model_url().to_string(),
        serial_number: renderer.serial_number().to_string(),
        uuid: renderer.unique_id().to_string(),
    };

    trace!("Rendering device description template");
    let response = template.render()?;
    debug!("Device description request completed successfully");
    Ok(XmlResponse(response))
}

#[instrument]
async fn handle_av_transport_description() -> Result<XmlResponse, TemplateError> {
    debug!("Handling AVTransport description request");
    let template = MediaRendererAvTransportTemplate;
    trace!("Rendering AVTransport description template");
    let response = template.render()?;
    debug!("AVTransport description request completed successfully");
    Ok(XmlResponse(response))
}

#[instrument]
async fn handle_rendering_control_description() -> Result<XmlResponse, TemplateError> {
    debug!("Handling RenderingControl description request");
    let template = MediaRendererRenderingControlTemplate;
    trace!("Rendering RenderingControl description template");
    let response = template.render()?;
    debug!("RenderingControl description request completed successfully");
    Ok(XmlResponse(response))
}

#[instrument]
async fn handle_connection_manager_description() -> Result<XmlResponse, TemplateError> {
    debug!("Handling ConnectionManager description request");
    let template = MediaRendererConnectionManagerTemplate;
    trace!("Rendering ConnectionManager description template");
    let response = template.render()?;
    debug!("ConnectionManager description request completed successfully");
    Ok(XmlResponse(response))
}
