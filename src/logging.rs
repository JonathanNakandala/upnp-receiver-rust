//! Custom `tracing` event formatters.
//!
//! Default `tracing_subscriber` output leads with `ThreadId(01)` and the full
//! module path, which is noisy for a project with only a handful of logical
//! components. Both styles here derive a short `component` from the tracing
//! target (module path) instead — so existing `info!`/`debug!`/etc. call
//! sites don't need to change.
//!
//! - [`LogStyle::Terminal`] (the default): `INFO http Starting server status=200` —
//!   level/component/message are positional and colored for scanning
//!   in a terminal; no file:line (interactive use doesn't need it).
//! - [`LogStyle::Logfmt`]: `level=info component=http msg="Starting server" status=200 file=src/x.rs:12` —
//!   fully labeled `key=value` pairs, colorless, for piping to a file or log
//!   aggregator. Select with `LOG_FORMAT=logfmt`.
//!
//! Both are colored when stdout is a terminal and `NO_COLOR` isn't set.

use std::fmt;
use std::io::IsTerminal;

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const COMPONENT_COLOR: &str = "\x1b[34m"; // blue
const MESSAGE_COLOR: &str = "\x1b[97m"; // bright white
const FIELD_COLOR: &str = "\x1b[90m"; // gray — recedes behind the message

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LogStyle {
    Terminal,
    Logfmt,
}

pub struct TextFormatter {
    style: LogStyle,
    color: bool,
}

impl TextFormatter {
    pub fn new(style: LogStyle) -> Self {
        let color = std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal();
        Self { style, color }
    }

    /// Bypasses tty/NO_COLOR auto-detection to force color on or off —
    /// useful for previewing output regardless of how it's invoked.
    pub fn with_color(style: LogStyle, color: bool) -> Self {
        Self { style, color }
    }
}

impl<S, N> FormatEvent<S, N> for TextFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let level = meta.level();
        let component = component_for(meta.target());

        let mut fields = FieldCollector::default();
        event.record(&mut fields);
        let message = fields.message.unwrap_or_default();

        // e.g. tower_http's TraceLayer attaches method/uri/version to the
        // request span rather than to each "started"/"finished processing
        // request" event — without this, those lines would say nothing about
        // *which* request they're for.
        let span_fields = span_fields(ctx);
        let has_fields = !fields.pairs.is_empty() || span_fields.is_some();

        match self.style {
            LogStyle::Terminal => {
                // Two spaces between each of level/component/message/fields;
                // individual fields are separated by one. Grayed-out fields
                // (color mode only) make the bright message the thing your
                // eye lands on first, with fields readable as secondary
                // detail rather than competing with it.
                if self.color {
                    write!(
                        writer,
                        "{}{}{}  ",
                        level_color(level),
                        level_str(level).to_uppercase(),
                        RESET
                    )?;
                    write!(writer, "{}{}{}{}  ", BOLD, COMPONENT_COLOR, component, RESET)?;
                    write!(writer, "{}{}{}", MESSAGE_COLOR, message, RESET)?;
                } else {
                    write!(writer, "{}  ", level_str(level).to_uppercase())?;
                    write!(writer, "{}  ", component)?;
                    write!(writer, "{}", message)?;
                }

                if has_fields {
                    if self.color {
                        write!(writer, "  {}", FIELD_COLOR)?;
                        if let Some(sf) = &span_fields {
                            write!(writer, "{} ", sf)?;
                        }
                        for (key, value) in &fields.pairs {
                            write_kv(&mut writer, key, value)?;
                        }
                        write!(writer, "{}", RESET)?;
                    } else {
                        write!(writer, "  ")?;
                        if let Some(sf) = &span_fields {
                            write!(writer, "{} ", sf)?;
                        }
                        for (key, value) in &fields.pairs {
                            write_kv(&mut writer, key, value)?;
                        }
                    }
                }
            }
            LogStyle::Logfmt => {
                if self.color {
                    write!(
                        writer,
                        "{}level={:<5}{} ",
                        level_color(level),
                        level_str(level),
                        RESET
                    )?;
                    write!(
                        writer,
                        "{}{}component={}{} ",
                        BOLD, COMPONENT_COLOR, component, RESET
                    )?;
                } else {
                    write!(writer, "level={:<5} ", level_str(level))?;
                    write!(writer, "component={} ", component)?;
                }

                if let Some(sf) = &span_fields {
                    write!(writer, "{} ", sf)?;
                }

                for (key, value) in &fields.pairs {
                    write_kv(&mut writer, key, value)?;
                }

                if let (Some(file), Some(line)) = (meta.file(), meta.line()) {
                    write!(writer, "file={}:{} ", file, line)?;
                }

                write!(writer, "msg={:?}", message)?;
            }
        }

        writeln!(writer)
    }
}

/// Collects the formatted fields (e.g. `method=POST uri=/foo`) attached to
/// every span the current event is nested in, root-to-leaf. tower_http's
/// `TraceLayer` is the main producer of these — it records request metadata
/// on the span once rather than on every event within it.
fn span_fields<S, N>(ctx: &FmtContext<'_, S, N>) -> Option<String>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    let scope = ctx.event_scope()?;
    let mut buf = String::new();

    for span in scope.from_root() {
        let ext = span.extensions();
        if let Some(fields) = ext.get::<tracing_subscriber::fmt::FormattedFields<N>>() {
            if !fields.is_empty() {
                if !buf.is_empty() {
                    buf.push(' ');
                }
                buf.push_str(fields);
            }
        }
    }

    if buf.is_empty() {
        None
    } else {
        Some(buf)
    }
}

/// Maps a tracing target (Rust module path, e.g.
/// `upnp_receiver::discovery::ssdp`) to a short, stable component name.
/// Substring matching keeps this robust to nested submodules (e.g.
/// `server::soap::handlers::av_transport::action` still maps to
/// `av_transport`) without needing a call site to opt in.
fn component_for(target: &str) -> std::borrow::Cow<'static, str> {
    // Names match how UPnP itself refers to these: service type names are
    // spec'd as e.g. "urn:schemas-upnp-org:service:AVTransport:1", and SSDP/
    // HTTP/SOAP are protocol acronyms conventionally written in caps.
    const TABLE: &[(&str, &str)] = &[
        ("discovery::ssdp", "SSDP"),
        ("discovery::search", "SSDP"),
        ("discovery::messages", "SSDP"),
        ("discovery::service", "SSDP"),
        ("rendering_control", "RenderingControl"),
        ("connection_manager", "ConnectionManager"),
        ("av_transport", "AVTransport"),
        ("media_renderer", "MediaRenderer"),
        ("backend::gstreamer", "MediaBackend"),
        ("backend::dummy", "MediaBackend"),
        ("client_logger", "ClientLog"),
        ("http_server", "HTTP"),
        ("tower_http", "HTTP"),
        ("soap", "SOAP"),
    ];

    // The crate-root module (main.rs) has a target equal to the crate name
    // itself, with no "::" to distinguish it from a real submodule.
    if target == "upnp_receiver" {
        return std::borrow::Cow::Borrowed("Main");
    }

    for (needle, name) in TABLE {
        if target.contains(needle) {
            return std::borrow::Cow::Borrowed(*name);
        }
    }

    // Fall back to the last module path segment, PascalCased to match the
    // table above, rather than the full path.
    let segment = target.rsplit("::").next().unwrap_or(target);
    std::borrow::Cow::Owned(to_pascal_case(segment))
}

fn to_pascal_case(snake: &str) -> String {
    snake
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn level_str(level: &Level) -> &'static str {
    match *level {
        Level::TRACE => "trace",
        Level::DEBUG => "debug",
        Level::INFO => "info",
        Level::WARN => "warn",
        Level::ERROR => "error",
    }
}

fn level_color(level: &Level) -> &'static str {
    match *level {
        Level::TRACE => "\x1b[2m",  // dim
        Level::DEBUG => "\x1b[36m", // cyan
        Level::INFO => "\x1b[32m",  // green
        Level::WARN => "\x1b[33m",  // yellow
        Level::ERROR => "\x1b[31m", // red
    }
}

fn write_kv(writer: &mut Writer<'_>, key: &str, value: &str) -> fmt::Result {
    if value.is_empty() || value.contains(' ') || value.contains('"') {
        write!(writer, "{}={:?} ", key, value)
    } else {
        write!(writer, "{}={} ", key, value)
    }
}

/// Pulls the `message` field out separately (rendered positionally in
/// terminal style, as `msg=` in logfmt style) and collects every other field
/// in call order for `key=value` rendering.
#[derive(Default)]
struct FieldCollector {
    message: Option<String>,
    pairs: Vec<(&'static str, String)>,
}

impl Visit for FieldCollector {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let formatted = format!("{:?}", value);
        if field.name() == "message" {
            self.message = Some(formatted);
        } else {
            self.pairs.push((field.name(), formatted));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.pairs.push((field.name(), value.to_string()));
        }
    }
}
