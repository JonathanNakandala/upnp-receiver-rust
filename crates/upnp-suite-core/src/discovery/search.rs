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
    Uuid(String),
    Service(String),
    Device(String),
}

impl SearchTarget {
    pub fn from_str(search_target: &str) -> Self {
        match search_target {
            "ssdp:all" => Self::All,
            "upnp:rootdevice" => Self::RootDevice,
            target if target.starts_with("uuid:") => Self::Uuid(target.to_string()),
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
            SearchTarget::Uuid(uuid) => self.handle_uuid(services, &uuid),
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

            // Device uuid response
            responses.push(self.create_response(&format!("uuid:{}", info.unique_id), &info));

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

    fn handle_uuid(
        &self,
        services: &[Box<dyn SsdpService>],
        search_target: &str,
    ) -> Result<Vec<SsdpMessage>, DiscoveryServiceError> {
        debug!(target = %search_target, "Processing uuid search");
        let mut responses = Vec::new();

        for service in services {
            let info = service.service_info()?;
            let uuid = format!("uuid:{}", info.unique_id);
            if uuid.eq_ignore_ascii_case(search_target) {
                debug!(target = %search_target, "Found matching device uuid");
                responses.push(self.create_response(&uuid, &info));
                break;
            }
        }

        Ok(responses)
    }

    /// Whether an `advertised` service/device URN (what this device actually
    /// implements, e.g. `...:ConnectionManager:3`) satisfies a `requested`
    /// search target (what a control point is searching for, e.g.
    /// `...:ConnectionManager:1`).
    ///
    /// Per the UPnP AV device specs, a device implementing version N of a
    /// type "shall respond to all SSDP queries that specify [version] N-1 or
    /// lower" — backward compatibility means a higher-versioned service is a
    /// strict superset, so it satisfies any search for its own version or
    /// any older one. Only the trailing `:<version>` segment is version-
    /// aware; everything before it must match exactly, and a non-numeric or
    /// missing version on either side falls back to a plain string compare.
    fn urn_satisfies(advertised: &str, requested: &str) -> bool {
        if advertised == requested {
            return true;
        }
        fn split(urn: &str) -> Option<(&str, u32)> {
            let (base, version) = urn.rsplit_once(':')?;
            Some((base, version.parse().ok()?))
        }
        match (split(advertised), split(requested)) {
            (
                Some((advertised_base, advertised_version)),
                Some((requested_base, requested_version)),
            ) => advertised_base == requested_base && advertised_version >= requested_version,
            _ => false,
        }
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
            if info
                .services
                .iter()
                .any(|advertised| Self::urn_satisfies(advertised, service_type))
            {
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
            if Self::urn_satisfies(&info.device_type, device_type) {
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
    fn urn_satisfies_backward_compatible_version_searches() {
        // A device advertising v3 must still answer a search for v1 or v2
        // (backward compatibility), and obviously its own version.
        assert!(SearchHandler::urn_satisfies(
            "urn:schemas-upnp-org:service:ConnectionManager:3",
            "urn:schemas-upnp-org:service:ConnectionManager:1",
        ));
        assert!(SearchHandler::urn_satisfies(
            "urn:schemas-upnp-org:service:ConnectionManager:3",
            "urn:schemas-upnp-org:service:ConnectionManager:3",
        ));
        // But not a search for a version it doesn't actually implement yet.
        assert!(!SearchHandler::urn_satisfies(
            "urn:schemas-upnp-org:service:ConnectionManager:3",
            "urn:schemas-upnp-org:service:ConnectionManager:4",
        ));
        // Different service entirely, even with a satisfying version.
        assert!(!SearchHandler::urn_satisfies(
            "urn:schemas-upnp-org:service:ConnectionManager:3",
            "urn:schemas-upnp-org:service:AVTransport:1",
        ));
    }

    struct FakeService(ServiceInfo);

    impl SsdpService for FakeService {
        fn service_info(&self) -> Result<ServiceInfo, DiscoveryServiceError> {
            Ok(self.0.clone())
        }
    }

    fn fake_service(unique_id: uuid::Uuid) -> Box<dyn SsdpService> {
        Box::new(FakeService(ServiceInfo {
            device_type: "urn:schemas-upnp-org:device:MediaRenderer:1".to_string(),
            unique_id,
            location: "http://192.168.1.1:8080/description.xml".to_string(),
            server: crate::discovery::service::Server::Default,
            services: vec!["urn:schemas-upnp-org:service:AVTransport:1".to_string()],
        }))
    }

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

        if let SearchTarget::Uuid(uuid) =
            SearchTarget::from_str("uuid:2f402f80-da50-11e1-9b23-001788608f88")
        {
            assert_eq!(uuid, "uuid:2f402f80-da50-11e1-9b23-001788608f88");
        } else {
            panic!("Expected Uuid variant");
        }

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

    #[test]
    fn responds_to_matching_uuid_search() {
        let unique_id = uuid::Uuid::parse_str("2f402f80-da50-11e1-9b23-001788608f88").unwrap();
        let services = vec![fake_service(unique_id)];
        let handler = SearchHandler::new(30);

        let target = SearchTarget::from_str("uuid:2f402f80-da50-11e1-9b23-001788608f88");
        let responses = handler.handle_target(target, &services).unwrap();

        assert_eq!(responses.len(), 1);
        let rendered = responses[0].to_string();
        assert!(rendered.contains("ST: uuid:2f402f80-da50-11e1-9b23-001788608f88"));
        assert!(rendered.contains("USN: uuid:2f402f80-da50-11e1-9b23-001788608f88\r\n"));
    }

    #[test]
    fn ignores_non_matching_uuid_search() {
        let unique_id = uuid::Uuid::parse_str("2f402f80-da50-11e1-9b23-001788608f88").unwrap();
        let services = vec![fake_service(unique_id)];
        let handler = SearchHandler::new(30);

        let target = SearchTarget::from_str("uuid:00000000-0000-0000-0000-000000000000");
        let responses = handler.handle_target(target, &services).unwrap();

        assert!(responses.is_empty());
    }

    #[test]
    fn ssdp_all_includes_uuid_response() {
        let unique_id = uuid::Uuid::parse_str("2f402f80-da50-11e1-9b23-001788608f88").unwrap();
        let services = vec![fake_service(unique_id)];
        let handler = SearchHandler::new(30);

        let responses = handler
            .handle_target(SearchTarget::All, &services)
            .unwrap();

        assert!(responses
            .iter()
            .any(|r| r.to_string().contains("ST: uuid:2f402f80-da50-11e1-9b23-001788608f88")));
    }
}
