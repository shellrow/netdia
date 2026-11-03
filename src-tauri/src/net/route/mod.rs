use netroute::RouteEntry;
use std::io;

pub fn list_routes() -> io::Result<Vec<RouteEntry>> {
    netroute::list_routes()
}

/// Ask the routing table for the source address without sending a packet.
pub fn source_address(destination: std::net::IpAddr) -> io::Result<std::net::IpAddr> {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
    let bind = if destination.is_ipv4() {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    } else {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    };
    let socket = UdpSocket::bind(SocketAddr::new(bind, 0))?;
    socket.connect(SocketAddr::new(destination, 9))?;
    Ok(socket.local_addr()?.ip())
}

#[cfg(test)]
mod tests {
    #[test]
    fn loopback_source_follows_destination_family() {
        for ip in ["127.0.0.1", "::1"] {
            let ip = ip.parse().unwrap();
            assert_eq!(super::source_address(ip).unwrap(), ip);
        }
    }
}
