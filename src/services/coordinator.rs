use super::types::MediaInfo;
use crate::server::soap::types::av_transport::TransportState;
use crate::services::error::{ConnectionError, TransportError};
use crate::services::{AVTransport, ConnectionManager, ConnectionState, ServiceError};
use futures::future::join_all;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, watch};
use tracing::{debug, error, info, trace, warn};

#[derive(Clone, Debug)]
pub enum ServiceEvent {
    PlaybackStateChanged(TransportState),
    ConnectionStateChanged(ConnectionState),
    MediaChanged(Option<MediaInfo>),
    Error(ServiceError),
}

pub struct ServiceCoordinator {
    connection_manager: Arc<dyn ConnectionManager>,
    av_transport: Arc<dyn AVTransport>,
    event_tx: broadcast::Sender<ServiceEvent>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl ServiceCoordinator {
    pub fn new(
        connection_manager: Arc<dyn ConnectionManager>,
        av_transport: Arc<dyn AVTransport>,
    ) -> Self {
        info!("Initializing ServiceCoordinator");
        let (event_tx, _) = broadcast::channel(100);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        Self {
            connection_manager,
            av_transport,
            event_tx,
            shutdown_tx,
            shutdown_rx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServiceEvent> {
        trace!("New subscriber to ServiceEvents");
        self.event_tx.subscribe()
    }

    async fn broadcast_event(&self, event: ServiceEvent) -> Result<(), ServiceError> {
        trace!(event = ?event, "Broadcasting service event");

        if let Err(e) = self.event_tx.send(event.clone()) {
            let error = match &event {
                ServiceEvent::PlaybackStateChanged(state) => {
                    warn!(state = ?state, error = %e, "Failed to broadcast playback state");
                    ServiceError::Transport(TransportError::Internal(format!(
                        "Failed to broadcast playback state: {}",
                        e
                    )))
                }
                ServiceEvent::ConnectionStateChanged(state) => {
                    warn!(state = ?state, error = %e, "Failed to broadcast connection state");
                    ServiceError::Connection(ConnectionError::ConnectionFailed(format!(
                        "Failed to broadcast connection state: {}",
                        e
                    )))
                }
                _ => {
                    warn!(error = %e, "Failed to broadcast event");
                    ServiceError::Network(format!("Failed to broadcast event: {}", e))
                }
            };
            Err(error)
        } else {
            trace!(event = ?event, "Successfully broadcast event");
            Ok(())
        }
    }

    pub async fn run(&self) -> Result<(), ServiceError> {
        info!("Starting service coordinator");
        let mut shutdown_rx = self.shutdown_rx.clone();

        // Clone required components
        let av_transport = self.av_transport.clone();
        let connection_manager = self.connection_manager.clone();
        let event_tx = self.event_tx.clone();
        let shutdown_tx = self.shutdown_tx.clone();

        let broadcast_event = create_broadcast_function(event_tx);

        let monitors = vec![
            tokio::spawn({
                let shutdown = shutdown_rx.clone();
                let av_transport = av_transport.clone();
                let broadcast = broadcast_event.clone();
                async move { monitor_playback_state(av_transport, broadcast, shutdown).await }
            }),
            tokio::spawn({
                let shutdown = shutdown_rx.clone();
                let broadcast = broadcast_event;
                async move { monitor_connection_state(connection_manager, broadcast, shutdown).await }
            }),
        ];

        let result = tokio::select! {
            ctrl_c_res = tokio::signal::ctrl_c() => {
                handle_ctrl_c(ctrl_c_res, shutdown_tx).await
            }
            monitor_results = join_all(monitors) => {
                handle_monitor_results(monitor_results).await
            }
        };

        match &result {
            Ok(()) => info!("Service coordinator shutdown completed successfully"),
            Err(e) => error!(error = %e, "Service coordinator shutdown with error"),
        }

        result
    }
}

impl Drop for ServiceCoordinator {
    fn drop(&mut self) {
        if let Err(e) = self.shutdown_tx.send(true) {
            error!(error = %e, "Failed to send shutdown signal during coordinator drop");
        } else {
            debug!("Successfully sent shutdown signal during coordinator drop");
        }
    }
}

// Helper functions
fn create_broadcast_function(
    event_tx: broadcast::Sender<ServiceEvent>,
) -> impl Fn(ServiceEvent) -> Pin<Box<dyn Future<Output = Result<(), ServiceError>> + Send>> + Clone
{
    move |event: ServiceEvent| {
        let event_tx = event_tx.clone();
        Box::pin(async move {
            trace!(event = ?event, "Attempting to broadcast event");
            if let Err(e) = event_tx.send(event.clone()) {
                let error = match &event {
                    ServiceEvent::PlaybackStateChanged(state) => {
                        warn!(state = ?state, error = %e, "Failed to broadcast playback state");
                        ServiceError::Transport(TransportError::Internal(format!(
                            "Failed to broadcast playback state: {}",
                            e
                        )))
                    }
                    ServiceEvent::ConnectionStateChanged(state) => {
                        warn!(state = ?state, error = %e, "Failed to broadcast connection state");
                        ServiceError::Connection(ConnectionError::ConnectionFailed(format!(
                            "Failed to broadcast connection state: {}",
                            e
                        )))
                    }
                    _ => {
                        warn!(error = %e, "Failed to broadcast event");
                        ServiceError::Network(format!("Failed to broadcast event: {}", e))
                    }
                };
                Err(error)
            } else {
                trace!(event = ?event, "Successfully broadcast event");
                Ok(())
            }
        })
    }
}

async fn handle_ctrl_c(
    ctrl_c_res: Result<(), std::io::Error>,
    shutdown_tx: watch::Sender<bool>,
) -> Result<(), ServiceError> {
    match ctrl_c_res {
        Ok(()) => {
            info!("Received ctrl-c shutdown signal");
            shutdown_tx.send(true).map_err(|e| {
                error!(error = %e, "Failed to send shutdown signal");
                ServiceError::Network(format!("Failed to send shutdown signal: {}", e))
            })?;
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "Failed to listen for ctrl-c signal");
            Err(ServiceError::Network(format!(
                "Ctrl-c handler error: {}",
                e
            )))
        }
    }
}

async fn handle_monitor_results(
    monitor_results: Vec<Result<Result<(), ServiceError>, tokio::task::JoinError>>,
) -> Result<(), ServiceError> {
    for result in &monitor_results {
        if let Err(e) = result {
            error!(error = %e, "Monitor task panicked");
            return Err(ServiceError::Network(format!("Monitor task panic: {}", e)));
        }
    }

    let first_error = monitor_results
        .into_iter()
        .filter_map(|r| r.ok())
        .find(|r| r.is_err());

    if let Some(Err(e)) = first_error {
        error!(error = %e, "Monitor task failed");
        Err(e)
    } else {
        debug!("All monitor tasks completed successfully");
        Ok(())
    }
}

async fn monitor_playback_state(
    av_transport: Arc<dyn AVTransport>,
    broadcast_event: impl Fn(ServiceEvent) -> Pin<Box<dyn Future<Output = Result<(), ServiceError>> + Send>>
        + Send,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), ServiceError> {
    info!("Starting playback state monitor");
    let mut interval = tokio::time::interval(Duration::from_millis(100));
    let mut last_state = None;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                info!("Playback state monitor received shutdown signal");
                break;
            }
            _ = interval.tick() => {
                let current_state = av_transport.get_state().await?;

                if last_state != Some(current_state) {
                    debug!(
                        previous_state = ?last_state,
                        new_state = ?current_state,
                        "Playback state changed"
                    );
                    broadcast_event(ServiceEvent::PlaybackStateChanged(current_state)).await?;
                    last_state = Some(current_state);
                }
            }
        }
    }

    info!("Playback state monitor shutdown complete");
    Ok(())
}

async fn monitor_connection_state(
    connection_manager: Arc<dyn ConnectionManager>,
    broadcast_event: impl Fn(ServiceEvent) -> Pin<Box<dyn Future<Output = Result<(), ServiceError>> + Send>>
        + Send,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), ServiceError> {
    info!("Starting connection state monitor");
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut last_state = None;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                info!("Connection state monitor received shutdown signal");
                break;
            }
            _ = interval.tick() => {
                let current_state = connection_manager.get_status().await?;

                if last_state != Some(current_state) {
                    debug!(
                        previous_state = ?last_state,
                        new_state = ?current_state,
                        "Connection state changed"
                    );
                    broadcast_event(ServiceEvent::ConnectionStateChanged(current_state)).await?;
                    last_state = Some(current_state);
                }
            }
        }
    }

    info!("Connection state monitor shutdown complete");
    Ok(())
}
