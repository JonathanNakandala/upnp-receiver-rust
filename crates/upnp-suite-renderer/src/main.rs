use backend::dummy::{run_dummy_process, DummyBackendManager};
use backend::gstreamer::{MenuState, PlaybackControls, SubtitleControl, VideoOutput};
use backend::{gstreamer, DummyBackend, GStreamerBackend, MediaBackend, MediaBackendError};
use server::soap::handlers::av_transport::utils::{parse_play_speed, SUPPORTED_PLAY_SPEEDS};
use server::soap::types::av_transport::TransportState;
use server::{ClientLogger, HttpServer};
use services::MediaRenderer;
use std::path::PathBuf;
use std::{error::Error, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use upnp_suite_client::RendererClient;
use upnp_suite_core::discovery::{service::MediaRendererConfig, SsdpAdvertiser};
use upnp_suite_core::logging::{LogStyle, TextFormatter};
mod backend;
mod ip;
mod server;
mod services;

const RENDERER_SERVER_HEADER: &str = concat!("Rust/", env!("CARGO_PKG_VERSION"), " UPnP/1.0");

/// Which [`MediaBackend`] to use, chosen via `--media-backend <value>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendChoice {
    /// Try GStreamer; fall back to the dummy backend (with a warning) if
    /// it's unavailable. The default when `--media-backend` isn't passed.
    Auto,
    /// Require GStreamer; quit if it's unavailable.
    GStreamer,
    /// Use the simulated subprocess backend unconditionally.
    Dummy,
}

fn parse_backend_choice(args: &[String]) -> BackendChoice {
    let value = args
        .iter()
        .position(|a| a == "--media-backend")
        .and_then(|i| args.get(i + 1));

    match value.map(String::as_str) {
        None => BackendChoice::Auto,
        Some("gstreamer") => BackendChoice::GStreamer,
        Some("dummy") => BackendChoice::Dummy,
        Some(other) => {
            eprintln!(
                "Invalid --media-backend value '{}': expected 'gstreamer' or 'dummy'",
                other
            );
            std::process::exit(1);
        }
    }
}

/// The media backend selected at startup, plus the dummy backend's process
/// manager if that's what got picked — kept separately since only the dummy
/// backend needs a health-monitor task and an async shutdown step.
struct SelectedBackend {
    media_backend: Option<Arc<dyn MediaBackend>>,
    /// The video window's menu subtitle toggle needs this specifically
    /// (not just the generic `MediaBackend`) - only ever `Some` for the
    /// GStreamer backend, since subtitle tracks are a GStreamer pipeline
    /// concept the dummy backend has nothing analogous to.
    subtitle_control: Option<Arc<dyn SubtitleControl>>,
    dummy_manager: Option<Arc<Mutex<DummyBackendManager>>>,
    /// What `ConnectionManager::GetProtocolInfo` should advertise as
    /// playable, given the backend actually in use.
    supported_mime_types: Vec<String>,
}

/// GStreamer's registry enumeration (see [`gstreamer::supported_mime_types`])
/// is best-effort and can come back empty on an unusual install; falling
/// back to at least MP3 keeps `GetProtocolInfo` non-empty, since a bare
/// empty list looks like "this renderer plays nothing" to a control point.
fn gstreamer_supported_mime_types() -> Vec<String> {
    let mime_types = gstreamer::supported_mime_types();
    if mime_types.is_empty() {
        vec!["audio/mpeg".to_string()]
    } else {
        mime_types
    }
}

fn use_gstreamer_backend(backend: GStreamerBackend) -> SelectedBackend {
    info!("Using GStreamer media backend");
    let supported_mime_types = gstreamer_supported_mime_types();
    info!(
        mime_types = %supported_mime_types.join(", "),
        "Detected GStreamer-supported formats"
    );
    let backend = Arc::new(backend);
    SelectedBackend {
        media_backend: Some(backend.clone() as Arc<dyn MediaBackend>),
        subtitle_control: Some(backend as Arc<dyn SubtitleControl>),
        dummy_manager: None,
        supported_mime_types,
    }
}

async fn start_dummy_backend() -> SelectedBackend {
    let manager = Arc::new(Mutex::new(
        DummyBackendManager::new()
            .with_heartbeat_timeout(Duration::from_secs(5))
            .with_max_restart_attempts(10),
    ));

    let start_result = {
        let mut lock = manager.lock().await;
        lock.start().await
    };

    match start_result {
        Ok(_) => {
            info!("Dummy media backend started successfully");
            SelectedBackend {
                media_backend: Some(Arc::new(DummyBackend::new(manager.clone()))),
                subtitle_control: None,
                dummy_manager: Some(manager),
                supported_mime_types: vec!["audio/mpeg".to_string()],
            }
        }
        Err(e) => {
            warn!(error = %e, "Failed to start dummy media backend - audio features will be unavailable");
            SelectedBackend {
                media_backend: None,
                subtitle_control: None,
                dummy_manager: None,
                supported_mime_types: Vec::new(),
            }
        }
    }
}

/// Picks and constructs the backend `--media-backend` (or its `Auto`
/// default) asked for. Takes `build_gstreamer_backend` rather than a
/// `VideoOutput` (or any other GStreamer-specific construction detail)
/// directly: this function's job is choosing *among* backends, and
/// `BackendChoice::Dummy` - and any backend added here later - has nothing
/// to do with how the GStreamer variant wires up its video window. A
/// closure keeps that detail scoped to the one match arm that needs it
/// instead of leaking a GStreamer type into every caller of this function.
async fn select_backend(
    choice: BackendChoice,
    build_gstreamer_backend: impl FnOnce() -> Result<GStreamerBackend, MediaBackendError>,
) -> SelectedBackend {
    match choice {
        BackendChoice::GStreamer => match build_gstreamer_backend() {
            Ok(backend) => use_gstreamer_backend(backend),
            Err(e) => {
                eprintln!("--media-backend gstreamer requested but GStreamer is unavailable: {e}");
                std::process::exit(1);
            }
        },
        BackendChoice::Dummy => start_dummy_backend().await,
        BackendChoice::Auto => match build_gstreamer_backend() {
            Ok(backend) => use_gstreamer_backend(backend),
            Err(e) => {
                warn!(error = %e, "GStreamer unavailable, falling back to dummy media backend");
                start_dummy_backend().await
            }
        },
    }
}

/// `--log-clients` writes every SOAP request/response a control point sends,
/// grouped per client (by User-Agent, falling back to IP), under this
/// directory — useful for working out what real-world clients actually send
/// and which of it isn't implemented yet. Override the location with the
/// `CLIENT_LOG_DIR` env var.
/// `--icon <path>` points at a PNG/JPEG/GIF file to advertise as the device
/// icon; its dimensions/mimetype are sniffed from the file itself. Omitted
/// entirely (no `iconList`) when not passed.
fn parse_icon_path(args: &[String]) -> Option<PathBuf> {
    args.iter()
        .position(|a| a == "--icon")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
}

fn client_log_dir() -> PathBuf {
    std::env::var("CLIENT_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("logs/clients"))
}
/// Chosen via `LOG_FORMAT`:
/// - unset (default): colored terminal output — `INFO http Starting server status=200`.
/// - `logfmt`: colored, fully labeled `key=value` lines (incl. `file:line`) —
///   for piping to a file or a logfmt-aware aggregator.
/// - `json`: structured JSON, one object per line, fields flattened to the
///   top level (not nested under a `fields` key) — for log aggregators that
///   parse JSON directly.
pub fn parse_log_format() -> String {
    std::env::var("LOG_FORMAT").unwrap_or_default()
}

fn init_logging(format: &str) -> Result<(), Box<dyn Error>> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = tracing_subscriber::registry().with(env_filter);

    match format {
        "json" => {
            let formatter = fmt::layer()
                .with_target(true)
                .with_file(true)
                .with_line_number(true)
                .with_timer(fmt::time::UtcTime::rfc_3339())
                .json()
                .flatten_event(true);
            subscriber.with(formatter).try_init()?;
        }
        "logfmt" => {
            // with_ansi(false): our TextFormatter does all coloring itself;
            // without this, tracing_subscriber's own default field formatter
            // (used to pre-format span fields, e.g. tower_http's
            // method/uri) bakes in its own ANSI codes that leak through.
            let formatter = fmt::layer()
                .with_ansi(false)
                .event_format(TextFormatter::new(LogStyle::Logfmt));
            subscriber.with(formatter).try_init()?;
        }
        _ => {
            let formatter = fmt::layer()
                .with_ansi(false)
                .event_format(TextFormatter::new(LogStyle::Terminal));
            subscriber.with(formatter).try_init()?;
        }
    }

    Ok(())
}

// Create the renderer with the selected media backend, if any
fn create_renderer(
    media_backend: Option<Arc<dyn MediaBackend>>,
    supported_mime_types: Vec<String>,
    icon_path: Option<PathBuf>,
) -> Result<MediaRenderer, Box<dyn Error>> {
    let mut builder = MediaRendererConfig::builder()
        .port(8080)
        .server(RENDERER_SERVER_HEADER.to_string())
        .friendly_name("Jono's Rust Media Renderer".to_string());
    if let Some(icon_path) = icon_path {
        builder = builder.icon_path(icon_path);
    }
    let config = builder.build();

    MediaRenderer::new_with_media_backend(config, media_backend, supported_mime_types).map_err(
        |e| {
            error!(error = %e, "Failed to create media renderer");
            e
        },
    )
}
// Initialize the SSDP advertiser
async fn init_advertiser(
    renderer: &MediaRenderer,
) -> Result<Option<SsdpAdvertiser>, Box<dyn std::error::Error>> {
    match SsdpAdvertiser::new().await {
        Ok(mut advertiser) => {
            advertiser.add_service(renderer.clone());
            info!("Added renderer to SSDP advertiser");
            Ok(Some(advertiser))
        }
        Err(e) => {
            // Box the error to convert it to Box<dyn StdError>
            warn!(error = %e, "Failed to create SSDP advertiser - discovery will be unavailable");
            Err(Box::new(e))
        }
    }
}

// Run the dummy backend's health-monitor task only if that's the backend in
// use; GStreamer runs in-process and has nothing to monitor.
async fn run_media_monitor(dummy_manager: Option<Arc<Mutex<DummyBackendManager>>>) {
    if let Some(manager) = dummy_manager {
        DummyBackendManager::run_monitor_task(manager).await;
    } else {
        // Just wait forever - this task should never complete
        futures::future::pending::<()>().await;
    }
}

// Handle shutdown signals
async fn handle_shutdown_signals() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl-c");
}

/// Builds the video window menu's actions - each one drives this same
/// process's own AVTransport service over its real SOAP endpoint via
/// `client` (a loopback `upnp-suite-client` connection), so a menu click is
/// handled - and evented - exactly like a real control point's action
/// would be, through the very same SOAP handler, with no separate
/// hand-rolled `LastChange` publish path to keep in sync. `runtime` lets
/// the menu's closures (which run synchronously, on the main thread, and
/// can't themselves `.await`) hand the actual SOAP call off to the async
/// side.
fn build_playback_controls(
    client: Arc<RendererClient>,
    subtitle_control: Option<Arc<dyn SubtitleControl>>,
    runtime: tokio::runtime::Handle,
) -> PlaybackControls {
    let play_pause: Box<dyn Fn() + Send + Sync> = {
        let client = client.clone();
        let runtime = runtime.clone();
        Box::new(move || {
            let client = client.clone();
            runtime.spawn(async move {
                let Ok(av_transport) = client.av_transport() else {
                    return;
                };
                let state = av_transport
                    .get_transport_info()
                    .await
                    .ok()
                    .and_then(|info| TransportState::parse(&info.current_transport_state))
                    .unwrap_or(TransportState::Stopped);
                let result = if state == TransportState::Playing {
                    av_transport.pause().await
                } else {
                    av_transport.play().await
                };
                if let Err(e) = result {
                    warn!(error = %e, "Menu play/pause failed");
                }
            });
        })
    };

    let stop: Box<dyn Fn() + Send + Sync> = {
        let client = client.clone();
        let runtime = runtime.clone();
        Box::new(move || {
            let client = client.clone();
            runtime.spawn(async move {
                let Ok(av_transport) = client.av_transport() else {
                    return;
                };
                if let Err(e) = av_transport.stop().await {
                    warn!(error = %e, "Menu stop failed");
                }
            });
        })
    };

    let set_speed: Box<dyn Fn(f64) + Send + Sync> = {
        let client = client.clone();
        Box::new(move |speed| {
            let client = client.clone();
            runtime.spawn(async move {
                let Ok(av_transport) = client.av_transport() else {
                    return;
                };
                if let Err(e) = av_transport.set_play_speed(speed).await {
                    warn!(error = %e, speed, "Menu speed change failed");
                }
            });
        })
    };

    let toggle_subtitles: Box<dyn Fn() -> bool + Send + Sync> = Box::new(move || {
        subtitle_control
            .as_ref()
            .map(|c| c.toggle_subtitles())
            .unwrap_or(false)
    });

    PlaybackControls {
        play_pause,
        stop,
        set_speed,
        toggle_subtitles,
        supported_speeds: SUPPORTED_PLAY_SPEEDS.to_vec(),
    }
}

/// Keeps the video window's menu (Play/Pause label, checked speed) in sync
/// by forwarding `client`'s AVTransport event stream - real GENA NOTIFYs
/// when the loopback SUBSCRIBE succeeds, falling back to polling
/// internally otherwise (see `RendererClient::subscribe_av_transport`) -
/// regardless of what last changed the transport: a SOAP action, the menu
/// itself, or the backend reaching end-of-stream on its own. Subtitles
/// need no equivalent here - they're a local GStreamer concept with no
/// UPnP counterpart, and the menu's subtitle checkbox already updates
/// synchronously on toggle (see `PlaybackMenu::dispatch`); this just reads
/// the current value each time it rebuilds `MenuState`.
fn spawn_menu_state_forwarder(
    client: Arc<RendererClient>,
    subtitle_control: Option<Arc<dyn SubtitleControl>>,
    video_output: VideoOutput,
) {
    tokio::spawn(async move {
        let stream = match client.subscribe_av_transport().await {
            Ok(stream) => stream,
            Err(e) => {
                warn!(error = %e, "Failed to subscribe to AVTransport events - video window menu will not stay in sync");
                return;
            }
        };
        let mut rx = stream.changes();

        loop {
            if rx.changed().await.is_err() {
                warn!("AVTransport event stream closed; video window menu will stop updating");
                break;
            }
            let snapshot = rx.borrow_and_update().clone();
            let transport_state =
                TransportState::parse(&snapshot.transport_state).unwrap_or(TransportState::NoMediaPresent);
            let speed = parse_play_speed(&snapshot.play_speed).unwrap_or(1.0);
            let subtitles_enabled = subtitle_control
                .as_ref()
                .map(|c| c.subtitles_enabled())
                .unwrap_or(false);
            video_output.send_state(MenuState {
                transport_state,
                speed,
                subtitles_enabled,
            });
        }
    });
}

/// Everything that isn't the video window: builds the backend, the SSDP
/// advertiser and the HTTP/SOAP server, and runs them until a shutdown
/// signal arrives. Split out from `main` so it can run inside a tokio
/// runtime on its own background thread - see `main`'s doc comment for why.
async fn run_renderer(video_output: VideoOutput) -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    init_logging(&parse_log_format())?;
    info!("Starting UPnP Media Renderer...");

    // Initialize components
    let backend_choice = parse_backend_choice(&args);
    let selected = select_backend(backend_choice, || {
        GStreamerBackend::new_with_video_output(video_output.clone())
    })
    .await;
    let renderer = create_renderer(
        selected.media_backend.clone(),
        selected.supported_mime_types.clone(),
        parse_icon_path(&args),
    )?;

    info!(
        ip = %renderer.ip_addr(),
        port = %renderer.port(),
        name = %renderer.friendly_name(),
        has_media = %renderer.has_media_capabilities(),
        "Created media renderer"
    );
    let ip = renderer.ip_addr();
    let port = renderer.port();

    let advertiser_result = init_advertiser(&renderer).await;
    let advertiser_opt = match advertiser_result {
        Ok(adv) => adv,
        Err(e) => {
            warn!(error = %e, "Failed to create SSDP advertiser - discovery will be unavailable");
            None
        }
    };

    let client_logger = if args.iter().any(|a| a == "--log-clients") {
        let dir = client_log_dir();
        info!(dir = %dir.display(), "Client request logging enabled");
        Some(Arc::new(ClientLogger::new(dir)))
    } else {
        None
    };

    let http_server = HttpServer::new(Arc::new(renderer)).with_client_logger(client_logger);
    let bind_addr = format!("{}:{}", http_server.ip_addr(), http_server.port());

    // Bind - and start accepting connections at the TCP level - before
    // anything else: the video window's menu, wired up below as a loopback
    // control point, needs this server actually reachable.
    let listener = http_server.bind(&bind_addr).await?;
    let server_task = tokio::spawn(http_server.serve(listener));

    // Wires the video window's native menu (play/pause/stop/speed/subtitles)
    // to this same renderer's real SOAP/GENA endpoints over loopback -
    // `upnp-suite-client` talking to this process exactly like a real
    // control point would, so a menu click is handled (and evented) via
    // the same SOAP action code path, not a separate one. Connects via the
    // renderer's own advertised IP rather than 127.0.0.1: the server binds
    // specifically to that interface (not 0.0.0.0), which is otherwise
    // unreachable through loopback.
    match RendererClient::connect(&format!("http://{ip}:{port}/description.xml")).await {
        Ok(client) => {
            let client = Arc::new(client);
            info!(
                has_subtitle_control = selected.subtitle_control.is_some(),
                "Wiring video window menu to AVTransport over loopback"
            );
            let controls = build_playback_controls(
                client.clone(),
                selected.subtitle_control.clone(),
                tokio::runtime::Handle::current(),
            );
            info!("Sending playback controls to video window");
            video_output.send_controls(controls);
            spawn_menu_state_forwarder(client, selected.subtitle_control.clone(), video_output.clone());
        }
        Err(e) => {
            warn!(error = %e, "Failed to connect loopback client - video window menu will not be built");
        }
    }

    info!("All services initialized, starting main loop...");

    // Run all services concurrently
    tokio::select! {
        result = async {
            if let Some(advertiser) = &advertiser_opt {
                advertiser.run().await
            } else {
                // Just wait forever if no advertiser
                Ok(())
            }
        } => {
            if let Err(e) = result {
                error!(
                    error = %e,
                    error_type = std::any::type_name_of_val(&e),
                    "SSDP Advertiser error"
                );
            }
        }
        result = server_task => {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    error!(
                        error = %e,
                        error_type = std::any::type_name_of_val(&e),
                        "HTTP Server error"
                    );
                }
                Err(e) => {
                    error!(error = %e, "HTTP server task panicked");
                }
            }
        }
        _ = run_media_monitor(selected.dummy_manager.clone()) => {
            error!("Media process monitor task exited unexpectedly");
        }
        _ = handle_shutdown_signals() => {
            info!("Shutdown signal received, stopping services...");
        }
    }

    // Shutdown
    if let Some(advertiser) = &advertiser_opt {
        info!("Announcing SSDP byebye...");
        if let Err(e) = advertiser.announce_byebye().await {
            warn!(error = %e, "Failed to announce SSDP byebye");
        }
    }

    if let Some(manager) = selected.dummy_manager {
        info!("Shutting down dummy media backend...");
        if let Err(e) = manager.lock().await.shutdown().await {
            error!(error = %e, "Error shutting down dummy media backend");
        }
    }

    Ok(())
}

/// Not `#[tokio::main]`: the video window's event loop ([`gstreamer::video`])
/// has to run on the process's main thread (a winit/AppKit requirement,
/// strict on macOS), and that loop blocks for the life of the app - it
/// can't share the main thread with a tokio runtime driving everything
/// else. So this thread runs only the window; a second thread gets its own
/// tokio runtime and runs the actual renderer (SSDP, HTTP/SOAP, playback).
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // Running as the dummy backend's child process: a separate, short-lived
    // invocation of this same binary (see `DummyBackendManager`), with no
    // video window involved at all.
    if args.len() >= 3 && args[1] == "--backend" {
        return match args[2].parse::<u16>() {
            Ok(port) => tokio::runtime::Runtime::new()?.block_on(run_dummy_process(port)),
            Err(_) => {
                eprintln!("Invalid port number for backend process");
                std::process::exit(1);
            }
        };
    }

    let (video_output, run_video_window) = gstreamer::video::new();

    std::thread::Builder::new()
        .name("upnp-renderer".to_string())
        .spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("failed to start the async runtime");
            let result = runtime.block_on(run_renderer(video_output));
            if let Err(e) = result {
                eprintln!("Renderer exited with an error: {e}");
            }
            // The renderer (SSDP/HTTP/SOAP/playback) is everything this
            // process does other than show a window; once it's done there's
            // nothing left running, so end the whole process, including the
            // video window's event loop still blocking the main thread.
            std::process::exit(0);
        })
        .expect("failed to spawn the renderer thread");

    run_video_window()?;
    Ok(())
}
