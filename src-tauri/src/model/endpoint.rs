use netdev::MacAddr;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};

/// Transport protocol type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum TransportProtocol {
    Tcp,
    Udp,
    Quic,
}

impl TransportProtocol {
    /// Create a TransportProtocol from a string representation.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "tcp" => Some(TransportProtocol::Tcp),
            "udp" => Some(TransportProtocol::Udp),
            "quic" => Some(TransportProtocol::Quic),
            _ => None,
        }
    }
    /// Get the string representation of the TransportProtocol.
    pub fn as_str(&self) -> &'static str {
        match self {
            TransportProtocol::Tcp => "tcp",
            TransportProtocol::Udp => "udp",
            TransportProtocol::Quic => "quic",
        }
    }
}

/// Network port with transport protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
pub struct Port {
    pub number: u16,
    pub transport: TransportProtocol,
}

impl Port {
    /// Create a new Port instance.
    pub fn new(number: u16, transport: TransportProtocol) -> Self {
        Self { number, transport }
    }
    /// Get the SocketAddr for the given IP address and this port.
    pub fn socket_addr(&self, ip: IpAddr) -> SocketAddr {
        SocketAddr::new(ip, self.number)
    }
}

impl From<(u16, TransportProtocol)> for Port {
    fn from(t: (u16, TransportProtocol)) -> Self {
        Self {
            number: t.0,
            transport: t.1,
        }
    }
}

impl std::fmt::Display for Port {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}",
            match self.transport {
                TransportProtocol::Tcp => "tcp",
                TransportProtocol::Udp => "udp",
                TransportProtocol::Quic => "quic",
            },
            self.number
        )
    }
}

/// Representation of a host (IP address and optional hostname)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub ip: IpAddr,
    pub hostname: Option<String>,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            ip: IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
            hostname: None,
        }
    }
}

impl Host {
    /// Create a new Host instance.
    pub fn new(ip: IpAddr) -> Self {
        Self {
            ip,
            ..Default::default()
        }
    }
    /// Create a new Host instance with the specified hostname.
    pub fn with_hostname(ip: IpAddr, hostname: String) -> Self {
        Self {
            ip,
            hostname: Some(hostname),
            ..Default::default()
        }
    }
}

/// Representation of a possible host with optional IP address and hostname
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaybeHost {
    pub ip: Option<IpAddr>,
    pub hostname: Option<String>,
}

/// Representation of an endpoint with IP, hostname, MAC address, tags, and ports
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub ip: IpAddr,
    pub hostname: Option<String>,
    pub mac_addr: Option<MacAddr>,
    pub tags: Vec<String>,
    pub ports: Vec<Port>,
}

impl Default for Endpoint {
    fn default() -> Self {
        Self {
            ip: IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
            hostname: None,
            mac_addr: None,
            tags: Vec::new(),
            ports: Vec::new(),
        }
    }
}

impl Endpoint {
    /// Create a new Endpoint instance.
    pub fn new(ip: IpAddr) -> Self {
        Self {
            ip,
            ..Default::default()
        }
    }
    /// Create a new Endpoint instance with the specified hostname.
    pub fn with_hostname(ip: IpAddr, hostname: String) -> Self {
        Self {
            ip,
            hostname: Some(hostname),
            ..Default::default()
        }
    }
    /// Add a port to the endpoint if it does not already exist.
    pub fn upsert_port(&mut self, port: Port) {
        if !self.ports.contains(&port) {
            self.ports.push(port);
        }
    }
    /// Merge another Endpoint into this one, combining tags and ports.
    pub fn merge(&mut self, other: Endpoint) {
        if self.hostname.is_none() {
            self.hostname = other.hostname;
        }
        if self.mac_addr.is_none() {
            self.mac_addr = other.mac_addr;
        }

        for t in other.tags {
            if !self.tags.contains(&t) {
                self.tags.push(t);
            }
        }

        for p in other.ports {
            if !self.ports.contains(&p) {
                self.ports.push(p);
            }
        }
    }
    /// Get the SocketAddr instances for the specified transport protocol.
    pub fn socket_addrs(&self, transport: TransportProtocol) -> Vec<SocketAddr> {
        self.ports
            .iter()
            .filter(|p| p.transport == transport)
            .map(|p| p.socket_addr(self.ip))
            .collect()
    }
}
