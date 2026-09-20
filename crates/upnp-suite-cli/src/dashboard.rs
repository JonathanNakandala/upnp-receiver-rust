//! A live-updating ratatui dashboard for one renderer: transport state
//! (GENA-pushed, falling back to polling — see
//! [`RendererClient::subscribe_av_transport`]), volume/mute likewise, and
//! track position, which is never GENA-evented in UPnP so it's always
//! polled directly here via `GetPositionInfo`.

use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::Frame;
use tokio::time::interval;
use upnp_suite_client::{AVTransportSnapshot, PositionInfo, RendererClient, RenderingControlSnapshot};

use crate::discover;

const AV_TRANSPORT_PREFIX: &str = "urn:schemas-upnp-org:service:AVTransport:";
const POSITION_POLL: Duration = Duration::from_millis(1000);

pub async fn run(wait: Duration) {
    let devices = match discover::scan(wait).await {
        Ok(devices) => devices,
        Err(err) => {
            eprintln!("SSDP discovery failed: {err}");
            return;
        }
    };

    let candidates: Vec<_> = devices
        .into_iter()
        .filter(|d| {
            d.report
                .services
                .iter()
                .any(|s| s.service_type.starts_with(AV_TRANSPORT_PREFIX))
        })
        .collect();

    if candidates.is_empty() {
        println!("No discovered device advertises AVTransport.");
        return;
    }

    let Ok(device) = inquire::Select::new("Dashboard for:", candidates).prompt() else {
        return;
    };
    let device_label = device.label();

    println!("Connecting to {}...", device.report.location);
    let client = match RendererClient::connect(&device.report.location).await {
        Ok(client) => client,
        Err(err) => {
            eprintln!("Failed to connect: {err}");
            return;
        }
    };

    let transport_stream = match client.subscribe_av_transport().await {
        Ok(stream) => stream,
        Err(err) => {
            eprintln!("Failed to subscribe to AVTransport: {err}");
            return;
        }
    };
    let mut transport_rx = transport_stream.changes();

    let rendering_stream = client.subscribe_rendering_control().await.ok();
    let mut rendering_rx = rendering_stream.as_ref().map(|s| s.changes());

    let av = client
        .av_transport()
        .expect("AVTransport confirmed present by the candidate filter above");

    let mut position = PositionInfo::default();
    if let Ok(info) = av.get_position_info().await {
        position = info;
    }
    let mut poll = interval(POSITION_POLL);
    poll.tick().await; // first tick fires immediately; we've already polled once above

    let mut terminal = ratatui::init();
    let mut events = EventStream::new();

    loop {
        let transport_snapshot = transport_rx.borrow_and_update().clone();
        let rendering_snapshot = rendering_rx.as_mut().map(|rx| *rx.borrow_and_update());

        let draw_result = terminal.draw(|frame| {
            render(
                frame,
                &device_label,
                &transport_snapshot,
                rendering_snapshot.as_ref(),
                &position,
            )
        });
        if draw_result.is_err() {
            break;
        }

        let rendering_changed = async {
            match &mut rendering_rx {
                Some(rx) => {
                    let _ = rx.changed().await;
                }
                None => futures::future::pending::<()>().await,
            }
        };

        tokio::select! {
            result = transport_rx.changed() => {
                if result.is_err() {
                    break;
                }
            }
            _ = rendering_changed => {}
            _ = poll.tick() => {
                if let Ok(info) = av.get_position_info().await {
                    position = info;
                }
            }
            event = events.next() => {
                match event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        let quit = matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL));
                        if quit {
                            break;
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => break,
                }
            }
        }
    }

    ratatui::restore();
}

fn render(
    frame: &mut Frame,
    device_label: &str,
    transport: &AVTransportSnapshot,
    rendering: Option<&RenderingControlSnapshot>,
    position: &PositionInfo,
) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(frame.area());

    frame.render_widget(
        Paragraph::new(device_label)
            .block(Block::default().borders(Borders::ALL).title("Device"))
            .bold(),
        layout[0],
    );

    frame.render_widget(transport_panel(transport), layout[1]);
    frame.render_widget(position_gauge(position), layout[2]);
    frame.render_widget(volume_panel(rendering), layout[3]);

    frame.render_widget(
        Paragraph::new("q / Esc / Ctrl-C to quit").dim(),
        layout[4],
    );
}

fn transport_panel(transport: &AVTransportSnapshot) -> Paragraph<'static> {
    let state_color = match transport.transport_state.as_str() {
        "PLAYING" => Color::Green,
        "PAUSED_PLAYBACK" => Color::Yellow,
        "TRANSITIONING" => Color::Cyan,
        "STOPPED" => Color::Red,
        _ => Color::Gray,
    };

    let lines = vec![
        Line::from(vec![
            "State: ".into(),
            transport.transport_state.clone().fg(state_color).bold(),
        ]),
        Line::from(format!("Status: {}", transport.transport_status)),
        Line::from(format!("Speed: {}", transport.play_speed)),
        Line::from(format!("Play mode: {}", transport.play_mode)),
        Line::from(format!("URI: {}", transport.current_uri)),
    ];

    Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Transport"))
}

fn position_gauge(position: &PositionInfo) -> Gauge<'static> {
    let elapsed = parse_hms(&position.rel_time);
    let total = parse_hms(&position.track_duration);

    let ratio = match (elapsed, total) {
        (Some(e), Some(t)) if t > 0 => (e as f64 / t as f64).clamp(0.0, 1.0),
        _ => 0.0,
    };

    let label = format!(
        "{} / {}",
        if position.rel_time.is_empty() { "--:--:--" } else { &position.rel_time },
        if position.track_duration.is_empty() { "--:--:--" } else { &position.track_duration },
    );

    Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Position"))
        .gauge_style(Style::default().fg(Color::Blue))
        .ratio(ratio)
        .label(label)
}

fn volume_panel(rendering: Option<&RenderingControlSnapshot>) -> Paragraph<'static> {
    let text = match rendering {
        Some(snapshot) => format!(
            "Volume: {}{}",
            snapshot.volume,
            if snapshot.muted { " (muted)" } else { "" }
        ),
        None => "RenderingControl not available on this device".to_string(),
    };

    Paragraph::new(text).block(Block::default().borders(Borders::ALL).title("Volume"))
}

/// Parses UPnP's `H+:MM:SS` position/duration strings into total seconds;
/// `None` for anything else (`NOT_IMPLEMENTED`, empty, malformed).
fn parse_hms(value: &str) -> Option<u64> {
    let mut parts = value.split(':').rev();
    let seconds: u64 = parts.next()?.parse().ok()?;
    let minutes: u64 = parts.next().map(|p| p.parse().ok()).unwrap_or(Some(0))?;
    let hours: u64 = parts.next().map(|p| p.parse().ok()).unwrap_or(Some(0))?;
    if parts.next().is_some() {
        return None;
    }
    Some(hours * 3600 + minutes * 60 + seconds)
}
