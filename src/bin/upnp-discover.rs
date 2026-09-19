//! Standalone UPnP control-point CLI: sends an SSDP `M-SEARCH`, then fetches
//! and reports each discovered device's identity and every service it
//! advertises — service type/version, the actions it implements, and the
//! SCPD's own UPnP Device Architecture spec version.
//!
//! Usage: `upnp-discover [wait-seconds] [--vars]`
//! - `wait-seconds`: how long to listen for search responses (default 3).
//! - `--vars`/`-v`: also report each state variable that declares an
//!   `allowedValueList` (enum) or `allowedValueRange` (numeric bounds) —
//!   the concrete options/ranges a control point may send that variable.
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
use upnp_receiver::discovery::description::{
    fetch_device_report, DeviceReport, ServiceReport, VariableConstraint,
};
use upnp_receiver::logging::{LogStyle, TextFormatter};

const DEFAULT_WAIT_SECS: u64 = 3;
/// Column width the wrapped action/variable lists target, including the
/// leading indent — chosen to comfortably fit an 80-column terminal.
const WRAP_WIDTH: usize = 76;

struct Args {
    wait: Duration,
    show_vars: bool,
}

impl Args {
    fn parse() -> Self {
        let mut wait_secs = DEFAULT_WAIT_SECS;
        let mut show_vars = false;

        for arg in std::env::args().skip(1) {
            match arg.as_str() {
                "--vars" | "-v" => show_vars = true,
                other => {
                    if let Ok(secs) = other.parse::<u64>() {
                        wait_secs = secs;
                    }
                }
            }
        }

        Self {
            wait: Duration::from_secs(wait_secs),
            show_vars,
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .event_format(TextFormatter::new(LogStyle::Terminal))
        .init();

    let args = Args::parse();
    let wait = args.wait;

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
        println!("{}", device_tree(&palette, response, report, args.show_vars));
    }
}

fn device_tree(
    palette: &Palette,
    response: &SsdpResponse,
    report: &DeviceReport,
    show_vars: bool,
) -> Tree<String> {
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
            .map(|service| service_node(palette, service, show_vars)),
    )
}

fn service_node(palette: &Palette, service: &ServiceReport, show_vars: bool) -> Tree<String> {
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

    let mut node = Tree::new(header);

    if !service.actions.is_empty() {
        node.push(labeled_block(
            palette,
            "Actions",
            &wrap_csv(&service.actions, WRAP_WIDTH),
        ));
    }

    if show_vars {
        let constrained: Vec<String> = service
            .state_variables
            .iter()
            .filter_map(|v| v.constraint.as_ref().map(|c| format_variable(palette, &v.name, c)))
            .collect();

        if !constrained.is_empty() {
            node.push(labeled_block(
                palette,
                "Allowed values",
                &constrained.join("\n"),
            ));
        }
    }

    node.push(gena_node(palette, service));

    node
}

/// GENA eventing support: whether the service declares an `eventSubURL` at
/// all, and — if so — which state variables actually get evented (a
/// SUBSCRIBE against a service with no evented variables succeeds but
/// never delivers a NOTIFY, so that's worth surfacing too).
fn gena_node(palette: &Palette, service: &ServiceReport) -> Tree<String> {
    let Some(event_sub_url) = &service.event_sub_url else {
        return Tree::new(format!(
            "{dim}GENA: not supported{reset}",
            dim = palette.dim,
            reset = palette.reset,
        ));
    };

    let evented: Vec<String> = service
        .state_variables
        .iter()
        .filter(|v| v.evented)
        .map(|v| v.name.clone())
        .collect();

    let body = if evented.is_empty() {
        format!("subscribe: {}\n    (no evented state variables declared)", event_sub_url)
    } else {
        format!(
            "subscribe: {}\n{}",
            event_sub_url,
            wrap_csv(&evented, WRAP_WIDTH)
        )
    };

    labeled_block(palette, "GENA", &body)
}

fn labeled_block(palette: &Palette, label: &str, body: &str) -> Tree<String> {
    Tree::new(format!(
        "{dim}{label}:{reset}\n{body}",
        dim = palette.dim,
        reset = palette.reset,
        label = label,
        body = body,
    ))
    .with_multiline(true)
}

fn format_variable(palette: &Palette, name: &str, constraint: &VariableConstraint) -> String {
    const INDENT: &str = "    ";
    let values = match constraint {
        VariableConstraint::Enum(values) => values.join(", "),
        VariableConstraint::Range {
            minimum,
            maximum,
            step,
        } => match step {
            Some(step) => format!("{}..{} (step {})", minimum, maximum, step),
            None => format!("{}..{}", minimum, maximum),
        },
    };

    format!(
        "{INDENT}{bold}{name}{reset}: {values}",
        bold = palette.bold,
        reset = palette.reset,
    )
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
