//! Standalone UPnP control-point CLI.
//!
//! With no arguments, shows an interactive menu (see [`menu`]) offering:
//! SSDP device/service discovery (the original behavior), a per-device
//! capabilities report, subscribing to a device's GENA events and printing
//! them as they arrive, and a live transport/position/volume dashboard for
//! a selected device.
//!
//! Any of those can also be run directly as a subcommand — see
//! `upnp-discover --help`.
//!
//! Progress/errors go through `tracing` (the same timestamp-free
//! `TextFormatter` the renderer binary uses).

mod capabilities;
mod cli;
mod dashboard;
mod discover;
mod gena;
mod menu;

use std::time::Duration;

use clap::Parser;
use cli::{Cli, Command};
use tracing_subscriber::EnvFilter;
use upnp_suite_core::logging::{LogStyle, TextFormatter};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .event_format(TextFormatter::new(LogStyle::Terminal))
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Command::Discover { wait_secs, vars }) => {
            match discover::scan(Duration::from_secs(wait_secs)).await {
                Ok(devices) => {
                    if devices.is_empty() {
                        println!("No UPnP devices responded.");
                    } else {
                        discover::print_report(&devices, vars);
                    }
                }
                Err(err) => {
                    tracing::error!(error = %err, "SSDP discovery failed");
                    std::process::exit(1);
                }
            }
        }
        Some(Command::Capabilities { wait_secs }) => {
            capabilities::run(Duration::from_secs(wait_secs)).await
        }
        Some(Command::Gena { wait_secs }) => gena::run(Duration::from_secs(wait_secs)).await,
        Some(Command::Dashboard { wait_secs }) => dashboard::run(Duration::from_secs(wait_secs)).await,
        None => menu::run().await,
    }
}
