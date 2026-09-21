use crate::discovery::request::MSearchRequest;
use crate::discovery::search::{SearchHandler, SearchTarget};
use crate::discovery::{
    error::DiscoveryServiceError,
    messages::{SsdpHeader, SsdpMessage},
    service::{ServiceInfo, SsdpService},
    SSDP_MULTICAST_ADDR,
};
use rand::Rng;
use socket2::{Domain, Socket, Type};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tracing::{debug, error, info, trace, warn};

const NOTIFY_INTERVAL_SECS: u64 = 30;
const MAX_PACKET_SIZE: usize = 1024;
pub struct SsdpAdvertiser {
    socket: Arc<UdpSocket>,
    services: Vec<Box<dyn SsdpService>>,
    search_handler: SearchHandler,
}

impl SsdpAdvertiser {
    pub async fn new() -> Result<Self, DiscoveryServiceError> {
        let socket = bind_shared_multicast_socket()?;

        socket.set_broadcast(true)?;
        socket.set_multicast_ttl_v4(4)?;
        socket.set_multicast_loop_v4(true)?;
        socket.join_multicast_v4(Ipv4Addr::new(239, 255, 255, 250), Ipv4Addr::UNSPECIFIED)?;

        Ok(Self {
            socket: Arc::new(socket),
            services: Vec::new(),
            search_handler: SearchHandler::new(NOTIFY_INTERVAL_SECS),
        })
    }

    pub fn add_service<S: SsdpService + 'static>(&mut self, service: S) {
        self.services.push(Box::new(service));
    }
    fn handle_message(&self, msg: &[u8], addr: SocketAddr) -> Result<(), DiscoveryServiceError> {
        let msg_str = match std::str::from_utf8(msg) {
            Ok(s) => s,
            Err(_) => {
                warn!(addr = %addr, "Received invalid UTF-8 in SSDP message");
                return Ok(());
            }
        };
        trace!(message = %msg_str, addr = %addr, "Received SSDP message");

        if !msg_str.starts_with("M-SEARCH") {
            debug!(
                message_type = %msg_str.lines().next().unwrap_or(""),
                "Received non-M-SEARCH message"
            );
            return Ok(());
        }

        let request = match MSearchRequest::parse(msg_str) {
            Ok(request) => request,
            Err(err) => {
                warn!(addr = %addr, error = %err, "Rejecting malformed M-SEARCH request");
                return Ok(());
            }
        };

        debug!(
            target = %request.search_target,
            mx = request.mx.as_secs(),
            addr = %addr,
            "Processing search request"
        );
        self.handle_search(&request.search_target, request.mx, addr)
    }
    async fn send_message(
        &self,
        msg: &SsdpMessage,
        addr: &str,
    ) -> Result<(), DiscoveryServiceError> {
        self.send_raw(&msg.to_string(), addr).await
    }

    async fn send_raw(&self, message: &str, addr: &str) -> Result<(), DiscoveryServiceError> {
        debug!(
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

    /// Builds the responses and schedules them for delivery after a random
    /// delay in `[0, mx]` — required by SSDP so that every device on the LAN
    /// answering the same multicast M-SEARCH doesn't reply at the same
    /// instant. Spawned as a separate task (rather than `.await`ed inline)
    /// so a slow-to-respond search doesn't stall `run`'s receive loop for
    /// up to `mx` seconds.
    fn handle_search(
        &self,
        search_target: &str,
        mx: Duration,
        addr: SocketAddr,
    ) -> Result<(), DiscoveryServiceError> {
        let target = SearchTarget::from_str(search_target);
        let responses = self.search_handler.handle_target(target, &self.services)?;

        if responses.is_empty() {
            return Ok(());
        }

        let socket = Arc::clone(&self.socket);
        let delay = random_delay_within(mx);
        let dest = addr.to_string();

        tokio::spawn(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            for response in responses {
                let payload = response.to_string();
                if let Err(err) = socket.send_to(payload.as_bytes(), &dest).await {
                    warn!(addr = %dest, error = %err, "Failed to send delayed SSDP search response");
                }
            }
        });

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
                            if let Err(e) = self.handle_message(&buf[..len], addr) {
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

/// Binds UDP port 1900 with `SO_REUSEADDR` (and `SO_REUSEPORT` on unix) set
/// before bind, since SSDP requires the port to be shared
fn bind_shared_multicast_socket() -> Result<UdpSocket, DiscoveryServiceError> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, None)?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 1900).into())?;

    Ok(UdpSocket::from_std(socket.into())?)
}

/// Picks a random point in `[0, mx]` to delay a search response by, per the
/// SSDP spec's anti-flood requirement (UPnP Device Architecture, MX header).
fn random_delay_within(mx: Duration) -> Duration {
    if mx.is_zero() {
        return Duration::ZERO;
    }
    let max_millis = mx.as_millis().min(u128::from(u64::MAX)) as u64;
    Duration::from_millis(rand::thread_rng().gen_range(0..=max_millis))
}
