use crate::discovery::search::{SearchHandler, SearchTarget};
use crate::discovery::{
    error::DiscoveryServiceError,
    messages::{SsdpHeader, SsdpMessage},
    service::{ServiceInfo, SsdpService},
};
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;
use tracing::{debug, error, info, trace, warn};

const SSDP_MULTICAST_ADDR: &str = "239.255.255.250:1900";
const NOTIFY_INTERVAL_SECS: u64 = 30;
const MAX_PACKET_SIZE: usize = 1024;
use std::collections::HashMap;
pub struct SsdpAdvertiser {
    socket: UdpSocket,
    services: Vec<Box<dyn SsdpService>>,
    search_handler: SearchHandler,
}

impl SsdpAdvertiser {
    pub async fn new() -> Result<Self, DiscoveryServiceError> {
        let socket = UdpSocket::bind("0.0.0.0:1900").await?;

        socket.set_broadcast(true)?;
        socket.set_multicast_ttl_v4(4)?;
        socket.set_multicast_loop_v4(true)?;
        socket.join_multicast_v4(Ipv4Addr::new(239, 255, 255, 250), Ipv4Addr::UNSPECIFIED)?;

        Ok(Self {
            socket,
            services: Vec::new(),
            search_handler: SearchHandler::new(NOTIFY_INTERVAL_SECS),
        })
    }

    pub fn add_service<S: SsdpService + 'static>(&mut self, service: S) {
        self.services.push(Box::new(service));
    }
    async fn handle_message(
        &self,
        msg: &[u8],
        addr: SocketAddr,
    ) -> Result<(), DiscoveryServiceError> {
        if let Ok(msg_str) = std::str::from_utf8(msg) {
            trace!(
                message = %msg_str,
                addr = %addr,
                "Received SSDP message"
            );

            // Parse M-SEARCH requests
            if msg_str.starts_with("M-SEARCH") {
                debug!("Processing M-SEARCH request");

                // Extract headers into a map for easier processing
                let headers: HashMap<String, String> = msg_str
                    .lines()
                    .filter_map(|line| {
                        let parts: Vec<&str> = line.splitn(2, ':').collect();
                        if parts.len() == 2 {
                            Some((parts[0].trim().to_uppercase(), parts[1].trim().to_string()))
                        } else {
                            None
                        }
                    })
                    .collect();

                // Validate M-SEARCH requirements
                if let Some(man) = headers.get("MAN") {
                    if man.trim_matches('"') != "ssdp:discover" {
                        warn!("Invalid MAN header in M-SEARCH");
                        return Ok(());
                    }
                } else {
                    warn!("Missing MAN header in M-SEARCH");
                    return Ok(());
                }

                if let Some(st) = headers.get("ST") {
                    debug!(target = %st, "Found search target");
                    self.handle_search(st, addr).await?;
                } else {
                    warn!("No ST (search target) header found in M-SEARCH");
                }
            } else {
                debug!(
                    message_type = %msg_str.lines().next().unwrap_or(""),
                    "Received non-M-SEARCH message"
                );
            }
        } else {
            warn!(
                addr = %addr,
                "Received invalid UTF-8 in SSDP message"
            );
        }
        Ok(())
    }
    async fn send_message(
        &self,
        msg: &SsdpMessage,
        addr: &str,
    ) -> Result<(), DiscoveryServiceError> {
        self.send_raw(&msg.to_string(), addr).await
    }

    async fn send_raw(&self, message: &str, addr: &str) -> Result<(), DiscoveryServiceError> {
        info!(
            addr = %addr,
            "Sending SSDP message"
        );
        self.socket.send_to(message.as_bytes(), addr).await?;
        Ok(())
    }

    fn build_header(&self, service: &ServiceInfo) -> SsdpHeader {
        SsdpHeader {
            location: service.location.clone(),
            server: service.server.to_string(),
            cache_control: Duration::from_secs(NOTIFY_INTERVAL_SECS * 2),
            unique_id: service.unique_id,
        }
    }

    async fn send_notify(&self, service: &ServiceInfo) -> Result<(), DiscoveryServiceError> {
        let msg = SsdpMessage::new_notify(
            self.build_header(service),
            service.device_type.clone(),
            service.services.clone(),
        );

        self.send_message(&msg, SSDP_MULTICAST_ADDR).await
    }

    async fn send_byebye(&self, service: &ServiceInfo) -> Result<(), DiscoveryServiceError> {
        let msg = SsdpMessage::new_notify(
            self.build_header(service),
            service.device_type.clone(),
            service.services.clone(),
        );

        if let Some(byebye) = msg.to_byebye_string() {
            self.send_raw(&byebye, SSDP_MULTICAST_ADDR).await?;
        }

        Ok(())
    }

    pub async fn announce_byebye(&self) -> Result<(), DiscoveryServiceError> {
        info!("Announcing ssdp:byebye for all services");
        for service in &self.services {
            let info = service.service_info()?;
            self.send_byebye(&info).await?;
        }
        Ok(())
    }

    async fn handle_search(
        &self,
        search_target: &str,
        addr: SocketAddr,
    ) -> Result<(), DiscoveryServiceError> {
        let target = SearchTarget::from_str(search_target);

        debug!(
            target = ?target,
            addr = %addr,
            "Processing search request"
        );

        let responses = self.search_handler.handle_target(target, &self.services)?;

        for response in responses {
            self.send_message(&response, &addr.to_string()).await?;
        }

        Ok(())
    }

    pub async fn run(&self) -> Result<(), DiscoveryServiceError> {
        let mut interval = tokio::time::interval(Duration::from_secs(NOTIFY_INTERVAL_SECS));
        let mut buf = [0; MAX_PACKET_SIZE];
        info!(interval = NOTIFY_INTERVAL_SECS, "Starting SSDP Advertiser");

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    for service in &self.services {
                        let info = service.service_info()?;
                        self.send_notify(&info).await?;
                    }
                }
                result = self.socket.recv_from(&mut buf) => {
                    match result {
                        Ok((len, addr)) => {
                            if let Err(e) = self.handle_message(&buf[..len], addr).await {
                                error!("Error handling message: {}", e);
                            }
                        }
                        Err(e) => error!("Socket error: {}", e),
                    }
                }
            }
        }
    }
}
