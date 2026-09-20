//! Lists every GENA-eventable service across discovered devices and, for
//! the ones this crate's client actually knows how to drive (AVTransport,
//! RenderingControl), subscribes and prints each incoming snapshot to the
//! terminal until Ctrl-C.

use std::fmt;
use std::time::Duration;

use chrono::Utc;
use upnp_suite_client::RendererClient;

use crate::discover::{self, split_urn, Discovered};

const AV_TRANSPORT_PREFIX: &str = "urn:schemas-upnp-org:service:AVTransport:";
const RENDERING_CONTROL_PREFIX: &str = "urn:schemas-upnp-org:service:RenderingControl:";

#[derive(Clone, Copy, PartialEq, Eq)]
enum ServiceKind {
    AvTransport,
    RenderingControl,
    Unsupported,
}

impl ServiceKind {
    fn of(service_type: &str) -> Self {
        if service_type.starts_with(AV_TRANSPORT_PREFIX) {
            ServiceKind::AvTransport
        } else if service_type.starts_with(RENDERING_CONTROL_PREFIX) {
            ServiceKind::RenderingControl
        } else {
            ServiceKind::Unsupported
        }
    }
}

/// One selectable (device, service) pair from the scan, restricted to
/// services this client can actually subscribe to.
struct GenaTarget {
    device_label: String,
    description_url: String,
    service_type: String,
    kind: ServiceKind,
}

impl fmt::Display for GenaTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (name, version) = split_urn(&self.service_type);
        write!(f, "{} — {}:{}", self.device_label, name, version)
    }
}

/// Runs the full flow: scan, print every eventable service found (including
/// ones this client can't subscribe to yet, for visibility), let the user
/// pick a supported one, then subscribe and stream snapshots until Ctrl-C.
pub async fn run(wait: Duration) {
    let devices = match discover::scan(wait).await {
        Ok(devices) => devices,
        Err(err) => {
            eprintln!("SSDP discovery failed: {err}");
            return;
        }
    };

    if devices.is_empty() {
        println!("No UPnP devices responded.");
        return;
    }

    let all_eventable = list_eventable_services(&devices);
    if all_eventable.is_empty() {
        println!("No discovered service advertises GENA eventing.");
        return;
    }

    println!("\nServices advertising GENA eventing:");
    for target in &all_eventable {
        let note = match target.kind {
            ServiceKind::Unsupported => "  (not yet subscribable by this CLI)",
            _ => "",
        };
        println!("  - {target}{note}");
    }

    let selectable: Vec<GenaTarget> = all_eventable
        .into_iter()
        .filter(|t| t.kind != ServiceKind::Unsupported)
        .collect();

    if selectable.is_empty() {
        println!("\nNone of those are subscribable yet (only AVTransport/RenderingControl are wired up).");
        return;
    }

    let Ok(target) = inquire::Select::new("Subscribe to:", selectable).prompt() else {
        return;
    };

    subscribe_and_print(&target).await;
}

fn list_eventable_services(devices: &[Discovered]) -> Vec<GenaTarget> {
    let mut targets = Vec::new();
    for device in devices {
        for service in &device.report.services {
            if service.event_sub_url.is_none() {
                continue;
            }
            targets.push(GenaTarget {
                device_label: device.label(),
                description_url: device.report.location.clone(),
                service_type: service.service_type.clone(),
                kind: ServiceKind::of(&service.service_type),
            });
        }
    }
    targets
}

async fn subscribe_and_print(target: &GenaTarget) {
    println!("\nConnecting to {}...", target.description_url);
    let client = match RendererClient::connect(&target.description_url).await {
        Ok(client) => client,
        Err(err) => {
            eprintln!("Failed to connect: {err}");
            return;
        }
    };

    println!("Subscribing...");

    match target.kind {
        ServiceKind::AvTransport => {
            let stream = match client.subscribe_av_transport().await {
                Ok(stream) => stream,
                Err(err) => {
                    eprintln!("Subscribe failed: {err}");
                    return;
                }
            };
            println!("Subscribed. Waiting for events (Ctrl-C to stop)...\n");
            let mut rx = stream.changes();
            print_snapshot("AVTransport", &*rx.borrow_and_update());
            while rx.changed().await.is_ok() {
                print_snapshot("AVTransport", &*rx.borrow_and_update());
            }
        }
        ServiceKind::RenderingControl => {
            let stream = match client.subscribe_rendering_control().await {
                Ok(stream) => stream,
                Err(err) => {
                    eprintln!("Subscribe failed: {err}");
                    return;
                }
            };
            println!("Subscribed. Waiting for events (Ctrl-C to stop)...\n");
            let mut rx = stream.changes();
            print_snapshot("RenderingControl", &*rx.borrow_and_update());
            while rx.changed().await.is_ok() {
                print_snapshot("RenderingControl", &*rx.borrow_and_update());
            }
        }
        ServiceKind::Unsupported => unreachable!("filtered out before this point"),
    }

    println!("Event stream ended.");
}

fn print_snapshot(label: &str, snapshot: &impl fmt::Debug) {
    println!("[{}] {label}: {snapshot:?}", Utc::now().format("%H:%M:%S%.3f"));
}
