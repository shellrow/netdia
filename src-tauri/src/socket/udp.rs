use super::SocketFamily;
use socket2::{Domain, Protocol, Socket, Type as SockType};
use std::io;
use std::net::{SocketAddr, UdpSocket as StdUdpSocket};
use std::time::Duration;
use tokio::net::UdpSocket;

/// UDP socket type, either DGRAM or RAW.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UdpSocketType {
    Dgram,
    Raw,
}

impl UdpSocketType {
    /// Returns true if the socket type is DGRAM.
    pub fn is_dgram(&self) -> bool {
        matches!(self, UdpSocketType::Dgram)
    }

    /// Returns true if the socket type is RAW.
    pub fn is_raw(&self) -> bool {
        matches!(self, UdpSocketType::Raw)
    }

    /// Converts the UDP socket type to a `socket2::Type`.
    pub(crate) fn to_sock_type(&self) -> SockType {
        match self {
            UdpSocketType::Dgram => SockType::DGRAM,
            UdpSocketType::Raw => SockType::RAW,
        }
    }
}

/// Configuration options for a UDP socket.
#[derive(Debug, Clone)]
pub struct UdpConfig {
    /// The socket family.
    pub socket_family: SocketFamily,
    /// The socket type (DGRAM or RAW).
    pub socket_type: UdpSocketType,
    /// Address to bind. If `None`, the operating system chooses the address.
    pub bind_addr: Option<SocketAddr>,
    /// Enable address reuse (`SO_REUSEADDR`).
    pub reuseaddr: Option<bool>,
    /// Allow broadcast (`SO_BROADCAST`).
    pub broadcast: Option<bool>,
    /// Time to live value.
    pub ttl: Option<u32>,
    /// Hop limit value.
    pub hoplimit: Option<u32>,
    /// Read timeout for the socket.
    pub read_timeout: Option<Duration>,
    /// Write timeout for the socket.
    pub write_timeout: Option<Duration>,
    /// Bind to a specific interface (Linux only).
    pub bind_device: Option<String>,
}

impl Default for UdpConfig {
    fn default() -> Self {
        Self {
            socket_family: SocketFamily::IPV4,
            socket_type: UdpSocketType::Dgram,
            bind_addr: None,
            reuseaddr: None,
            broadcast: None,
            ttl: None,
            hoplimit: None,
            read_timeout: None,
            write_timeout: None,
            bind_device: None,
        }
    }
}

impl UdpConfig {
    /// Create a new UDP configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the bind address.
    pub fn with_bind_addr(mut self, addr: SocketAddr) -> Self {
        self.bind_addr = Some(addr);
        self
    }

    /// Enable address reuse.
    pub fn with_reuseaddr(mut self, on: bool) -> Self {
        self.reuseaddr = Some(on);
        self
    }

    /// Allow broadcast.
    pub fn with_broadcast(mut self, on: bool) -> Self {
        self.broadcast = Some(on);
        self
    }

    /// Set the time to live value.
    pub fn with_ttl(mut self, ttl: u32) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// Set the hop limit value.
    pub fn with_hoplimit(mut self, hops: u32) -> Self {
        self.hoplimit = Some(hops);
        self
    }

    /// Set the read timeout.
    pub fn with_read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = Some(timeout);
        self
    }

    /// Set the write timeout.
    pub fn with_write_timeout(mut self, timeout: Duration) -> Self {
        self.write_timeout = Some(timeout);
        self
    }

    /// Bind to a specific interface (Linux only).
    pub fn with_bind_device(mut self, iface: impl Into<String>) -> Self {
        self.bind_device = Some(iface.into());
        self
    }
}

/// Asynchronous UDP socket built on top of Tokio.
#[derive(Debug)]
pub struct AsyncUdpSocket {
    inner: UdpSocket,
}

impl AsyncUdpSocket {
    /// Create an asynchronous UDP socket from the given configuration.
    pub fn from_config(config: &UdpConfig) -> io::Result<Self> {
        let socket = Socket::new(
            config.socket_family.to_domain(),
            config.socket_type.to_sock_type(),
            Some(Protocol::UDP),
        )?;

        socket.set_nonblocking(true)?;

        // Set socket options based on configuration
        if let Some(flag) = config.reuseaddr {
            socket.set_reuse_address(flag)?;
        }
        if let Some(flag) = config.broadcast {
            socket.set_broadcast(flag)?;
        }
        if let Some(ttl) = config.ttl {
            socket.set_ttl(ttl)?;
        }
        if let Some(hoplimit) = config.hoplimit {
            socket.set_unicast_hops_v6(hoplimit)?;
        }
        if let Some(timeout) = config.read_timeout {
            socket.set_read_timeout(Some(timeout))?;
        }
        if let Some(timeout) = config.write_timeout {
            socket.set_write_timeout(Some(timeout))?;
        }

        // Linux: optional interface name
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "fuchsia"))]
        if let Some(iface) = &config.bind_device {
            socket.bind_device(Some(iface.as_bytes()))?;
        }

        // bind to the specified address if provided
        if let Some(addr) = config.bind_addr {
            socket.bind(&addr.into())?;
        }

        #[cfg(windows)]
        let std_socket = unsafe {
            use std::os::windows::io::{FromRawSocket, IntoRawSocket};
            StdUdpSocket::from_raw_socket(socket.into_raw_socket())
        };
        #[cfg(unix)]
        let std_socket = unsafe {
            use std::os::fd::{FromRawFd, IntoRawFd};
            StdUdpSocket::from_raw_fd(socket.into_raw_fd())
        };

        let inner = UdpSocket::from_std(std_socket)?;

        Ok(Self { inner })
    }

    /// Create a socket of arbitrary type (DGRAM or RAW).
    pub fn new(domain: Domain, sock_type: SockType) -> io::Result<Self> {
        let socket = Socket::new(domain, sock_type, Some(Protocol::UDP))?;
        socket.set_nonblocking(true)?;

        #[cfg(windows)]
        let std_socket = unsafe {
            use std::os::windows::io::{FromRawSocket, IntoRawSocket};
            StdUdpSocket::from_raw_socket(socket.into_raw_socket())
        };
        #[cfg(unix)]
        let std_socket = unsafe {
            use std::os::fd::{FromRawFd, IntoRawFd};
            StdUdpSocket::from_raw_fd(socket.into_raw_fd())
        };

        let inner = UdpSocket::from_std(std_socket)?;

        Ok(Self { inner })
    }

    /// Convenience constructor for IPv4 DGRAM.
    pub fn v4_dgram() -> io::Result<Self> {
        Self::new(Domain::IPV4, SockType::DGRAM)
    }

    /// Convenience constructor for IPv6 DGRAM.
    pub fn v6_dgram() -> io::Result<Self> {
        Self::new(Domain::IPV6, SockType::DGRAM)
    }

    /// IPv4 RAW UDP. Requires administrator privileges.
    pub fn raw_v4() -> io::Result<Self> {
        Self::new(Domain::IPV4, SockType::RAW)
    }

    /// IPv6 RAW UDP. Requires administrator privileges.
    pub fn raw_v6() -> io::Result<Self> {
        Self::new(Domain::IPV6, SockType::RAW)
    }

    /// Send data asynchronously.
    pub async fn send_to(&self, buf: &[u8], target: SocketAddr) -> io::Result<usize> {
        self.inner.send_to(buf, target).await
    }

    /// Receive data asynchronously.
    pub async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.inner.recv_from(buf).await
    }

    /// Retrieve the local socket address.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn into_tokio_socket(self) -> io::Result<UdpSocket> {
        Ok(self.inner)
    }

    #[cfg(unix)]
    pub fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        use std::os::fd::AsRawFd;
        self.inner.as_raw_fd()
    }

    #[cfg(windows)]
    pub fn as_raw_socket(&self) -> std::os::windows::io::RawSocket {
        use std::os::windows::io::AsRawSocket;
        self.inner.as_raw_socket()
    }
}
