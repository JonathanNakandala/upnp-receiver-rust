pub mod client;
pub mod description;
pub(crate) mod error;
mod messages;
mod request;
mod search;
pub(crate) mod service;
mod ssdp;

pub use error::DiscoveryServiceError;
pub use service::MediaRendererConfig;
pub use ssdp::SsdpAdvertiser;

/// The well-known SSDP multicast group + port every device and control
/// point sends to and listens on (UPnP Device Architecture §1.2.2).
pub(crate) const SSDP_MULTICAST_ADDR: &str = "239.255.255.250:1900";
