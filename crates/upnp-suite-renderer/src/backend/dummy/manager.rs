use std::env;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{error, info, warn};

use super::error::DummyProcessError;
use super::protocol::{MediaCommand, MediaResponse};
use crate::server::soap::types::av_transport::TransportState;

pub type Result<T> = std::result::Result<T, DummyProcessError>;

pub struct DummyBackendManager {
    child_process: Option<Child>,
    connection: Option<TcpStream>,
    address: String,
    last_heartbeat: Instant,
    heartbeat_timeout: Duration,
    restart_attempts: u32,

    // Current media state
    current_uri: Option<String>,
    current_metadata: Option<String>,
    playback_state: TransportState,
    is_playing: bool,

    // Configuration
    max_restart_attempts: u32,
    executable_path: String,
}

async fn send_command(stream: &mut TcpStream, command: MediaCommand) -> Result<MediaResponse> {
    let cmd_json = serde_json::to_string(&command)?;
    let cmd_bytes = cmd_json.as_bytes();

    // Send length prefix (u32) and then data
    let len: u32 = cmd_bytes.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(cmd_bytes).await?;
    stream.flush().await?;

    // Read response length
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let response_len = u32::from_be_bytes(len_buf) as usize;

    // Read response
    let mut response_buf = vec![0u8; response_len];
    stream.read_exact(&mut response_buf).await?;

    // Deserialize response
    let response: MediaResponse = serde_json::from_slice(&response_buf)?;

    Ok(response)
}

impl Default for DummyBackendManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DummyBackendManager {
    pub fn new() -> Self {
        Self {
            child_process: None,
            connection: None,
            address: String::new(),
            last_heartbeat: Instant::now(),
            heartbeat_timeout: Duration::from_secs(5),
            restart_attempts: 0,
            current_uri: None,
            current_metadata: None,
            playback_state: TransportState::Stopped,
            is_playing: false,
            max_restart_attempts: 10,
            executable_path: env::current_exe()
                .unwrap_or_else(|_| "backend".into())
                .display()
                .to_string(),
        }
    }

    pub fn with_executable_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.executable_path = path.as_ref().display().to_string();
        self
    }

    pub fn with_heartbeat_timeout(mut self, timeout: Duration) -> Self {
        self.heartbeat_timeout = timeout;
        self
    }

    pub fn with_max_restart_attempts(mut self, attempts: u32) -> Self {
        self.max_restart_attempts = attempts;
        self
    }

    pub async fn start(&mut self) -> Result<()> {
        if self.child_process.is_some() {
            return Ok(()); // Already started
        }

        self.spawn_new_process().await
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        // Try graceful shutdown first
        if let Some(conn) = &mut self.connection {
            if let Err(e) = send_command(conn, MediaCommand::Shutdown).await {
                warn!("Failed to send shutdown command: {}", e);
            }
        }

        if let Some(mut child) = self.child_process.take() {
            // First try a non-blocking wait to see if it's already done
            match child.try_wait() {
                Ok(Some(status)) => {
                    info!("Media process already exited with status: {}", status);
                }
                _ => {
                    // Not yet exited, try with timeout
                    let wait_future = tokio::task::spawn_blocking(move || child.wait());
                    match tokio::time::timeout(Duration::from_secs(1), wait_future).await {
                        Ok(Ok(Ok(status))) => {
                            info!("Media process exited with status: {}", status);
                        }
                        _ => {
                            // Timeout or error - need to force kill
                            // But child is moved, so we need to spawn a new process
                            info!("Killing media process");

                            #[cfg(unix)]
                            {
                                // For Unix, we can use the PID from /proc
                                use nix::sys::signal::{kill, Signal};
                                use nix::unistd::Pid;
                                use std::fs::read_dir;

                                // Find and kill matching child processes
                                if let Ok(entries) = read_dir("/proc") {
                                    for entry in entries.filter_map(|res| res.ok()) {
                                        if let Ok(pid_str) = entry.file_name().into_string() {
                                            if let Ok(pid) = pid_str.parse::<i32>() {
                                                // Optional: check if it's our child by reading /proc/{pid}/ppid
                                                // For now, we'll rely on the fact that we're cleaning up only once
                                                let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
                                            }
                                        }
                                    }
                                }

                                // Wait a bit and then try SIGKILL for any remaining
                                tokio::time::sleep(Duration::from_millis(100)).await;
                                // Similar loop with SIGKILL if needed
                            }

                            #[cfg(windows)]
                            {
                                // On Windows, this more complex approach isn't needed
                                // Windows handles are managed differently
                                warn!("Child process timeout on Windows - may need manual cleanup");
                            }
                        }
                    }
                }
            }
        }

        // Clean up connection
        if let Some(conn) = self.connection.take() {
            drop(conn);
        }

        self.address.clear();
        Ok(())
    }
    pub async fn monitor_health(&mut self) -> Result<()> {
        // 1. Check if process has terminated
        if let Some(child) = &mut self.child_process {
            match child.try_wait() {
                Ok(Some(status)) => {
                    warn!("Media process exited with status: {}", status);
                    return self.restart_process().await;
                }
                Err(e) => {
                    error!("Error checking process status: {}", e);
                    return self.restart_process().await;
                }
                _ => {} // Still running
            }
        } else {
            // No child process, try to start one
            return self.restart_process().await;
        }

        // 2. Check time since last heartbeat
        if self.last_heartbeat.elapsed() > self.heartbeat_timeout {
            warn!(
                "Media process heartbeat timeout after {:?}",
                self.heartbeat_timeout
            );
            return self.restart_process().await;
        }

        // 3. Send a ping and expect response with timeout
        if let Some(conn) = &mut self.connection {
            match timeout(
                Duration::from_secs(2),
                send_command(conn, MediaCommand::Ping),
            )
            .await
            {
                Ok(Ok(MediaResponse::Pong)) => {
                    // Ping successful, update heartbeat
                    self.last_heartbeat = Instant::now();
                }
                _ => {
                    warn!("Media process failed to respond to ping");
                    return self.restart_process().await;
                }
            }
        } else {
            // No connection, try to restart
            return self.restart_process().await;
        }

        Ok(())
    }

    async fn restart_process(&mut self) -> Result<()> {
        // Check if we've hit the max restart attempts
        if self.restart_attempts >= self.max_restart_attempts {
            return Err(DummyProcessError::MaxRestartAttempts(
                self.max_restart_attempts,
            ));
        }

        // 1. Kill existing process if it exists
        if let Some(mut child) = self.child_process.take() {
            info!("Terminating unresponsive media process");

            // Try graceful shutdown first
            if let Some(conn) = &mut self.connection {
                // Try to send shutdown command with timeout
                let _ = timeout(
                    Duration::from_millis(500),
                    send_command(conn, MediaCommand::Shutdown),
                )
                .await;
            }

            // Force kill if still running
            if let Ok(None) = child.try_wait() {
                // Process still running, kill it
                #[cfg(unix)]
                {
                    use nix::sys::signal::{kill, Signal};
                    use nix::unistd::Pid;

                    // Try SIGTERM first
                    let _ = kill(Pid::from_raw(child.id() as i32), Signal::SIGTERM);

                    // Wait a bit
                    tokio::time::sleep(Duration::from_millis(100)).await;

                    // If still running, use SIGKILL
                    if child.try_wait().map(|s| s.is_none()).unwrap_or(false) {
                        let _ = kill(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
                    }
                }

                #[cfg(windows)]
                {
                    let _ = child.kill();
                }
            }

            // Ensure connection is closed
            if let Some(conn) = self.connection.take() {
                drop(conn);
            }
        }

        // 2. Apply exponential backoff for restarts
        let backoff = self.calculate_backoff();
        if backoff > Duration::ZERO {
            info!("Waiting {:?} before restarting media process", backoff);
            tokio::time::sleep(backoff).await;
        }

        // 3. Start new process
        info!(
            "Starting new media process (attempt {})",
            self.restart_attempts + 1
        );
        match self.spawn_new_process().await {
            Ok(_) => {
                info!("Successfully restarted media process");
                self.restart_attempts = 0;
                self.last_heartbeat = Instant::now();
                Ok(())
            }
            Err(e) => {
                self.restart_attempts += 1;
                error!("Failed to restart media process: {}", e);
                Err(e)
            }
        }
    }

    fn calculate_backoff(&self) -> Duration {
        if self.restart_attempts == 0 {
            Duration::ZERO
        } else {
            let backoff_secs = (2_u64.pow(self.restart_attempts.min(6))).min(60);
            Duration::from_secs(backoff_secs)
        }
    }

    async fn spawn_new_process(&mut self) -> Result<()> {
        // Find available port
        let port = self.find_available_port()?;
        let address = format!("127.0.0.1:{}", port);

        info!(port, "Spawning media backend process");
        let child = Command::new(&self.executable_path)
            .arg("--backend")
            .arg(format!("{}", port))
            .spawn()?;

        // Wait for the process to start up
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Try to connect with timeout and retries
        let mut attempts = 0;
        let connection = loop {
            match timeout(Duration::from_secs(1), TcpStream::connect(&address)).await {
                Ok(Ok(conn)) => break conn,
                _ if attempts < 5 => {
                    attempts += 1;
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                _ => {
                    return Err(DummyProcessError::Connection(
                        "Failed to connect to media process after 5 attempts".into(),
                    ));
                }
            }
        };

        // Store new process info
        self.child_process = Some(child);
        self.connection = Some(connection);
        self.address = address;

        // Initialize the process state
        self.reinitialize_state().await?;

        Ok(())
    }

    async fn reinitialize_state(&mut self) -> Result<()> {
        // Clone the data we need first to release the borrow
        let uri_clone = self.current_uri.clone();
        let metadata_clone = self.current_metadata.clone();
        let was_playing = self.is_playing;

        // Now use the cloned data
        if let Some(uri) = uri_clone {
            // No borrow conflict because we're using cloned data
            self.load_uri(&uri, metadata_clone).await?;

            // Restore playback state if needed
            if was_playing {
                self.play().await?;
            }
        }

        Ok(())
    }

    fn find_available_port(&self) -> Result<u16> {
        // Bind to port 0 to let the OS choose an available port
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        Ok(port)
    }

    // Public API for controlling media

    pub async fn load_uri(&mut self, uri: &str, metadata: Option<String>) -> Result<()> {
        if let Some(conn) = &mut self.connection {
            match send_command(
                conn,
                MediaCommand::Load {
                    uri: uri.to_string(),
                    metadata: metadata.clone(),
                },
            )
            .await?
            {
                MediaResponse::Ok => {
                    self.current_uri = Some(uri.to_string());
                    self.current_metadata = metadata;
                    self.is_playing = false;
                    self.playback_state = TransportState::Stopped;
                    Ok(())
                }
                MediaResponse::Error { message } => Err(DummyProcessError::CommandFailed(message)),
                _ => Err(DummyProcessError::UnexpectedResponse(
                    "when loading media".into(),
                )),
            }
        } else {
            Err(DummyProcessError::NoConnection)
        }
    }

    pub async fn get_uri(&self) -> Option<String> {
        self.current_uri.clone()
    }
    pub async fn get_current_metadata(&self) -> Option<String> {
        self.current_metadata.clone()
    }
    pub async fn play(&mut self) -> Result<()> {
        if let Some(conn) = &mut self.connection {
            match send_command(conn, MediaCommand::Play).await? {
                MediaResponse::Ok => {
                    self.is_playing = true;
                    self.playback_state = TransportState::Playing;
                    Ok(())
                }
                MediaResponse::NoMediaLoaded => Err(DummyProcessError::NoMediaLoaded),
                MediaResponse::Error { message } => Err(DummyProcessError::CommandFailed(message)),
                _ => Err(DummyProcessError::UnexpectedResponse("when playing".into())),
            }
        } else {
            Err(DummyProcessError::NoConnection)
        }
    }

    pub async fn pause(&mut self) -> Result<()> {
        if let Some(conn) = &mut self.connection {
            match send_command(conn, MediaCommand::Pause).await? {
                MediaResponse::Ok => {
                    self.is_playing = false;
                    self.playback_state = TransportState::PausedPlayback;
                    Ok(())
                }
                MediaResponse::Error { message } => Err(DummyProcessError::CommandFailed(message)),
                _ => Err(DummyProcessError::UnexpectedResponse("when pausing".into())),
            }
        } else {
            Err(DummyProcessError::NoConnection)
        }
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(conn) = &mut self.connection {
            match send_command(conn, MediaCommand::Stop).await? {
                MediaResponse::Ok => {
                    self.is_playing = false;
                    self.playback_state = TransportState::Stopped;
                    Ok(())
                }
                MediaResponse::Error { message } => Err(DummyProcessError::CommandFailed(message)),
                _ => Err(DummyProcessError::UnexpectedResponse("when stopping".into())),
            }
        } else {
            Err(DummyProcessError::NoConnection)
        }
    }

    pub async fn seek(&mut self, position_ms: u64) -> Result<()> {
        if let Some(conn) = &mut self.connection {
            match send_command(
                conn,
                MediaCommand::Seek {
                    position: position_ms,
                },
            )
            .await?
            {
                MediaResponse::Ok => Ok(()),
                MediaResponse::Error { message } => Err(DummyProcessError::CommandFailed(message)),
                _ => Err(DummyProcessError::UnexpectedResponse("when seeking".into())),
            }
        } else {
            Err(DummyProcessError::NoConnection)
        }
    }

    pub async fn get_position(&mut self) -> Result<Duration> {
        if let Some(conn) = &mut self.connection {
            match send_command(conn, MediaCommand::GetPosition).await? {
                MediaResponse::Position { milliseconds } => Ok(Duration::from_millis(milliseconds)),
                MediaResponse::Error { message } => Err(DummyProcessError::CommandFailed(format!(
                    "Error getting position: {}",
                    message
                ))),
                _ => Err(DummyProcessError::UnexpectedResponse(
                    "when getting position".into(),
                )),
            }
        } else {
            Err(DummyProcessError::NoConnection)
        }
    }

    pub async fn get_duration(&mut self) -> Result<Option<Duration>> {
        if let Some(conn) = &mut self.connection {
            match send_command(conn, MediaCommand::GetDuration).await? {
                MediaResponse::Duration { milliseconds } => {
                    Ok(milliseconds.map(Duration::from_millis))
                }
                MediaResponse::Error { message } => Err(DummyProcessError::CommandFailed(format!(
                    "Error getting position: {}",
                    message
                ))),
                _ => Err(DummyProcessError::UnexpectedResponse(
                    "when getting duration".into(),
                )),
            }
        } else {
            Err(DummyProcessError::NoConnection)
        }
    }

    pub async fn get_state(&mut self) -> Result<TransportState> {
        if let Some(conn) = &mut self.connection {
            match send_command(conn, MediaCommand::GetState).await? {
                MediaResponse::State { state } => {
                    self.playback_state = state;
                    Ok(state)
                }
                MediaResponse::Error { message } => Err(DummyProcessError::CommandFailed(format!(
                    "Error getting playback state: {}",
                    message
                ))),
                _ => Err(DummyProcessError::UnexpectedResponse(
                    "when getting playback state".into(),
                )),
            }
        } else {
            Err(DummyProcessError::NoConnection)
        }
    }

    // Function to run the monitor in a loop
    pub async fn run_monitor_task(manager: std::sync::Arc<Mutex<Self>>) {
        let mut interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            interval.tick().await;

            let result = {
                let mut lock = manager.lock().await;
                lock.monitor_health().await
            };

            if let Err(e) = result {
                error!("Monitor task error: {}", e);
                // If we've hit max retries or have a critical error, we might want to notify
                // the main application or take other actions
            }
        }
    }
}

impl Drop for DummyBackendManager {
    fn drop(&mut self) {
        // Best effort cleanup when the manager is dropped
        if let Some(mut child) = self.child_process.take() {
            let _ = child.kill();
        }
    }
}
