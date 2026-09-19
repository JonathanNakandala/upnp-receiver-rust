use crate::server::client_logger::{ClientLogEntry, ClientLogger};
use crate::server::eventing::{
    handle_av_transport_events, handle_connection_manager_events, handle_rendering_control_events,
};
use crate::server::soap::handlers::UPnPServiceRouter;
use crate::server::templates::{
    MediaRendererAvTransportTemplate, MediaRendererConnectionManagerTemplate,
    MediaRendererDeviceTemplate, MediaRendererRenderingControlTemplate,
};
use crate::services::MediaRenderer;
use crate::services::ServiceError;
use askama::Template;
use axum::{
    body::{to_bytes, Body},
    extract::{connect_info::ConnectInfo, State},
    http::{header, Method, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
    routing::{any, get},
    Router,
};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use tower_http::trace::{self, TraceLayer};
use tracing::{debug, error, info, instrument, trace};

/// Requests/responses larger than this are truncated before being written to
/// the client log — SOAP control bodies are always small, this just guards
/// against something unexpected (e.g. a malformed or huge POST) filling disk.
const MAX_LOGGED_BODY_BYTES: usize = 64 * 1024;
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
    client_logger: Option<Arc<ClientLogger>>,
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
        Self {
            renderer,
            client_logger: None,
        }
    }

    /// Enables per-client SOAP request/response logging (see `--log-clients`),
    /// including outbound GENA NOTIFY deliveries, filed under the same
    /// per-client directory as the SUBSCRIBE that created each subscription.
    pub fn with_client_logger(mut self, logger: Option<Arc<ClientLogger>>) -> Self {
        if let Some(logger) = &logger {
            self.renderer
                .av_transport_events
                .set_client_logger(Arc::clone(logger));
            self.renderer
                .rendering_control_events
                .set_client_logger(Arc::clone(logger));
        }
        self.client_logger = logger;
        self
    }

    pub fn ip_addr(&self) -> IpAddr {
        self.renderer.ip_addr()
    }

    pub fn port(&self) -> u16 {
        self.renderer.port()
    }

    pub fn router(&self) -> Router {
        let mut router = Router::new()
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
            // GENA eventing routes (SUBSCRIBE/UNSUBSCRIBE aren't GET/POST,
            // so these are matched with `any` and dispatch on the method
            // themselves)
            .route(
                "/services/AVTransport/events",
                any(handle_av_transport_events),
            )
            .route(
                "/services/RenderingControl/events",
                any(handle_rendering_control_events),
            )
            .route(
                "/services/ConnectionManager/events",
                any(handle_connection_manager_events),
            )
            // SOAP control routes
            .merge(UPnPServiceRouter::new(Arc::clone(&self.renderer)).router());

        if let Some(logger) = &self.client_logger {
            // Wraps the whole router (routed and unmatched paths alike), so a
            // POST to a service we haven't implemented yet still gets logged.
            router = router.layer(middleware::from_fn_with_state(
                Arc::clone(logger),
                log_client_request,
            ));
        }

        router
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(request_span)
                    .on_request(trace::DefaultOnRequest::new().level(tracing::Level::DEBUG))
                    .on_response(trace::DefaultOnResponse::new().level(tracing::Level::DEBUG)),
            )
            .with_state(Arc::clone(&self.renderer))
    }

    #[instrument(skip(self))]
    pub async fn run(self, bind_addr: String) -> Result<(), ServiceError> {
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

        info!(address = %addr, "Starting HTTP server");
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
        let router = self.router();
        match axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                error!(error = %e, "HTTP server encountered an error");
                Err(ServiceError::IoError(e.to_string()))
            }
        }
    }
}
/// Headers worth pulling out for GENA (SUBSCRIBE/UNSUBSCRIBE) requests —
/// everything interesting on those lives in headers, not the (usually empty)
/// body.
const GENA_HEADERS: &[&str] = &["CALLBACK", "NT", "SID", "TIMEOUT"];

/// Metadata pulled off a request before its body is consumed, so it can be
/// reattached to the log entry once the response comes back.
struct RequestMeta {
    remote_addr: String,
    user_agent: Option<String>,
    method: String,
    path: String,
    soap_action: Option<String>,
    gena_headers: Vec<(String, String)>,
}

/// Reads a body to completion for logging, then rebuilds an equivalent
/// request/response so the rest of the stack sees the same bytes it would
/// have without logging enabled. On a read failure, forwards an empty body
/// rather than dropping the request/response entirely.
async fn buffer_body(body: Body) -> (String, Body) {
    match to_bytes(body, MAX_LOGGED_BODY_BYTES).await {
        Ok(bytes) => (
            String::from_utf8_lossy(&bytes).to_string(),
            Body::from(bytes),
        ),
        Err(e) => {
            error!(error = %e, "Failed to buffer body for client logging");
            (format!("<failed to read body: {}>", e), Body::empty())
        }
    }
}

/// Captures POSTs (SOAP calls) and GENA SUBSCRIBE/UNSUBSCRIBE requests to any
/// path (matched or not) for `--log-clients`: buffers the request/response
/// bodies, forwards them through unchanged, and hands the captured pair to
/// the `ClientLogger`.
async fn log_client_request(
    State(logger): State<Arc<ClientLogger>>,
    req: axum::http::Request<Body>,
    next: Next,
) -> axum::response::Response {
    let method = req.method().as_str();
    if method != Method::POST.as_str() && method != "SUBSCRIBE" && method != "UNSUBSCRIBE" {
        return next.run(req).await;
    }

    let meta = RequestMeta {
        remote_addr: req
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        method: req.method().to_string(),
        path: req.uri().path().to_string(),
        user_agent: req
            .headers()
            .get(header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string),
        soap_action: req
            .headers()
            .get("SOAPAction")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string),
        gena_headers: GENA_HEADERS
            .iter()
            .filter_map(|name| {
                req.headers()
                    .get(*name)
                    .and_then(|v| v.to_str().ok())
                    .map(|v| (name.to_string(), v.to_string()))
            })
            .collect(),
    };

    let (parts, body) = req.into_parts();
    let (request_body, body) = buffer_body(body).await;
    let req = axum::http::Request::from_parts(parts, body);

    let response = next.run(req).await;

    let (resp_parts, resp_body) = response.into_parts();
    let response_status = resp_parts.status.as_u16();
    let (response_body, resp_body) = buffer_body(resp_body).await;

    logger
        .log(&ClientLogEntry {
            remote_addr: meta.remote_addr,
            user_agent: meta.user_agent,
            method: meta.method,
            path: meta.path,
            soap_action: meta.soap_action,
            gena_headers: meta.gena_headers,
            request_body,
            response_status,
            response_body,
        })
        .await;

    axum::http::Response::from_parts(resp_parts, resp_body)
}

/// Builds the tracing span each request runs in. Carries just the client
/// address — every event logged anywhere during the request (including deep
/// inside an action handler) inherits this span's fields, so keeping it to
/// the one thing that's genuinely useful everywhere (who's asking) avoids
/// every log line getting method/uri/headers noise repeated onto it.
fn request_span(req: &axum::http::Request<Body>) -> tracing::Span {
    let remote_addr = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.to_string());

    tracing::info_span!("request", client = remote_addr.as_deref().unwrap_or("-"))
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
