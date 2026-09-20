//! GENA subscribe/NOTIFY for a `RendererClient` - each `subscribe_*` call
//! tries a real GENA SUBSCRIBE first (push updates, the same mechanism a
//! real control point uses), and falls back to polling the matching SOAP
//! action on an interval if that fails (service doesn't advertise
//! eventing, or the SUBSCRIBE itself errors) - either way the caller gets
//! the same [`EventStream`] pushing snapshots into a `watch` channel.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::any;
use axum::Router;
use reqwest::{Method, Url};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::av_transport::TransportInfo;
use crate::device::RendererClient;
use crate::error::ClientError;
use crate::last_change::{
    parse_av_transport_last_change, parse_rendering_control_last_change, AVTransportSnapshot,
    RenderingControlSnapshot,
};
use crate::soap::invoke_action;

const DEFAULT_INSTANCE_ID: &str = "0";
const MASTER_CHANNEL: &str = "Master";
/// How often to poll when GENA eventing isn't available - matches what the
/// video window's menu polled at before it had a real client to subscribe
/// through.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// A live feed of state snapshots for one service, backed by GENA NOTIFYs
/// when the SUBSCRIBE succeeded, or by polling otherwise - the caller can't
/// tell which from the API, only (optionally) from logs.
pub struct EventStream<T> {
    rx: watch::Receiver<T>,
    task: Option<JoinHandle<()>>,
    cleanup: Option<SubscriptionCleanup>,
}

struct SubscriptionCleanup {
    http: reqwest::Client,
    event_sub_url: Url,
    sid: String,
    listener: Arc<NotifyListener>,
}

impl<T: Clone> EventStream<T> {
    /// A receiver that yields every subsequent snapshot - clone freely,
    /// each clone tracks its own read position independently.
    pub fn changes(&self) -> watch::Receiver<T> {
        self.rx.clone()
    }

    /// The most recently observed snapshot.
    pub fn latest(&self) -> T {
        self.rx.borrow().clone()
    }
}

impl<T> Drop for EventStream<T> {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
        if let Some(cleanup) = self.cleanup.take() {
            cleanup.listener.unregister(&cleanup.sid);
            // Best-effort: only fires if a tokio runtime is still around to
            // run it (e.g. not during process exit), same as any other
            // fire-and-forget cleanup in this codebase.
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    send_unsubscribe(&cleanup.http, &cleanup.event_sub_url, &cleanup.sid).await;
                });
            }
        }
    }
}

impl RendererClient {
    pub async fn subscribe_av_transport(&self) -> Result<EventStream<AVTransportSnapshot>, ClientError> {
        let endpoint = self
            .av_transport
            .as_ref()
            .ok_or(ClientError::ServiceNotFound("AVTransport"))?;
        let (tx, rx) = watch::channel(AVTransportSnapshot::default());

        if let Some(event_sub_url) = endpoint.event_sub_url.clone() {
            match self.try_subscribe(&event_sub_url).await {
                Ok(subscription) => {
                    let task = tokio::spawn(run_notify_loop(
                        subscription.http.clone(),
                        subscription.event_sub_url.clone(),
                        subscription.sid.clone(),
                        subscription.timeout,
                        subscription.notify_rx,
                        tx,
                        parse_av_transport_last_change,
                    ));
                    return Ok(EventStream {
                        rx,
                        task: Some(task),
                        cleanup: Some(SubscriptionCleanup {
                            http: subscription.http,
                            event_sub_url: subscription.event_sub_url,
                            sid: subscription.sid,
                            listener: subscription.listener,
                        }),
                    });
                }
                Err(e) => {
                    warn!(error = %e, "AVTransport SUBSCRIBE failed, falling back to polling");
                }
            }
        } else {
            info!("AVTransport service does not advertise eventing, falling back to polling");
        }

        let task = tokio::spawn(run_av_transport_poll_loop(
            self.http.clone(),
            endpoint.control_url.clone(),
            endpoint.service_type.clone(),
            tx,
        ));
        Ok(EventStream {
            rx,
            task: Some(task),
            cleanup: None,
        })
    }

    pub async fn subscribe_rendering_control(
        &self,
    ) -> Result<EventStream<RenderingControlSnapshot>, ClientError> {
        let endpoint = self
            .rendering_control
            .as_ref()
            .ok_or(ClientError::ServiceNotFound("RenderingControl"))?;
        let (tx, rx) = watch::channel(RenderingControlSnapshot::default());

        if let Some(event_sub_url) = endpoint.event_sub_url.clone() {
            match self.try_subscribe(&event_sub_url).await {
                Ok(subscription) => {
                    let task = tokio::spawn(run_notify_loop(
                        subscription.http.clone(),
                        subscription.event_sub_url.clone(),
                        subscription.sid.clone(),
                        subscription.timeout,
                        subscription.notify_rx,
                        tx,
                        parse_rendering_control_last_change,
                    ));
                    return Ok(EventStream {
                        rx,
                        task: Some(task),
                        cleanup: Some(SubscriptionCleanup {
                            http: subscription.http,
                            event_sub_url: subscription.event_sub_url,
                            sid: subscription.sid,
                            listener: subscription.listener,
                        }),
                    });
                }
                Err(e) => {
                    warn!(error = %e, "RenderingControl SUBSCRIBE failed, falling back to polling");
                }
            }
        } else {
            info!("RenderingControl service does not advertise eventing, falling back to polling");
        }

        let task = tokio::spawn(run_rendering_control_poll_loop(
            self.http.clone(),
            endpoint.control_url.clone(),
            endpoint.service_type.clone(),
            tx,
        ));
        Ok(EventStream {
            rx,
            task: Some(task),
            cleanup: None,
        })
    }

    async fn try_subscribe(&self, event_sub_url: &Url) -> Result<NewSubscription, ClientError> {
        let listener = self.notify_listener().await?;
        let raw = send_subscribe(&self.http, event_sub_url, &listener.callback_url()).await?;
        let notify_rx = listener.register(raw.sid.clone());
        info!(sid = %raw.sid, %event_sub_url, "Subscribed to GENA events");
        Ok(NewSubscription {
            http: self.http.clone(),
            event_sub_url: event_sub_url.clone(),
            sid: raw.sid,
            timeout: raw.timeout,
            notify_rx,
            listener,
        })
    }
}

struct NewSubscription {
    http: reqwest::Client,
    event_sub_url: Url,
    sid: String,
    timeout: Duration,
    notify_rx: mpsc::UnboundedReceiver<String>,
    listener: Arc<NotifyListener>,
}

/// Drives one subscription for its whole life: forwards every parsed NOTIFY
/// into `tx`, and renews before `timeout` expires. Exits (silently letting
/// the subscription lapse) if a renewal fails or the NOTIFY listener goes
/// away - the caller finds out only implicitly, via the channel going
/// stale, same as a real control point would if it stopped polling
/// `GetTransportInfo` as a backstop.
async fn run_notify_loop<T, F>(
    http: reqwest::Client,
    event_sub_url: Url,
    sid: String,
    timeout: Duration,
    mut notify_rx: mpsc::UnboundedReceiver<String>,
    tx: watch::Sender<T>,
    parse: F,
) where
    T: Send + 'static,
    F: Fn(&str) -> Result<T, ClientError> + Send + 'static,
{
    let mut renew_at = tokio::time::Instant::now() + renewal_margin(timeout);
    loop {
        tokio::select! {
            body = notify_rx.recv() => {
                match body {
                    Some(body) => match parse(&body) {
                        Ok(snapshot) => {
                            if tx.send(snapshot).is_err() {
                                break;
                            }
                        }
                        Err(e) => warn!(error = %e, sid, "failed to parse NOTIFY body"),
                    },
                    None => break,
                }
            }
            _ = tokio::time::sleep_until(renew_at) => {
                match send_renew(&http, &event_sub_url, &sid).await {
                    Ok(timeout) => {
                        renew_at = tokio::time::Instant::now() + renewal_margin(timeout);
                    }
                    Err(e) => {
                        warn!(error = %e, sid, "GENA renewal failed; subscription will lapse");
                        break;
                    }
                }
            }
        }
    }
}

/// Renews at 80% of the granted TIMEOUT, floored at a few seconds so even a
/// very short grant still leaves room to renew before it actually expires.
fn renewal_margin(timeout: Duration) -> Duration {
    timeout.mul_f64(0.8).max(Duration::from_secs(5))
}

async fn run_av_transport_poll_loop(
    http: reqwest::Client,
    control_url: Url,
    service_type: String,
    tx: watch::Sender<AVTransportSnapshot>,
) {
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    loop {
        interval.tick().await;
        if tx.is_closed() {
            break;
        }
        match invoke_action(
            &http,
            &control_url,
            &service_type,
            "GetTransportInfo",
            &[("InstanceID", DEFAULT_INSTANCE_ID)],
        )
        .await
        {
            Ok(fields) => {
                let info = TransportInfo {
                    current_transport_state: fields
                        .get("CurrentTransportState")
                        .cloned()
                        .unwrap_or_default(),
                    current_transport_status: fields
                        .get("CurrentTransportStatus")
                        .cloned()
                        .unwrap_or_default(),
                    current_speed: fields.get("CurrentSpeed").cloned().unwrap_or_default(),
                };
                // Polling only covers what `GetTransportInfo` reports
                // (state/status/speed) - URI/metadata/play mode stay at
                // whatever they last were (empty, if this renderer has no
                // eventing at all), the same gap the old fixed-interval
                // menu poller had before this client existed.
                tx.send_modify(|snapshot| {
                    snapshot.transport_state = info.current_transport_state;
                    snapshot.transport_status = info.current_transport_status;
                    snapshot.play_speed = info.current_speed;
                });
            }
            Err(e) => warn!(error = %e, "AVTransport poll failed"),
        }
    }
}

async fn run_rendering_control_poll_loop(
    http: reqwest::Client,
    control_url: Url,
    service_type: String,
    tx: watch::Sender<RenderingControlSnapshot>,
) {
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    loop {
        interval.tick().await;
        if tx.is_closed() {
            break;
        }

        let volume = invoke_action(
            &http,
            &control_url,
            &service_type,
            "GetVolume",
            &[("InstanceID", DEFAULT_INSTANCE_ID), ("Channel", MASTER_CHANNEL)],
        )
        .await
        .ok()
        .and_then(|f| f.get("CurrentVolume").and_then(|v| v.parse().ok()));

        let muted = invoke_action(
            &http,
            &control_url,
            &service_type,
            "GetMute",
            &[("InstanceID", DEFAULT_INSTANCE_ID), ("Channel", MASTER_CHANNEL)],
        )
        .await
        .ok()
        .map(|f| {
            f.get("CurrentMute")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false)
        });

        if volume.is_none() && muted.is_none() {
            warn!("RenderingControl poll failed");
            continue;
        }

        tx.send_modify(|snapshot| {
            if let Some(volume) = volume {
                snapshot.volume = volume;
            }
            if let Some(muted) = muted {
                snapshot.muted = muted;
            }
        });
    }
}

struct RawSubscription {
    sid: String,
    timeout: Duration,
}

async fn send_subscribe(
    http: &reqwest::Client,
    event_sub_url: &Url,
    callback_url: &str,
) -> Result<RawSubscription, ClientError> {
    let method = Method::from_bytes(b"SUBSCRIBE").expect("SUBSCRIBE is a valid HTTP method token");
    let response = http
        .request(method, event_sub_url.clone())
        .header("CALLBACK", format!("<{callback_url}>"))
        .header("NT", "upnp:event")
        .header("TIMEOUT", "Second-1800")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(ClientError::MalformedResponse(format!(
            "SUBSCRIBE failed with HTTP {}",
            response.status()
        )));
    }

    let sid = response
        .headers()
        .get("SID")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ClientError::MalformedResponse("SUBSCRIBE response missing SID".into()))?
        .to_string();
    let timeout = response
        .headers()
        .get("TIMEOUT")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_timeout_header)
        .unwrap_or(Duration::from_secs(1800));

    Ok(RawSubscription { sid, timeout })
}

async fn send_renew(
    http: &reqwest::Client,
    event_sub_url: &Url,
    sid: &str,
) -> Result<Duration, ClientError> {
    let method = Method::from_bytes(b"SUBSCRIBE").expect("SUBSCRIBE is a valid HTTP method token");
    let response = http
        .request(method, event_sub_url.clone())
        .header("SID", sid)
        .header("TIMEOUT", "Second-1800")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(ClientError::MalformedResponse(format!(
            "SUBSCRIBE renewal failed with HTTP {}",
            response.status()
        )));
    }

    Ok(response
        .headers()
        .get("TIMEOUT")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_timeout_header)
        .unwrap_or(Duration::from_secs(1800)))
}

/// Fire-and-forget - a failed UNSUBSCRIBE (device gone, network hiccup)
/// just leaves a subscription to expire on its own TIMEOUT, same as it
/// would for any control point that exits uncleanly.
async fn send_unsubscribe(http: &reqwest::Client, event_sub_url: &Url, sid: &str) {
    let Ok(method) = Method::from_bytes(b"UNSUBSCRIBE") else {
        return;
    };
    if let Err(e) = http
        .request(method, event_sub_url.clone())
        .header("SID", sid)
        .send()
        .await
    {
        warn!(error = %e, sid, "UNSUBSCRIBE failed");
    }
}

fn parse_timeout_header(value: &str) -> Option<Duration> {
    value
        .strip_prefix("Second-")
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
}

/// The local HTTP endpoint every GENA subscription this client makes uses
/// as its CALLBACK - one listener shared across every subscription a given
/// `RendererClient` holds, dispatching each incoming NOTIFY to the
/// subscription its SID belongs to.
pub(crate) struct NotifyListener {
    /// The address advertised in CALLBACK - the machine's LAN-reachable IP,
    /// not necessarily what the listener is bound to. A subscribed-to
    /// device off-host (the common case for an external control point like
    /// `upnp-suite-cli`) can never route a NOTIFY back to 127.0.0.1; only a
    /// subscriber on the *same* host as the device (e.g. this renderer
    /// subscribing to its own loopback SOAP endpoint) can use that. Binding
    /// `0.0.0.0` and advertising the real interface IP works for both.
    advertise_addr: SocketAddr,
    routes: Mutex<HashMap<String, mpsc::UnboundedSender<String>>>,
}

impl NotifyListener {
    pub(crate) async fn start() -> Result<Arc<Self>, ClientError> {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:0")
            .await
            .map_err(|e| {
                ClientError::MalformedResponse(format!("failed to bind NOTIFY listener: {e}"))
            })?;
        let port = listener.local_addr().map_err(|e| {
            ClientError::MalformedResponse(format!("failed to read NOTIFY listener address: {e}"))
        })?.port();
        let advertise_ip = local_ip_address::local_ip().unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));

        let this = Arc::new(Self {
            advertise_addr: SocketAddr::new(advertise_ip, port),
            routes: Mutex::new(HashMap::new()),
        });

        let state = Arc::clone(&this);
        tokio::spawn(async move {
            let app = Router::new().route("/", any(notify_handler)).with_state(state);
            if let Err(e) = axum::serve(listener, app).await {
                warn!(error = %e, "GENA NOTIFY listener stopped");
            }
        });

        Ok(this)
    }

    fn callback_url(&self) -> String {
        format!("http://{}/", self.advertise_addr)
    }

    fn register(&self, sid: String) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.routes.lock().unwrap().insert(sid, tx);
        rx
    }

    fn unregister(&self, sid: &str) {
        self.routes.lock().unwrap().remove(sid);
    }
}

async fn notify_handler(
    State(listener): State<Arc<NotifyListener>>,
    headers: HeaderMap,
    body: String,
) -> StatusCode {
    let Some(sid) = headers.get("SID").and_then(|v| v.to_str().ok()) else {
        return StatusCode::BAD_REQUEST;
    };

    let sender = listener.routes.lock().unwrap().get(sid).cloned();
    match sender {
        Some(tx) => {
            let _ = tx.send(body);
            StatusCode::OK
        }
        // An unknown SID means our own UNSUBSCRIBE raced this NOTIFY -
        // still acknowledge it rather than making the sender think
        // delivery failed.
        None => StatusCode::OK,
    }
}
