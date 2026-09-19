use std::time::Duration;
use uuid::Uuid;

// Constants for SSDP message fields
const SSDP_HOST: &str = "239.255.255.250:1900";
#[cfg(test)]
const UPNP_VERSION: &str = "UPnP/1.0";

#[derive(Debug, Clone)]
pub struct SsdpHeader {
    pub location: String,
    pub server: String,
    pub cache_control: Duration,
    pub unique_id: Uuid,
}

#[derive(Debug, Clone)]
pub enum SsdpMessage {
    Notify {
        header: SsdpHeader,
        device_type: String,
        services: Vec<String>,
    },
    Response {
        header: SsdpHeader,
        search_target: String,
        ext: bool,
    },
    MSearch {
        search_target: String,
        mx: Duration,
    },
}

impl SsdpHeader {
    fn format_cache_control(&self) -> String {
        format!("max-age={}", self.cache_control.as_secs())
    }

    fn format_common_headers(&self) -> String {
        format!(
            "CACHE-CONTROL: {}\r\n\
             LOCATION: {}\r\n\
             SERVER: {}\r\n\
             DATE: {}\r\n",
            self.format_cache_control(),
            self.location,
            self.server,
            httpdate::fmt_http_date(std::time::SystemTime::now())
        )
    }

    fn format_uuid(&self) -> String {
        format!("uuid:{}", self.unique_id)
    }
}

impl SsdpMessage {
    pub fn new_notify(header: SsdpHeader, device_type: String, services: Vec<String>) -> Self {
        Self::Notify {
            header,
            device_type,
            services,
        }
    }

    pub fn new_response(header: SsdpHeader, search_target: String, ext: bool) -> Self {
        Self::Response {
            header,
            search_target,
            ext,
        }
    }

    pub fn new_msearch(search_target: String, mx: Duration) -> Self {
        Self::MSearch { search_target, mx }
    }

    fn format_usn(uuid: &str, target_type: &str) -> String {
        if target_type == "upnp:rootdevice" {
            format!("{}::upnp:rootdevice", uuid)
        } else {
            format!("{}::{}", uuid, target_type)
        }
    }

}

impl std::fmt::Display for SsdpMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rendered = match self {
            SsdpMessage::Notify {
                header,
                device_type,
                services,
            } => {
                let mut messages = Vec::new();
                let uuid = header.format_uuid();

                // Root device notification
                messages.push(format!(
                    "NOTIFY * HTTP/1.1\r\n\
                     HOST: {}\r\n\
                     {}\
                     NTS: ssdp:alive\r\n\
                     NT: upnp:rootdevice\r\n\
                     USN: {}\r\n\r\n",
                    SSDP_HOST,
                    header.format_common_headers(),
                    Self::format_usn(&uuid, "upnp:rootdevice")
                ));

                // Device type notification
                messages.push(format!(
                    "NOTIFY * HTTP/1.1\r\n\
                     HOST: {}\r\n\
                     {}\
                     NTS: ssdp:alive\r\n\
                     NT: {}\r\n\
                     USN: {}\r\n\r\n",
                    SSDP_HOST,
                    header.format_common_headers(),
                    device_type,
                    Self::format_usn(&uuid, device_type)
                ));

                // Service notifications
                for service in services {
                    messages.push(format!(
                        "NOTIFY * HTTP/1.1\r\n\
                         HOST: {}\r\n\
                         {}\
                         NTS: ssdp:alive\r\n\
                         NT: {}\r\n\
                         USN: {}\r\n\r\n",
                        SSDP_HOST,
                        header.format_common_headers(),
                        service,
                        Self::format_usn(&uuid, service)
                    ));
                }

                messages.join("")
            }
            SsdpMessage::Response {
                header,
                search_target,
                ext,
            } => {
                let uuid = header.format_uuid();
                let usn = Self::format_usn(&uuid, search_target);
                let mut response = format!(
                    "HTTP/1.1 200 OK\r\n\
                     {}\
                     ST: {}\r\n\
                     USN: {}\r\n",
                    header.format_common_headers(),
                    search_target,
                    usn
                );

                if *ext {
                    response.push_str("EXT:\r\n");
                }

                response.push_str("\r\n");
                response
            }
            SsdpMessage::MSearch { search_target, mx } => {
                format!(
                    "M-SEARCH * HTTP/1.1\r\n\
                     HOST: {}\r\n\
                     MAN: \"ssdp:discover\"\r\n\
                     ST: {}\r\n\
                     MX: {}\r\n\r\n",
                    SSDP_HOST,
                    search_target,
                    mx.as_secs()
                )
            }
        };

        write!(f, "{}", rendered)
    }
}

impl SsdpMessage {
    // Add byebye message support
    pub fn to_byebye_string(&self) -> Option<String> {
        match self {
            SsdpMessage::Notify {
                header,
                device_type,
                services,
            } => {
                let mut messages = Vec::new();
                let uuid = header.format_uuid();

                // Root device byebye
                messages.push(format!(
                    "NOTIFY * HTTP/1.1\r\n\
                     HOST: {}\r\n\
                     NT: upnp:rootdevice\r\n\
                     NTS: ssdp:byebye\r\n\
                     USN: {}\r\n\r\n",
                    SSDP_HOST,
                    Self::format_usn(&uuid, "upnp:rootdevice")
                ));

                // Device type byebye
                messages.push(format!(
                    "NOTIFY * HTTP/1.1\r\n\
                     HOST: {}\r\n\
                     NT: {}\r\n\
                     NTS: ssdp:byebye\r\n\
                     USN: {}\r\n\r\n",
                    SSDP_HOST,
                    device_type,
                    Self::format_usn(&uuid, device_type)
                ));

                // Service byebyes
                for service in services {
                    messages.push(format!(
                        "NOTIFY * HTTP/1.1\r\n\
                         HOST: {}\r\n\
                         NT: {}\r\n\
                         NTS: ssdp:byebye\r\n\
                         USN: {}\r\n\r\n",
                        SSDP_HOST,
                        service,
                        Self::format_usn(&uuid, service)
                    ));
                }

                Some(messages.join(""))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_header() -> SsdpHeader {
        SsdpHeader {
            location: "http://192.168.1.1:8080/description.xml".to_string(),
            server: format!("Rust/1.0 {}", UPNP_VERSION),
            cache_control: Duration::from_secs(1800),
            unique_id: Uuid::parse_str("2f402f80-da50-11e1-9b23-001788608f88").unwrap(),
        }
    }

    #[test]
    fn test_notify_message_format() {
        let msg = SsdpMessage::new_notify(
            create_test_header(),
            "urn:schemas-upnp-org:device:MediaRenderer:1".to_string(),
            vec!["urn:schemas-upnp-org:service:ConnectionManager:1".to_string()],
        );

        let output = msg.to_string();
        assert!(output.contains("NOTIFY * HTTP/1.1"));
        assert!(output.contains(&format!("HOST: {}", SSDP_HOST)));
        assert!(output.contains("CACHE-CONTROL: max-age=1800"));
        assert!(output.contains("NT: upnp:rootdevice"));
        assert!(output.contains("USN: uuid:2f402f80-da50-11e1-9b23-001788608f88::upnp:rootdevice"));
        assert!(output.contains("DATE:"));
    }

    #[test]
    fn test_response_message_format() {
        let msg = SsdpMessage::new_response(
            create_test_header(),
            "urn:schemas-upnp-org:service:ConnectionManager:1".to_string(),
            true,
        );

        let output = msg.to_string();
        assert!(output.contains("HTTP/1.1 200 OK"));
        assert!(output.contains("ST: urn:schemas-upnp-org:service:ConnectionManager:1"));
        assert!(output.contains("USN: uuid:2f402f80-da50-11e1-9b23-001788608f88::urn:schemas-upnp-org:service:ConnectionManager:1"));
        assert!(output.contains("EXT:"));
        assert!(output.contains("DATE:"));
    }

    #[test]
    fn test_byebye_message_format() {
        let msg = SsdpMessage::new_notify(
            create_test_header(),
            "urn:schemas-upnp-org:device:MediaRenderer:1".to_string(),
            vec!["urn:schemas-upnp-org:service:ConnectionManager:1".to_string()],
        );

        let output = msg.to_byebye_string().unwrap();
        assert!(output.contains("NOTIFY * HTTP/1.1"));
        assert!(output.contains("NTS: ssdp:byebye"));
        assert!(output.contains("upnp:rootdevice"));
    }
}
