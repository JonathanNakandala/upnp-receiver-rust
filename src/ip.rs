use std::error::Error;
use std::net::{IpAddr, Ipv4Addr};
use local_ip_address::list_afinet_netifas;

pub fn get_default_ip() -> Result<IpAddr, Box<dyn Error>> {
    // First try to get all network interfaces
    let network_interfaces = list_afinet_netifas()?;

    // Look for non-loopback IPv4 addresses
    let default_ip = network_interfaces.iter()
        .find(|(_, ip)| {
            if let IpAddr::V4(addr) = ip {
                !addr.is_loopback() && !addr.is_link_local() && !addr.is_multicast()
            } else {
                false
            }
        })
        .map(|(_, ip)| *ip)
        .unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));

    Ok(default_ip)
}
