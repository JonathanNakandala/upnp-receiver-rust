use std::error::Error;
use std::net::IpAddr;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::backend::{MediaBackend, MediaBackendAVTransport, MediaBackendRenderingControl};
use crate::discovery::error::DiscoveryServiceError;
use crate::discovery::service::{
    MediaRendererConfig, MediaRendererService, Server, ServiceInfo, SsdpService,
};
use crate::ip::get_default_ip;
use crate::server::eventing::{av_transport_last_change, AVTransportEventState, EventManager};
use crate::server::{load_icon, DeviceIcon};
use crate::services::{
    av_transport::{AVTransport, AVTransportImpl},
    connection_manager::{ConnectionManager, DummyConnectionManager},
    rendering_control::{RenderingControl, RenderingControlImpl},
};

pub struct MediaRenderer {
    // Discovery/Config fields
    unique_id: Uuid,
    ip_addr: IpAddr,
    port: u16,
    server: Server,
    friendly_name: String,
    manufacturer: String,
    manufacturer_url: String,
    model_description: String,
    model_name: String,
    model_number: String,
    model_url: String,
    serial_number: String,
    services: Vec<MediaRendererService>,
    icon: Arc<Option<DeviceIcon>>,

    // Service implementations
    pub av_transport: Arc<dyn AVTransport>,
    pub rendering_control: Arc<dyn RenderingControl>,
    pub connection_manager: Arc<dyn ConnectionManager>,

    // GENA subscribers, one manager per evented service.
    pub av_transport_events: Arc<EventManager>,
    pub rendering_control_events: Arc<EventManager>,
    pub connection_manager_events: Arc<EventManager>,

    // Media backend (optional) — whichever `MediaBackend` implementation
    // main.rs selected (GStreamer, dummy, or none).
    media_backend: Option<Arc<dyn MediaBackend>>,
}
impl MediaRenderer {
    pub fn new(config: MediaRendererConfig) -> Result<Self, Box<dyn Error>> {
        // Create renderer without media capabilities
        Self::create_renderer(config, None, vec!["audio/mpeg".to_string()])
    }

    /// `supported_mime_types` becomes the `protocolInfo` sink entries
    /// `ConnectionManager::GetProtocolInfo` reports — one `http-get:*:<mime>:*`
    /// per entry — so callers should pass whatever the chosen `media_backend`
    /// can actually decode (e.g.
    /// [`gstreamer::supported_mime_types`](crate::backend::gstreamer::supported_mime_types)).
    pub fn new_with_media_backend(
        config: MediaRendererConfig,
        media_backend: Option<Arc<dyn MediaBackend>>,
        supported_mime_types: Vec<String>,
    ) -> Result<Self, Box<dyn Error>> {
        Self::create_renderer(config, media_backend, supported_mime_types)
    }

    // Private helper to create the renderer with or without media backend
    fn create_renderer(
        config: MediaRendererConfig,
        media_backend: Option<Arc<dyn MediaBackend>>,
        supported_mime_types: Vec<String>,
    ) -> Result<Self, Box<dyn Error>> {
        let ip_addr = match config.ip_addr {
            Some(ip) => ip,
            None => {
                let detected_ip = get_default_ip()?;
                info!(ip = %detected_ip, "No IP address specified, using detected IP");
                detected_ip
            }
        };

        // Loading the icon is best-effort: a bad --icon path shouldn't stop
        // the renderer from starting, just leave it without one (no
        // `iconList` advertised at all).
        let icon = config.icon_path.as_ref().and_then(|path| {
            match load_icon(path) {
                Ok(icon) => Some(icon),
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "Failed to load device icon, continuing without one");
                    None
                }
            }
        });

        // Initialize services based on whether we have a media backend
        let (av_transport, rendering_control) = if let Some(backend) = media_backend.clone() {
            info!("Using media backend for AV transport and rendering control");
            (
                Arc::new(MediaBackendAVTransport::new(backend.clone())) as Arc<dyn AVTransport>,
                Arc::new(MediaBackendRenderingControl::new(backend.clone()))
                    as Arc<dyn RenderingControl>,
            )
        } else {
            warn!("No media backend available, using fallback implementations");
            (
                Arc::new(AVTransportImpl::new()) as Arc<dyn AVTransport>,
                Arc::new(RenderingControlImpl::new()) as Arc<dyn RenderingControl>,
            )
        };

        let connection_manager = Arc::new(DummyConnectionManager::with_supported_mime_types(
            supported_mime_types,
        ));
        let av_transport_events = EventManager::new();
        let rendering_control_events = EventManager::new();
        let connection_manager_events = EventManager::new();

        // Forward transport-state changes the backend makes on its own
        // (reaching end-of-stream, hitting a decode error) to subscribed
        // control points immediately, rather than leaving them to find out
        // on their next GetTransportInfo poll. Caller-driven changes
        // (Play/Pause/Stop) already get a LastChange published right after
        // the SOAP handler that caused them returns — this only ever fires
        // for the ones nothing else published.
        if let Some(backend) = &media_backend {
            let mut state_changes = backend.state_changes();
            let events = av_transport_events.clone();
            let av_transport_for_state = av_transport.clone();
            tokio::spawn(async move {
                while state_changes.changed().await.is_ok() {
                    let event = AVTransportEventState::snapshot(av_transport_for_state.as_ref()).await;
                    events.broadcast(av_transport_last_change(&event));
                }
            });

            // Same idea, but for a gapless track promotion — the transport
            // state doesn't change across one (it stays Playing), so
            // state_changes above wouldn't fire; this is what lets a
            // subscribed control point see AVTransportURI/NextAVTransportURI
            // update without polling GetMediaInfo.
            let mut track_changes = backend.track_changes();
            let events = av_transport_events.clone();
            let av_transport_for_track = av_transport.clone();
            tokio::spawn(async move {
                while track_changes.changed().await.is_ok() {
                    let event = AVTransportEventState::snapshot(av_transport_for_track.as_ref()).await;
                    events.broadcast(av_transport_last_change(&event));
                }
            });
        }

        Ok(Self {
            unique_id: Uuid::new_v4(),
            ip_addr,
            port: config.port,
            server: config.server.map(Server::Custom).unwrap_or(Server::Default),
            friendly_name: config.friendly_name,
            manufacturer: config.manufacturer,
            manufacturer_url: config.manufacturer_url,
            model_description: config.model_description,
            model_name: config.model_name,
            model_number: config.model_number,
            model_url: config.model_url,
            serial_number: config.serial_number,
            services: MediaRendererConfig::default_services(),
            icon: Arc::new(icon),
            // Set service implementations
            av_transport,
            rendering_control,
            connection_manager,
            av_transport_events,
            rendering_control_events,
            connection_manager_events,
            media_backend,
        })
    }
    pub fn has_media_capabilities(&self) -> bool {
        self.media_backend.is_some()
    }
    pub fn ip_addr(&self) -> IpAddr {
        self.ip_addr
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn friendly_name(&self) -> &str {
        &self.friendly_name
    }

    pub fn manufacturer(&self) -> &str {
        &self.manufacturer
    }

    pub fn manufacturer_url(&self) -> &str {
        &self.manufacturer_url
    }

    pub fn model_description(&self) -> &str {
        &self.model_description
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn model_number(&self) -> &str {
        &self.model_number
    }

    pub fn model_url(&self) -> &str {
        &self.model_url
    }

    pub fn serial_number(&self) -> &str {
        &self.serial_number
    }

    pub fn unique_id(&self) -> Uuid {
        self.unique_id
    }

    pub fn icon(&self) -> Option<&DeviceIcon> {
        self.icon.as_ref().as_ref()
    }
}
impl Clone for MediaRenderer {
    fn clone(&self) -> Self {
        Self {
            unique_id: self.unique_id,
            ip_addr: self.ip_addr,
            port: self.port,
            server: self.server.clone(),
            friendly_name: self.friendly_name.clone(),
            manufacturer: self.manufacturer.clone(),
            manufacturer_url: self.manufacturer_url.clone(),
            model_description: self.model_description.clone(),
            model_name: self.model_name.clone(),
            model_number: self.model_number.clone(),
            model_url: self.model_url.clone(),
            serial_number: self.serial_number.clone(),
            services: self.services.clone(),
            icon: self.icon.clone(),
            // Clone the Arc service implementations
            av_transport: self.av_transport.clone(),
            rendering_control: self.rendering_control.clone(),
            connection_manager: self.connection_manager.clone(),
            av_transport_events: self.av_transport_events.clone(),
            rendering_control_events: self.rendering_control_events.clone(),
            connection_manager_events: self.connection_manager_events.clone(),
            media_backend: self.media_backend.clone(),
        }
    }
}

impl SsdpService for MediaRenderer {
    fn service_info(&self) -> Result<ServiceInfo, DiscoveryServiceError> {
        Ok(ServiceInfo {
            device_type: "urn:schemas-upnp-org:device:MediaRenderer:1".to_string(),
            unique_id: self.unique_id,
            location: format!("http://{}:{}/description.xml", self.ip_addr, self.port),
            server: self.server.clone(),
            services: self.services.iter().map(|s| s.to_urn()).collect(),
        })
    }
}
