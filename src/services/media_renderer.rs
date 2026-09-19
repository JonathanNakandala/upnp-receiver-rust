use std::error::Error;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

use crate::backend::{
    MediaBackend, MediaBackendAVTransport, MediaProcessBackend, MediaProcessManager,
};
use crate::discovery::error::DiscoveryServiceError;
use crate::discovery::service::{
    MediaRendererConfig, MediaRendererService, Server, ServiceInfo, SsdpService,
};
use crate::ip::get_default_ip;
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

    // Service implementations
    pub av_transport: Arc<dyn AVTransport>,
    pub rendering_control: Arc<dyn RenderingControl>,
    pub connection_manager: Arc<dyn ConnectionManager>,

    // Media process manager (optional)
    media_manager: Option<Arc<Mutex<MediaProcessManager>>>,
}
impl MediaRenderer {
    pub fn new(config: MediaRendererConfig) -> Result<Self, Box<dyn Error>> {
        // Create renderer without media capabilities
        Self::create_renderer(config, None)
    }

    pub fn new_with_media_backend(
        config: MediaRendererConfig,
        media_manager: Option<Arc<Mutex<MediaProcessManager>>>,
    ) -> Result<Self, Box<dyn Error>> {
        Self::create_renderer(config, media_manager)
    }

    // Private helper to create the renderer with or without media backend
    fn create_renderer(
        config: MediaRendererConfig,
        media_manager: Option<Arc<Mutex<MediaProcessManager>>>,
    ) -> Result<Self, Box<dyn Error>> {
        let ip_addr = match config.ip_addr {
            Some(ip) => ip,
            None => {
                let detected_ip = get_default_ip()?;
                info!(ip = %detected_ip, "No IP address specified, using detected IP");
                detected_ip
            }
        };

        // Create the media backend if we have a manager
        let media_backend = media_manager
            .clone()
            .map(|mgr| Arc::new(MediaProcessBackend::new(mgr)) as Arc<dyn MediaBackend>);

        // Initialize services based on whether we have a media backend
        let (av_transport, rendering_control) = if let Some(backend) = media_backend.clone() {
            info!("Using media backend for AV transport and rendering control");
            (
                Arc::new(MediaBackendAVTransport::new(backend.clone())) as Arc<dyn AVTransport>,
                Arc::new(RenderingControlImpl::new()) as Arc<dyn RenderingControl>, // Keep simple impl for now
            )
        } else {
            warn!("No media backend available, using fallback implementations");
            (
                Arc::new(AVTransportImpl::new()) as Arc<dyn AVTransport>,
                Arc::new(RenderingControlImpl::new()) as Arc<dyn RenderingControl>,
            )
        };

        let connection_manager = Arc::new(DummyConnectionManager::new());

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
            // Set service implementations
            av_transport,
            rendering_control,
            connection_manager,
            // Store the media manager
            media_manager,
        })
    }
    pub fn has_media_capabilities(&self) -> bool {
        self.media_manager.is_some()
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
            // Clone the Arc service implementations
            av_transport: self.av_transport.clone(),
            rendering_control: self.rendering_control.clone(),
            connection_manager: self.connection_manager.clone(),
            // Clone the media manager if it exists
            media_manager: self.media_manager.clone(),
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
pub struct MediaProcessAVTransport {
    media_manager: Arc<Mutex<MediaProcessManager>>,
}

impl MediaProcessAVTransport {
    pub fn new(media_manager: Arc<Mutex<MediaProcessManager>>) -> Self {
        Self { media_manager }
    }
}
