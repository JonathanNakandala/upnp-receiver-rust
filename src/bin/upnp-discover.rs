//! Standalone UPnP control-point CLI: sends an SSDP `M-SEARCH`, then fetches
//! and reports each discovered device's identity and every service it
//! advertises — service type/version, the actions it implements, and the
//! SCPD's own UPnP Device Architecture spec version.
//!
//! Usage: `upnp-discover [wait-seconds]` (default 3).
//!
//! Progress/errors go through `tracing` (the same timestamp-free
//! `TextFormatter` the receiver binary uses); the actual report is a
//! separate tree-per-device printout via `termtree`, since a 30-action
//! service reads far better as a wrapped block than as a single
//! `key=value` log field.

use std::io::IsTerminal;
use std::time::Duration;
use termtree::Tree;
use tracing_subscriber::EnvFilter;
use upnp_receiver::discovery::client::{discover, SsdpResponse};
use upnp_receiver::discovery::description::{fetch_device_report, DeviceReport, ServiceReport};
use upnp_receiver::logging::{LogStyle, TextFormatter};

const DEFAULT_WAIT_SECS: u64 = 3;
/// Column width the wrapped action list targets, including the leading
/// indent — chosen to comfortably fit an 80-column terminal.
const ACTIONS_WRAP_WIDTH: usize = 76;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .event_format(TextFormatter::new(LogStyle::Terminal))
        .init();

    let wait = Duration::from_secs(
        std::env::args()
            .nth(1)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(DEFAULT_WAIT_SECS),
    );

    tracing::info!(wait_secs = wait.as_secs(), "Sending SSDP M-SEARCH (ssdp:all)");

    let responses = match discover(wait).await {
        Ok(responses) => responses,
        Err(err) => {
            tracing::error!(error = %err, "SSDP discovery failed");
            std::process::exit(1);
        }
    };

    if responses.is_empty() {
        tracing::warn!("No UPnP devices responded");
        return;
    }

    tracing::info!(count = responses.len(), "Discovered devices, fetching descriptions");

    let http = reqwest::Client::new();
    let mut reports = Vec::new();
    for response in responses {
        match fetch_device_report(&http, &response.location).await {
            Ok(report) => reports.push((response, report)),
            Err(err) => tracing::error!(
                location = %response.location,
                error = %err,
                "Failed to fetch/parse device description"
            ),
        }
    }

    let palette = Palette::detect();
    println!();
    for (response, report) in &reports {
        println!("{}", device_tree(&palette, response, report));
    }
}

fn device_tree(palette: &Palette, response: &SsdpResponse, report: &DeviceReport) -> Tree<String> {
    let name = if report.friendly_name.is_empty() {
        "(unnamed device)"
    } else {
        report.friendly_name.as_str()
    };

    let identity = if report.model_name.is_empty() && report.manufacturer.is_empty() {
        String::new()
    } else {
        format!(
            " {dim}— {model} ({manufacturer}){reset}",
            dim = palette.dim,
            model = report.model_name,
            manufacturer = report.manufacturer,
            reset = palette.reset,
        )
    };

    let root = format!(
        "{bold}{name}{reset}{identity}\n{dim}  {udn}\n  {location}\n  from {from}, server: {server}{reset}",
        bold = palette.bold,
        reset = palette.reset,
        dim = palette.dim,
        name = name,
        identity = identity,
        udn = report.udn,
        location = report.location,
        from = response.from,
        server = response.server,
    );

    if report.services.is_empty() {
        return Tree::new(root);
    }

    Tree::new(root).with_leaves(
        report
            .services
            .iter()
            .map(|service| service_node(palette, service)),
    )
}

fn service_node(palette: &Palette, service: &ServiceReport) -> Tree<String> {
    let (name, version) = split_urn(&service.service_type);
    let header = format!(
        "{bold}{name}:{version}{reset} {dim}(SCPD {major}.{minor}){reset}",
        bold = palette.bold,
        reset = palette.reset,
        dim = palette.dim,
        name = name,
        version = version,
        major = service.spec_version.0,
        minor = service.spec_version.1,
    );

    if service.actions.is_empty() {
        return Tree::new(header);
    }

    let actions = wrap_csv(&service.actions, ACTIONS_WRAP_WIDTH);
    Tree::new(format!("{}\n{}", header, actions)).with_multiline(true)
}

/// `"urn:schemas-upnp-org:service:AVTransport:1"` -> `("AVTransport", "1")`.
fn split_urn(urn: &str) -> (&str, &str) {
    let mut parts = urn.rsplitn(3, ':');
    let version = parts.next().unwrap_or("");
    let name = parts.next().unwrap_or(urn);
    (name, version)
}

/// Greedy word-wrap for a comma-separated list of short ASCII identifiers
/// (UPnP action names never need more than that), indented so each
/// continuation line still lines up under the tree connector.
fn wrap_csv(items: &[String], width: usize) -> String {
    const INDENT: &str = "    ";
    let mut lines = Vec::new();
    let mut current = INDENT.to_string();

    for (i, item) in items.iter().enumerate() {
        let piece = if i + 1 == items.len() {
            item.clone()
        } else {
            format!("{}, ", item)
        };

        if current.len() > INDENT.len() && current.len() + piece.len() > width {
            lines.push(std::mem::replace(&mut current, INDENT.to_string()));
        }
        current.push_str(&piece);
    }
    lines.push(current);

    lines
        .into_iter()
        .map(|line| line.trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

struct Palette {
    bold: &'static str,
    dim: &'static str,
    reset: &'static str,
}

impl Palette {
    fn detect() -> Self {
        let enabled = std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal();
        if enabled {
            Self {
                bold: "\x1b[1m",
                dim: "\x1b[2m",
                reset: "\x1b[0m",
            }
        } else {
            Self {
                bold: "",
                dim: "",
                reset: "",
            }
        }
    }
}
