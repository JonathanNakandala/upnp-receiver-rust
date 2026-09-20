use crate::discovery::{
    error::DiscoveryServiceError,
    messages::SsdpMessage,
    service::{ServiceInfo, SsdpService},
};
use std::time::Duration;
use tracing::debug;

use super::messages::SsdpHeader;


#[derive(Debug)]
pub enum SearchTarget {
    All,
    RootDevice,
    Service(String),
    Device(String),
}

impl SearchTarget {
    pub fn from_str(search_target: &str) -> Self {
        match search_target {
            "ssdp:all" => Self::All,
            "upnp:rootdevice" => Self::RootDevice,
            target if target.contains(":service:") => Self::Service(target.to_string()),
            target if target.contains(":device:") => Self::Device(target.to_string()),
            target if !target.starts_with("urn:") => {
                Self::Service(format!("urn:schemas-upnp-org:service:{}", target))
            }
            target => {
                debug!(target = %target, "Unknown search target type, treating as device");
                Self::Device(target.to_string())
            }
        }
    }
}

pub struct SearchHandler {
    notify_interval: u64,
}

impl SearchHandler {
    pub fn new(notify_interval: u64) -> Self {
        Self { notify_interval }
    }

    fn create_response_header(&self, info: &ServiceInfo) -> SsdpHeader {
        SsdpHeader {
            location: info.location.clone(),
            server: info.server.to_string(),
            cache_control: Duration::from_secs(self.notify_interval * 2),
            unique_id: info.unique_id,
        }
    }

    fn create_response(&self, search_target: &str, info: &ServiceInfo) -> SsdpMessage {
        SsdpMessage::new_response(
            self.create_response_header(info),
            search_target.to_string(),
            true,
        )
    }

    pub fn handle_target(
        &self,
        target: SearchTarget,
        services: &[Box<dyn SsdpService>],
    ) -> Result<Vec<SsdpMessage>, DiscoveryServiceError> {
        match target {
            SearchTarget::All => self.handle_all(services),
            SearchTarget::RootDevice => self.handle_root_device(services),
            SearchTarget::Service(service) => self.handle_service(services, &service),
            SearchTarget::Device(device) => self.handle_device(services, &device),
        }
    }

    fn handle_all(
        &self,
        services: &[Box<dyn SsdpService>],
    ) -> Result<Vec<SsdpMessage>, DiscoveryServiceError> {
        debug!("Processing ssdp:all search");
        let mut responses = Vec::new();

        for service in services {
            let info = service.service_info()?;
            debug!(?info.services, "Available services for ssdp:all");

            // Root device response
            responses.push(self.create_response("upnp:rootdevice", &info));

            // Device type response
            responses.push(self.create_response(&info.device_type, &info));

            // Service responses
            for service_type in &info.services {
                responses.push(self.create_response(service_type, &info));
            }
        }

        Ok(responses)
    }

    fn handle_root_device(
        &self,
        services: &[Box<dyn SsdpService>],
    ) -> Result<Vec<SsdpMessage>, DiscoveryServiceError> {
        debug!("Processing root device search");
        let mut responses = Vec::new();

        for service in services {
            let info = service.service_info()?;
            responses.push(self.create_response("upnp:rootdevice", &info));
        }

        Ok(responses)
    }

    fn handle_service(
        &self,
        services: &[Box<dyn SsdpService>],
        service_type: &str,
    ) -> Result<Vec<SsdpMessage>, DiscoveryServiceError> {
        debug!(service = %service_type, "Processing service search");
        let mut responses = Vec::new();

        for service in services {
            let info = service.service_info()?;
            if info.services.contains(&service_type.to_string()) {
                debug!(service = %service_type, "Found matching service");
                responses.push(self.create_response(service_type, &info));
                break;
            }
        }

        Ok(responses)
    }

    fn handle_device(
        &self,
        services: &[Box<dyn SsdpService>],
        device_type: &str,
    ) -> Result<Vec<SsdpMessage>, DiscoveryServiceError> {
        debug!(device = %device_type, "Processing device search");
        let mut responses = Vec::new();

        for service in services {
            let info = service.service_info()?;
            if info.device_type == device_type {
                debug!(device = %device_type, "Found matching device");
                responses.push(self.create_response(device_type, &info));
                break;
            }
        }

        Ok(responses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_target_parsing() {
        assert!(matches!(
            SearchTarget::from_str("ssdp:all"),
            SearchTarget::All
        ));

        assert!(matches!(
            SearchTarget::from_str("upnp:rootdevice"),
            SearchTarget::RootDevice
        ));

        if let SearchTarget::Service(service) = SearchTarget::from_str("ConnectionManager:1") {
            assert_eq!(service, "urn:schemas-upnp-org:service:ConnectionManager:1");
        } else {
            panic!("Expected Service variant");
        }

        if let SearchTarget::Device(device) =
            SearchTarget::from_str("urn:schemas-upnp-org:device:MediaRenderer:1")
        {
            assert_eq!(device, "urn:schemas-upnp-org:device:MediaRenderer:1");
        } else {
            panic!("Expected Device variant");
        }
    }
}
