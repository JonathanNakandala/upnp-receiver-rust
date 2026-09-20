//! Control-point side of SSDP: sends `M-SEARCH` and collects device
//! responses. Complements [`super::SsdpAdvertiser`], which only ever
//! *answers* M-SEARCH — nothing in the `upnp-suite-renderer` binary itself
//! uses this; it's for tools like `upnp-discover` that need to find *other*
//! UPnP devices on the network.

use crate::discovery::error::DiscoveryServiceError;
use crate::discovery::messages::SsdpMessage;
use crate::discovery::SSDP_MULTICAST_ADDR;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::Instant;
use tracing::{debug, warn};

const MAX_PACKET_SIZE: usize = 2048;

/// One SSDP search response, close to as-received off the wire. `location`
/// is the only header every device is required to send, and the only one
/// [`discover`] itself relies on (to fetch the device description) — the
/// rest are kept for callers that want to inspect them directly.
#[derive(Debug, Clone)]
pub struct SsdpResponse {
    pub from: SocketAddr,
    pub location: String,
    pub server: String,
    pub search_target: String,
    pub usn: String,
}

impl SsdpResponse {
    fn parse(raw: &str, from: SocketAddr) -> Option<Self> {
        let mut lines = raw.split("\r\n").flat_map(|line| line.split('\n'));

        // A search response's status line is "HTTP/1.1 200 OK" — anything
        // else (stray NOTIFY, malformed datagram) isn't one.
        if !lines.next()?.trim_start().starts_with("HTTP/1.1 200") {
            return None;
        }

        let headers: HashMap<String, String> = lines
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                Some((name.trim().to_ascii_uppercase(), value.trim().to_string()))
            })
            .collect();

        Some(Self {
            from,
            location: headers.get("LOCATION")?.clone(),
            server: headers.get("SERVER").cloned().unwrap_or_default(),
            search_target: headers.get("ST").cloned().unwrap_or_default(),
            usn: headers.get("USN").cloned().unwrap_or_default(),
        })
    }
}

/// Sends an `ssdp:all` M-SEARCH and collects responses for `wait` (used as
/// both the listen window and the request's `MX`, so responders spread
/// their replies across the same period we're actually listening).
///
/// A single device answers once per advertised root/device/service type,
/// so a plain `ssdp:all` search yields many responses per device — these
/// are deduped down to one [`SsdpResponse`] per unique `LOCATION` before
/// returning, since that's what identifies a device, not any one `ST`.
pub async fn discover(wait: Duration) -> Result<Vec<SsdpResponse>, DiscoveryServiceError> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.set_broadcast(true)?;

    let request = SsdpMessage::new_msearch("ssdp:all".to_string(), wait).to_string();
    socket
        .send_to(request.as_bytes(), SSDP_MULTICAST_ADDR)
        .await?;

    let mut buf = [0u8; MAX_PACKET_SIZE];
    let mut by_location: HashMap<String, SsdpResponse> = HashMap::new();
    // A little slack past `wait` for responses sent right at the edge of
    // their randomized MX delay to still arrive.
    let deadline = Instant::now() + wait + Duration::from_millis(500);

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }

        match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
            Ok(Ok((len, from))) => {
                let Ok(text) = std::str::from_utf8(&buf[..len]) else {
                    continue;
                };
                match SsdpResponse::parse(text, from) {
                    Some(response) => {
                        by_location.entry(response.location.clone()).or_insert(response);
                    }
                    None => debug!(addr = %from, "Ignoring non-search-response SSDP datagram"),
                }
            }
            Ok(Err(err)) => {
                warn!(error = %err, "Socket error while collecting SSDP responses");
                break;
            }
            Err(_) => break, // listen window elapsed
        }
    }

    Ok(by_location.into_values().collect())
}
