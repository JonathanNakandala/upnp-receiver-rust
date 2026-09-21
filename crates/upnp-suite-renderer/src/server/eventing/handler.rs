//! The GENA `SUBSCRIBE`/`UNSUBSCRIBE` endpoint for a single service's
//! `eventSubURL`. These aren't GET/POST, so they're matched with axum's
//! `any()` and dispatched here by inspecting the raw method instead of
//! routing on it.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, Method, Response, StatusCode};
use axum::response::IntoResponse;
use tracing::info;

use super::last_change;
use super::last_change::AVTransportEventState;
use super::manager::{EventManager, SubscribeError};
use crate::services::instance::InstanceId;
use crate::services::MediaRenderer;

pub async fn handle_av_transport_events(
    State(renderer): State<Arc<MediaRenderer>>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    method: Method,
    headers: HeaderMap,
) -> Response<Body> {
    let manager = Arc::clone(&renderer.av_transport_events);
    let initial_body = {
        let renderer = Arc::clone(&renderer);
        async move {
            // GENA subscriptions are per-service, not per-instance (SUBSCRIBE
            // carries no InstanceID), so the mandatory initial event always
            // reports the default instance — the only one a plain
            // subscription without PrepareForConnection can ever mean.
            let transport = renderer
                .av_transport
                .get(InstanceId::DEFAULT)
                .expect("default AVTransport instance is never removed");
            let event =
                AVTransportEventState::snapshot(InstanceId::DEFAULT, transport.as_ref()).await;
            last_change::av_transport_last_change(&event)
        }
    };

    dispatch(&manager, &method, &headers, remote_addr, initial_body).await
}

pub async fn handle_rendering_control_events(
    State(renderer): State<Arc<MediaRenderer>>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    method: Method,
    headers: HeaderMap,
) -> Response<Body> {
    let manager = Arc::clone(&renderer.rendering_control_events);
    let initial_body = {
        let renderer = Arc::clone(&renderer);
        async move {
            let control = renderer
                .rendering_control
                .get(InstanceId::DEFAULT)
                .expect("default RenderingControl instance is never removed");
            let volume = control.get_volume().await.unwrap_or(0);
            let muted = control.get_mute().await.unwrap_or(false);
            let brightness = control.get_brightness().await.unwrap_or(50);
            let contrast = control.get_contrast().await.unwrap_or(50);
            last_change::rendering_control_last_change(
                InstanceId::DEFAULT,
                volume,
                muted,
                brightness,
                contrast,
            )
        }
    };

    dispatch(&manager, &method, &headers, remote_addr, initial_body).await
}

pub async fn handle_connection_manager_events(
    State(renderer): State<Arc<MediaRenderer>>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    method: Method,
    headers: HeaderMap,
) -> Response<Body> {
    let manager = Arc::clone(&renderer.connection_manager_events);
    let initial_body = {
        let renderer = Arc::clone(&renderer);
        async move {
            let ids = renderer
                .connection_manager
                .get_current_connection_ids()
                .await
                .unwrap_or_default();
            let ids = ids.iter().map(i32::to_string).collect::<Vec<_>>().join(",");
            last_change::connection_manager_current_ids_event(&ids)
        }
    };

    dispatch(&manager, &method, &headers, remote_addr, initial_body).await
}

/// Shared SUBSCRIBE/UNSUBSCRIBE logic, generic over which service's
/// [`EventManager`] and initial-state body it's operating on.
async fn dispatch(
    manager: &Arc<EventManager>,
    method: &Method,
    headers: &HeaderMap,
    remote_addr: SocketAddr,
    initial_body: impl std::future::Future<Output = String>,
) -> Response<Body> {
    let sid = headers
        .get("SID")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    match (method.as_str(), sid) {
        ("SUBSCRIBE", None) => {
            subscribe_new(manager, headers, remote_addr, initial_body.await).await
        }
        ("SUBSCRIBE", Some(sid)) => renew(manager, headers, &sid),
        ("UNSUBSCRIBE", Some(sid)) => unsubscribe(manager, &sid),
        ("UNSUBSCRIBE", None) => {
            (StatusCode::BAD_REQUEST, "UNSUBSCRIBE requires a SID header").into_response()
        }
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}

async fn subscribe_new(
    manager: &Arc<EventManager>,
    headers: &HeaderMap,
    remote_addr: SocketAddr,
    initial_body: String,
) -> Response<Body> {
    let callback = headers.get("CALLBACK").and_then(|v| v.to_str().ok());
    let timeout_header = headers.get("TIMEOUT").and_then(|v| v.to_str().ok());
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    match manager.subscribe(
        callback,
        timeout_header,
        remote_addr.to_string(),
        user_agent,
    ) {
        Ok(sub) => {
            info!(sid = %sub.sid, callbacks = ?sub.callback_urls, "New GENA subscription");
            manager.notify_initial(&sub, initial_body);
            Response::builder()
                .status(StatusCode::OK)
                .header("SID", &sub.sid)
                .header("TIMEOUT", format!("Second-{}", sub.timeout.as_secs()))
                .header("CONTENT-LENGTH", "0")
                .body(Body::empty())
                .unwrap()
        }
        Err(SubscribeError::MissingCallback) => precondition_failed("missing CALLBACK header"),
        Err(e) => precondition_failed(&e.to_string()),
    }
}

fn renew(manager: &Arc<EventManager>, headers: &HeaderMap, sid: &str) -> Response<Body> {
    let timeout_header = headers.get("TIMEOUT").and_then(|v| v.to_str().ok());

    match manager.renew(sid, timeout_header) {
        Ok(timeout) => {
            info!(sid, "Renewed GENA subscription");
            Response::builder()
                .status(StatusCode::OK)
                .header("SID", sid)
                .header("TIMEOUT", format!("Second-{}", timeout.as_secs()))
                .header("CONTENT-LENGTH", "0")
                .body(Body::empty())
                .unwrap()
        }
        Err(e) => precondition_failed(&e.to_string()),
    }
}

fn unsubscribe(manager: &Arc<EventManager>, sid: &str) -> Response<Body> {
    match manager.unsubscribe(sid) {
        Ok(()) => {
            info!(sid, "Cancelled GENA subscription");
            StatusCode::OK.into_response()
        }
        Err(e) => precondition_failed(&e.to_string()),
    }
}

fn precondition_failed(message: &str) -> Response<Body> {
    (StatusCode::PRECONDITION_FAILED, message.to_string()).into_response()
}
