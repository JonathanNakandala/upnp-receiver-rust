use backend::{process::run_backend_process, MediaProcessManager};
use discovery::{service::MediaRendererConfig, SsdpAdvertiser};
use server::HttpServer;
use services::MediaRenderer;
use std::{error::Error, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
mod backend;
mod discovery;
mod ip;
mod server;
mod services;
fn init_logging(json: bool) -> Result<(), Box<dyn Error>> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let formatter = fmt::layer()
        .with_thread_ids(true)
        .with_target(false)
        .with_file(true)
        .with_line_number(true)
        .compact()
        .with_timer(fmt::time::UtcTime::rfc_3339())
        .with_level(true)
        .without_time();

    let subscriber = tracing_subscriber::registry().with(env_filter);

    if json {
        subscriber.with(formatter.json()).try_init()?;
    } else {
        subscriber.with(formatter).try_init()?;
    }

    Ok(())
}

// Initialize media manager - returns None if it fails
async fn init_media_manager() -> Option<Arc<Mutex<MediaProcessManager>>> {
    let manager = Arc::new(Mutex::new(
        MediaProcessManager::new()
            .with_heartbeat_timeout(Duration::from_secs(5))
            .with_max_restart_attempts(10),
    ));

    // Try to start the media process, but don't fail the application if it doesn't work
    let start_result = {
        // Create a new scope for the lock to ensure it's dropped before we potentially move the manager
        let mut lock = manager.lock().await;
        lock.start().await
    };

    match start_result {
        Ok(_) => {
            info!("Media process started successfully");
            Some(manager)
        }
        Err(e) => {
            warn!(error = %e, "Failed to start media process - audio features will be unavailable");
            None
        }
    }
}

// Create the renderer with optional media manager
fn create_renderer(
    media_manager: Option<Arc<Mutex<MediaProcessManager>>>,
) -> Result<MediaRenderer, Box<dyn Error>> {
    let config = MediaRendererConfig::builder()
        .port(8080)
        .friendly_name("Jono's Rust Media Renderer".to_string())
        .build();

    // Just pass the option through directly
    MediaRenderer::new_with_media_backend(config, media_manager).map_err(|e| {
        error!(error = %e, "Failed to create media renderer");
        e.into()
    })
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

// Run the media process monitor task only if we have a media manager
async fn run_media_monitor(media_manager: Option<Arc<Mutex<MediaProcessManager>>>) {
    if let Some(manager) = media_manager {
        MediaProcessManager::run_monitor_task(manager).await;
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

    // Check if running as a backend process
    if args.len() >= 3 && args[1] == "--backend" {
        if let Ok(port) = args[2].parse::<u16>() {
            return run_backend_process(port).await;
        } else {
            eprintln!("Invalid port number for backend process");
            std::process::exit(1);
        }
    }

    // Regular application code
    let use_json = std::env::var("LOG_FORMAT")
        .map(|v| v == "json")
        .unwrap_or(false);

    init_logging(use_json)?;
    info!("Starting UPnP Media Renderer...");

    // Initialize components
    let media_manager = init_media_manager().await;
    let renderer = create_renderer(media_manager.clone())?;

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

    let http_server = HttpServer::new(Arc::new(renderer));
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
        _ = run_media_monitor(media_manager.clone()) => {
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

    if let Some(manager) = media_manager {
        info!("Shutting down media process...");
        if let Err(e) = manager.lock().await.shutdown().await {
            error!(error = %e, "Error shutting down media process");
        }
    }

    Ok(())
}
