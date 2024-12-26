use discovery::{service::MediaRendererConfig, SsdpAdvertiser};
use server::HttpServer;
use services::MediaRenderer;
use std::{error::Error, sync::Arc};
use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let use_json = std::env::var("LOG_FORMAT")
        .map(|v| v == "json")
        .unwrap_or(false);

    init_logging(use_json)?;

    info!("Starting UPnP Media Renderer...");

    let renderer = MediaRenderer::new(
        MediaRendererConfig::builder()
            .port(8080)
            .friendly_name("Jono's Rust Media Renderer".to_string())
            .build(),
    )
    .map_err(|e| {
        error!(error = %e, "Failed to create media renderer");
        e
    })?;

    info!(
        ip = %renderer.ip_addr(),
        port = %renderer.port(),
        name = %renderer.friendly_name(),
        "Created media renderer"
    );

    let mut advertiser = SsdpAdvertiser::new().await.map_err(|e| {
        error!(error = %e, "Failed to create SSDP advertiser");
        e
    })?;

    let renderer_for_ssdp = renderer.clone();
    advertiser.add_service(renderer_for_ssdp);
    info!("Added renderer to SSDP advertiser");

    let http_server = HttpServer::new(Arc::new(renderer));
    let bind_addr = format!("{}:{}", http_server.ip_addr(), http_server.port());

    info!("Initializing services...");

    tokio::select! {
        result = advertiser.run() => {
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
    }

    Ok(())
}
