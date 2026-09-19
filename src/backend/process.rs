use crate::backend::types::{MediaCommand, MediaResponse};
use crate::logging::{LogStyle, TextFormatter};
use crate::server::soap::types::av_transport::TransportState;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub async fn run_backend_process(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = tracing_subscriber::registry().with(env_filter);

    match std::env::var("LOG_FORMAT").as_deref() {
        Ok("json") => {
            subscriber
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_file(true)
                        .with_line_number(true)
                        .with_timer(fmt::time::UtcTime::rfc_3339())
                        .json()
                        .flatten_event(true),
                )
                .init();
        }
        Ok("logfmt") => {
            subscriber
                .with(
                    fmt::layer()
                        .with_ansi(false)
                        .event_format(TextFormatter::new(LogStyle::Logfmt)),
                )
                .init();
        }
        _ => {
            subscriber
                .with(
                    fmt::layer()
                        .with_ansi(false)
                        .event_format(TextFormatter::new(LogStyle::Terminal)),
                )
                .init();
        }
    }

    // Create TCP listener
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    info!(port, "Media backend process listening");

    // Accept connections
    loop {
        match listener.accept().await {
            Ok((socket, _)) => {
                info!("Client connected to media backend");
                if let Err(e) = handle_client(socket).await {
                    error!("Error handling client: {}", e);
                }
                // Only handle one client at a time for simplicity
                break;
            }
            Err(e) => {
                error!("Error accepting connection: {}", e);
            }
        }
    }

    info!("Backend process shutting down");
    Ok(())
}
#[derive(Debug)]
struct PlaybackState {
    uri: Option<String>,
    metadata: Option<String>,
    position: Duration,
    duration: Option<Duration>,
    state: TransportState,
}
async fn handle_client(mut socket: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    // Simple state for media playback simulation

    let mut state = PlaybackState {
        uri: None,
        metadata: None,
        position: Duration::from_secs(0),
        duration: None,
        state: TransportState::Stopped,
    };

    let mut position_updater = tokio::time::interval(Duration::from_millis(200));
    let mut should_update_position = false;

    loop {
        tokio::select! {
            _ = position_updater.tick(), if should_update_position => {
                if state.state == TransportState::Playing {
                    // Update position if we're playing
                    state.position += Duration::from_millis(200);

                    // Loop back to start if we reach the end
                    if let Some(duration) = state.duration {
                        if state.position > duration {
                            state.position = Duration::from_secs(0);
                        }
                    }
                }
            }
            result = read_command(&mut socket) => {
                match result {
                    Ok(command) => {
                        let response = process_command(command, &mut state, &mut should_update_position).await;

                        if response == MediaResponse::Shutdown {
                            info!("Shutdown command received, closing connection");
                            break;
                        }

                        send_response(&mut socket, response).await?;
                    }
                    Err(e) => {
                        error!("Error reading command: {}", e);
                        break;
                    }
                }
            }
        }
    }

    info!("Client disconnected");
    Ok(())
}

async fn read_command(socket: &mut TcpStream) -> Result<MediaCommand, Box<dyn std::error::Error>> {
    // Read command length
    let mut len_buf = [0u8; 4];
    socket.read_exact(&mut len_buf).await?;
    let cmd_len = u32::from_be_bytes(len_buf) as usize;

    // Read command data
    let mut cmd_buf = vec![0u8; cmd_len];
    socket.read_exact(&mut cmd_buf).await?;

    // Parse command
    let command = serde_json::from_slice(&cmd_buf)?;
    Ok(command)
}

async fn process_command(
    command: MediaCommand,
    state: &mut PlaybackState,
    should_update_position: &mut bool,
) -> MediaResponse {
    match command {
        MediaCommand::Ping => MediaResponse::Pong,
        MediaCommand::Shutdown => MediaResponse::Shutdown,
        MediaCommand::Play => {
            if state.uri.is_none() {
                MediaResponse::Error {
                    message: "No media loaded".to_string(),
                }
            } else {
                state.state = TransportState::Playing;
                *should_update_position = true;
                MediaResponse::Ok
            }
        }
        MediaCommand::Pause => {
            state.state = TransportState::PausedPlayback;
            MediaResponse::Ok
        }
        MediaCommand::Stop => {
            state.state = TransportState::Stopped;
            state.position = Duration::from_secs(0);
            MediaResponse::Ok
        }
        MediaCommand::GetPosition => MediaResponse::Position {
            milliseconds: state.position.as_millis() as u64,
        },
        MediaCommand::GetDuration => MediaResponse::Duration {
            milliseconds: state.duration.map(|d| d.as_millis() as u64),
        },
        MediaCommand::GetState => MediaResponse::State { state: state.state },
        MediaCommand::Load { uri, metadata } => {
            info!("Loading media: {}", uri);
            state.uri = Some(uri);
            state.metadata = metadata;
            state.position = Duration::from_secs(0);

            // For demonstration, set a fake duration
            state.duration = Some(Duration::from_secs(300)); // 5 minutes

            // Reset playback state
            state.state = TransportState::Stopped;
            MediaResponse::Ok
        }
        MediaCommand::Seek { position } => {
            state.position = Duration::from_millis(position);
            MediaResponse::Ok
        }
    }
}

async fn send_response(
    socket: &mut TcpStream,
    response: MediaResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    let response_json = serde_json::to_string(&response)?;
    let response_bytes = response_json.as_bytes();

    // Write length prefix
    socket
        .write_all(&(response_bytes.len() as u32).to_be_bytes())
        .await?;

    // Write response data
    socket.write_all(response_bytes).await?;
    socket.flush().await?;

    Ok(())
}
