use netdev::interface::state::OperState;
use netdev::interface::types::InterfaceType;
use netdev::ipnet::{Ipv4Net, Ipv6Net};
use netdev::{MacAddr, NetworkDevice};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::time::SystemTime;

/// Structure of Network Interface information
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NetworkInterface {
    /// Index of network interface. This is an integer which uniquely identifies the interface
    /// on this machine.
    pub index: u32,
    /// Machine-readable name of the network interface. On unix-like OSs, this is the interface
    /// name, like 'eth0' or 'eno1'. On Windows, this is the interface's GUID as a string.
    pub name: String,
    /// Display name of network interface. On Windows, this is the network adapter configured
    pub display_name: String,
    /// Friendly name of network interface. On Windows, this is the network adapter configured
    /// name, e.g. "Ethernet 5" or "Wi-Fi". On Mac, this is the interface display name,
    /// such as "Ethernet" or "FireWire". If no friendly name is available, this is left as None.
    pub friendly_name: Option<String>,
    /// Description of the network interface. On Windows, this is the network adapter model, such
    /// as "Realtek USB GbE Family Controller #4" or "Software Loopback Interface 1". Currently
    /// this is not available on platforms other than Windows.
    pub description: Option<String>,
    /// Interface Type
    pub if_type: InterfaceType,
    /// MAC address of network interface
    pub mac_addr: Option<MacAddr>,
    /// List of Ipv4Nets (IPv4 address + netmask) for the network interface
    pub ipv4: Vec<Ipv4Net>,
    /// List of Ipv6Nets (IPv6 address + netmask) for the network interface
    pub ipv6: Vec<Ipv6Net>,
    /// List of IPv6 Scope IDs for each of the corresponding elements in the ipv6 address vector.
    /// The Scope ID is an integer which uniquely identifies this interface address on the system,
    /// and must be provided when using link-local addressing to specify which interface
    /// you wish to use. The scope ID can be the same as the interface index, but is not
    /// required to be by the standard.
    /// The scope ID can also be referred to as the zone index.
    pub ipv6_scope_ids: Vec<u32>,
    /// Flags for the network interface (OS Specific)
    pub flags: u32,
    /// Operational state at the time of interface discovery
    pub oper_state: OperState,
    /// Speed in bits per second of the transmit for the network interface, if known.
    /// Currently only supported on Linux, Android, and Windows.
    pub transmit_speed: Option<u64>,
    /// Speed in bits per second of the receive for the network interface.
    /// Currently only supported on Linux, Android, and Windows.
    pub receive_speed: Option<u64>,
    /// Statistics for this network interface, such as received and transmitted bytes.
    ///
    /// This field is populated at the time of interface discovery
    ///
    /// The values represent a snapshot of total RX and TX bytes since system boot,
    /// and include a timestamp (`SystemTime`) indicating when the snapshot was taken.
    pub stats: TrafficStats,
    /// Default gateway for the network interface. This is the address of the router to which
    /// IP packets are forwarded when they need to be sent to a device outside
    /// of the local network.
    pub gateway: Option<NetworkDevice>,
    /// DNS server addresses for the network interface
    pub dns_servers: Vec<IpAddr>,
    /// Maximum Transmission Unit (MTU) for the network interface
    pub mtu: Option<u32>,
    /// Whether this is the default interface for accessing the Internet.
    pub default: bool,
}

fn stats_from_netdev(st: &netdev::stats::counters::InterfaceStats) -> TrafficStats {
    TrafficStats {
        rx_bytes: st.rx_bytes,
        tx_bytes: st.tx_bytes,
        rx_bytes_per_sec: 0.0,
        tx_bytes_per_sec: 0.0,
        timestamp: st.timestamp.unwrap_or(SystemTime::now()),
    }
}

impl From<netdev::Interface> for NetworkInterface {
    fn from(iface: netdev::Interface) -> Self {
        let display_name = iface
            .friendly_name
            .clone()
            .unwrap_or_else(|| iface.name.clone());
        let stats = iface
            .stats
            .as_ref()
            .map(stats_from_netdev)
            .unwrap_or(TrafficStats {
                rx_bytes: 0,
                tx_bytes: 0,
                rx_bytes_per_sec: 0.0,
                tx_bytes_per_sec: 0.0,
                timestamp: SystemTime::now(),
            });

        NetworkInterface {
            index: iface.index,
            name: iface.name,
            display_name,
            friendly_name: iface.friendly_name,
            description: iface.description,
            if_type: iface.if_type,
            mac_addr: iface.mac_addr.map(|m| m as MacAddr),
            ipv4: iface.ipv4,
            ipv6: iface.ipv6,
            ipv6_scope_ids: iface.ipv6_scope_ids,
            flags: iface.flags,
            oper_state: iface.oper_state,
            transmit_speed: iface.transmit_speed,
            receive_speed: iface.receive_speed,
            stats,
            gateway: iface.gateway,
            dns_servers: iface.dns_servers,
            mtu: iface.mtu,
            default: iface.default,
        }
    }
}

impl From<&netdev::Interface> for NetworkInterface {
    fn from(iface: &netdev::Interface) -> Self {
        let display_name = iface
            .friendly_name
            .clone()
            .unwrap_or_else(|| iface.name.clone());
        let stats = iface
            .stats
            .as_ref()
            .map(stats_from_netdev)
            .unwrap_or(TrafficStats {
                rx_bytes: 0,
                tx_bytes: 0,
                rx_bytes_per_sec: 0.0,
                tx_bytes_per_sec: 0.0,
                timestamp: SystemTime::now(),
            });

        NetworkInterface {
            index: iface.index,
            name: iface.name.clone(),
            display_name,
            friendly_name: iface.friendly_name.clone(),
            description: iface.description.clone(),
            if_type: iface.if_type,
            mac_addr: iface.mac_addr,
            ipv4: iface.ipv4.clone(),
            ipv6: iface.ipv6.clone(),
            ipv6_scope_ids: iface.ipv6_scope_ids.clone(),
            flags: iface.flags,
            oper_state: iface.oper_state,
            transmit_speed: iface.transmit_speed,
            receive_speed: iface.receive_speed,
            stats,
            gateway: iface.gateway.clone(),
            dns_servers: iface.dns_servers.clone(),
            mtu: iface.mtu,
            default: iface.default,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TrafficStats {
    /// Total received bytes on this interface.
    pub rx_bytes: u64,
    /// Total transmitted bytes on this interface.
    pub tx_bytes: u64,
    /// Received bytes per second on this interface.
    pub rx_bytes_per_sec: f64,
    /// Transmitted bytes per second on this interface.
    pub tx_bytes_per_sec: f64,
    /// The system timestamp when this snapshot was taken.
    pub timestamp: SystemTime,
}
