use backend::dummy::{run_dummy_process, DummyBackendManager};
use backend::{gstreamer, DummyBackend, GStreamerBackend, MediaBackend};
use discovery::{service::MediaRendererConfig, SsdpAdvertiser};
use logging::{LogStyle, TextFormatter};
use server::{ClientLogger, HttpServer};
use services::MediaRenderer;
use std::path::PathBuf;
use std::{error::Error, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
mod backend;
mod discovery;
mod ip;
mod logging;
mod server;
mod services;

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
    SelectedBackend {
        media_backend: Some(Arc::new(backend)),
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
                dummy_manager: Some(manager),
                supported_mime_types: vec!["audio/mpeg".to_string()],
            }
        }
        Err(e) => {
            warn!(error = %e, "Failed to start dummy media backend - audio features will be unavailable");
            SelectedBackend {
                media_backend: None,
                dummy_manager: None,
                supported_mime_types: Vec::new(),
            }
        }
    }
}

async fn select_backend(choice: BackendChoice) -> SelectedBackend {
    match choice {
        BackendChoice::GStreamer => match GStreamerBackend::new() {
            Ok(backend) => use_gstreamer_backend(backend),
            Err(e) => {
                eprintln!("--media-backend gstreamer requested but GStreamer is unavailable: {e}");
                std::process::exit(1);
            }
        },
        BackendChoice::Dummy => start_dummy_backend().await,
        BackendChoice::Auto => match GStreamerBackend::new() {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // Check if running as the dummy backend's child process
    if args.len() >= 3 && args[1] == "--backend" {
        if let Ok(port) = args[2].parse::<u16>() {
            return run_dummy_process(port).await;
        } else {
            eprintln!("Invalid port number for backend process");
            std::process::exit(1);
        }
    }

    // Regular application code
    init_logging(&parse_log_format())?;
    info!("Starting UPnP Media Renderer...");

    // Initialize components
    let backend_choice = parse_backend_choice(&args);
    let selected = select_backend(backend_choice).await;
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
        result = http_server.run(bind_addr) => {
            if let Err(e) = result {
                error!(
                    error = %e,
                    error_type = std::any::type_name_of_val(&e),
                    "HTTP Server error"
                );
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
