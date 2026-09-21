//! A live-updating ratatui dashboard for one renderer: transport state
//! (GENA-pushed, falling back to polling — see
//! [`RendererClient::subscribe_av_transport`]), volume/mute likewise, and
//! track position, which is never GENA-evented in UPnP so it's always
//! polled directly here via `GetPositionInfo`.
//!
//! Also accepts playback controls: space toggles play/pause, `s` stops,
//! `+`/`-` step volume, `m` toggles mute, and - only when the current
//! track's DIDL-Lite metadata looks like video (see
//! [`upnp_suite_client::suggests_video`]) - `b`/`B` step brightness and
//! `c`/`C` step contrast. Issued as SOAP actions against the selected
//! device and reflected back once its GENA NOTIFY (or the next position
//! poll) catches up.
//!
//! Brightness/Contrast are gated on content type in this dashboard, not by
//! the renderer itself: `SetBrightness`/`SetContrast` succeed against any
//! track UPnP-side (the spec doesn't tie them to a media type), it's just
//! that adjusting a video picture setting for audio-only playback has no
//! visible effect to show for it here.

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
const VOLUME_STEP: u16 = 5;
const PICTURE_STEP: u16 = 5;

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
    let rc = client.rendering_control().ok();

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

                        let is_playing = transport_rx.borrow().transport_state == "PLAYING";
                        let is_video = upnp_suite_client::suggests_video(
                            transport_rx.borrow().current_metadata.as_deref(),
                        );
                        let current_volume = rendering_rx.as_ref().map(|rx| rx.borrow().volume);
                        let current_muted = rendering_rx.as_ref().map(|rx| rx.borrow().muted);
                        let current_brightness =
                            rendering_rx.as_ref().map(|rx| rx.borrow().brightness);
                        let current_contrast =
                            rendering_rx.as_ref().map(|rx| rx.borrow().contrast);

                        match key.code {
                            KeyCode::Char(' ') => {
                                let result = if is_playing { av.pause().await } else { av.play().await };
                                if let Err(err) = result {
                                    eprintln!("Play/pause failed: {err}");
                                }
                            }
                            KeyCode::Char('s') => {
                                if let Err(err) = av.stop().await {
                                    eprintln!("Stop failed: {err}");
                                }
                            }
                            KeyCode::Char('+') | KeyCode::Char('=') => {
                                if let (Some(rc), Some(volume)) = (&rc, current_volume) {
                                    let new_volume = volume.saturating_add(VOLUME_STEP).min(100);
                                    if let Err(err) = rc.set_volume(new_volume).await {
                                        eprintln!("Set volume failed: {err}");
                                    }
                                }
                            }
                            KeyCode::Char('-') | KeyCode::Char('_') => {
                                if let (Some(rc), Some(volume)) = (&rc, current_volume) {
                                    let new_volume = volume.saturating_sub(VOLUME_STEP);
                                    if let Err(err) = rc.set_volume(new_volume).await {
                                        eprintln!("Set volume failed: {err}");
                                    }
                                }
                            }
                            KeyCode::Char('m') => {
                                if let (Some(rc), Some(muted)) = (&rc, current_muted) {
                                    if let Err(err) = rc.set_mute(!muted).await {
                                        eprintln!("Set mute failed: {err}");
                                    }
                                }
                            }
                            KeyCode::Char('b') if is_video => {
                                if let (Some(rc), Some(brightness)) = (&rc, current_brightness) {
                                    let new_brightness = brightness.saturating_sub(PICTURE_STEP);
                                    if let Err(err) = rc.set_brightness(new_brightness).await {
                                        eprintln!("Set brightness failed: {err}");
                                    }
                                }
                            }
                            KeyCode::Char('B') if is_video => {
                                if let (Some(rc), Some(brightness)) = (&rc, current_brightness) {
                                    let new_brightness =
                                        brightness.saturating_add(PICTURE_STEP).min(100);
                                    if let Err(err) = rc.set_brightness(new_brightness).await {
                                        eprintln!("Set brightness failed: {err}");
                                    }
                                }
                            }
                            KeyCode::Char('c') if is_video => {
                                if let (Some(rc), Some(contrast)) = (&rc, current_contrast) {
                                    let new_contrast = contrast.saturating_sub(PICTURE_STEP);
                                    if let Err(err) = rc.set_contrast(new_contrast).await {
                                        eprintln!("Set contrast failed: {err}");
                                    }
                                }
                            }
                            KeyCode::Char('C') if is_video => {
                                if let (Some(rc), Some(contrast)) = (&rc, current_contrast) {
                                    let new_contrast =
                                        contrast.saturating_add(PICTURE_STEP).min(100);
                                    if let Err(err) = rc.set_contrast(new_contrast).await {
                                        eprintln!("Set contrast failed: {err}");
                                    }
                                }
                            }
                            _ => {}
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
    let is_video = upnp_suite_client::suggests_video(transport.current_metadata.as_deref());

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Length(3),
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
    frame.render_widget(picture_panel(rendering, is_video), layout[4]);

    let help = if is_video {
        "space play/pause · s stop · +/- volume · m mute · b/B brightness · c/C contrast · q / Esc / Ctrl-C quit"
    } else {
        "space play/pause · s stop · +/- volume · m mute · q / Esc / Ctrl-C quit"
    };
    frame.render_widget(Paragraph::new(help).dim(), layout[5]);
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

/// Brightness/Contrast are UPnP state regardless of content type (see this
/// module's doc), so this always shows the current values when available -
/// `is_video` only decides the *reason* shown when they can't be adjusted
/// from here, not whether the panel appears at all.
fn picture_panel(rendering: Option<&RenderingControlSnapshot>, is_video: bool) -> Paragraph<'static> {
    let text = match rendering {
        Some(snapshot) => {
            let values = format!(
                "Brightness: {} · Contrast: {}",
                snapshot.brightness, snapshot.contrast
            );
            if is_video {
                values
            } else {
                format!("{values} (current track isn't video - b/B/c/C disabled)")
            }
        }
        None => "RenderingControl not available on this device".to_string(),
    };

    Paragraph::new(text).block(Block::default().borders(Borders::ALL).title("Picture"))
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
