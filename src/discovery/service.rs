use crate::discovery::error::DiscoveryServiceError;
use std::path::PathBuf;
use std::{fmt, net::IpAddr};
use uuid::Uuid;
pub const DEFAULT_SERVER: &str = concat!("Rust/", env!("CARGO_PKG_VERSION"), " UPnP/1.0");

#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub device_type: String,
    pub unique_id: Uuid,
    pub location: String,
    pub server: Server,
    pub services: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Server {
    Default,
    Custom(String),
}

impl Server {
    pub fn as_str(&self) -> &str {
        match self {
            Server::Default => DEFAULT_SERVER,
            Server::Custom(s) => s,
        }
    }
}

impl fmt::Display for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[async_trait::async_trait]
pub trait SsdpService: Send + Sync {
    fn service_info(&self) -> Result<ServiceInfo, DiscoveryServiceError>;
}

#[derive(Debug, Clone)]
pub enum MediaRendererService {
    AVTransport,
    RenderingControl,
    ConnectionManager,
}

impl MediaRendererService {
    pub(crate) fn to_urn(&self) -> String {
        match self {
            Self::AVTransport => "urn:schemas-upnp-org:service:AVTransport:1",
            Self::RenderingControl => "urn:schemas-upnp-org:service:RenderingControl:1",
            Self::ConnectionManager => "urn:schemas-upnp-org:service:ConnectionManager:1",
        }
        .to_string()
    }
}

#[derive(Debug, Clone)]
pub struct MediaRendererConfig {
    pub ip_addr: Option<IpAddr>,
    pub port: u16,
    pub server: Option<String>,
    pub friendly_name: String,
    pub manufacturer: String,
    pub manufacturer_url: String,
    pub model_description: String,
    pub model_name: String,
    pub model_number: String,
    pub model_url: String,
    pub serial_number: String,
    /// Path to a PNG/JPEG/GIF file to serve as the device icon
    /// (`--icon <path>`). No `iconList` is advertised at all when unset.
    pub icon_path: Option<PathBuf>,
}

impl MediaRendererConfig {
    pub fn builder() -> MediaRendererConfigBuilder {
        MediaRendererConfigBuilder::default()
    }

    pub(crate) fn default_services() -> Vec<MediaRendererService> {
        vec![
            MediaRendererService::AVTransport,
            MediaRendererService::RenderingControl,
            MediaRendererService::ConnectionManager,
        ]
    }
}

impl Default for MediaRendererConfig {
    fn default() -> Self {
        Self {
            ip_addr: None,
            port: 8080,
            server: None,
            friendly_name: "JonoRust Media Renderer".to_string(),
            manufacturer: "JonoRust UPnP".to_string(),
            manufacturer_url: "https://github.com/JonathanNakandala/rust-media-upnp-renderer"
                .to_string(),
            model_description: "UPnP Media Renderer".to_string(),
            model_name: "The Rust Renderer".to_string(),
            model_number: "1.0.0".to_string(),
            model_url: "https://github.com/yourusername/project".to_string(),
            serial_number: "1".to_string(),
            icon_path: None,
        }
    }
}

#[derive(Default)]
pub struct MediaRendererConfigBuilder {
    config: MediaRendererConfig,
}

impl MediaRendererConfigBuilder {
    pub fn ip_addr(mut self, ip_addr: IpAddr) -> Self {
        self.config.ip_addr = Some(ip_addr);
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.config.port = port;
        self
    }

    pub fn server(mut self, server: String) -> Self {
        self.config.server = Some(server);
        self
    }

    pub fn friendly_name(mut self, name: String) -> Self {
        self.config.friendly_name = name;
        self
    }

    pub fn manufacturer(mut self, manufacturer: String) -> Self {
        self.config.manufacturer = manufacturer;
        self
    }

    pub fn manufacturer_url(mut self, url: String) -> Self {
        self.config.manufacturer_url = url;
        self
    }

    pub fn model_description(mut self, description: String) -> Self {
        self.config.model_description = description;
        self
    }

    pub fn model_name(mut self, name: String) -> Self {
        self.config.model_name = name;
        self
    }

    pub fn model_number(mut self, number: String) -> Self {
        self.config.model_number = number;
        self
    }

    pub fn model_url(mut self, url: String) -> Self {
        self.config.model_url = url;
        self
    }

    pub fn serial_number(mut self, number: String) -> Self {
        self.config.serial_number = number;
        self
    }

    pub fn icon_path(mut self, path: PathBuf) -> Self {
        self.config.icon_path = Some(path);
        self
    }

    pub fn build(self) -> MediaRendererConfig {
        self.config
    }
}
