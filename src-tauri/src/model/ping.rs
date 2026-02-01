use crate::model::probe::ProbeStatus;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PingProtocol {
    Icmp,
    Tcp,
    Udp,
    Quic,
    Http,
}

impl std::fmt::Display for PingProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            PingProtocol::Icmp => "ICMP",
            PingProtocol::Tcp => "TCP",
            PingProtocol::Udp => "UDP",
            PingProtocol::Quic => "QUIC",
            PingProtocol::Http => "HTTP",
        };
        write!(f, "{}", s)
    }
}

impl FromStr for PingProtocol {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "icmp" => Ok(Self::Icmp),
            "tcp" => Ok(Self::Tcp),
            "udp" => Ok(Self::Udp),
            "quic" => Ok(Self::Quic),
            "http" => Ok(Self::Http),
            _ => Err(()),
        }
    }
}

/// Settings for a ping operation
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PingSetting {
    pub ip_addr: IpAddr,
    pub hostname: Option<String>,
    pub port: Option<u16>,
    pub hop_limit: u8,
    pub protocol: PingProtocol,
    pub count: u32,
    pub timeout_ms: u64,
    pub send_rate_ms: u64,
}

/// Single result of a ping operation
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PingSample {
    /// Sequence number
    pub seq: u32,
    /// IP address
    pub ip_addr: IpAddr,
    /// Host name
    pub hostname: Option<String>,
    /// Port
    pub port: Option<u16>,
    /// Round Trip Time (milliseconds)
    pub rtt_ms: Option<u64>,
    /// Status
    pub probe_status: ProbeStatus,
    /// Protocol
    pub protocol: PingProtocol,
}

/// Statistics of ping results
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PingStat {
    /// IP address
    pub ip_addr: IpAddr,
    /// Host name
    pub hostname: Option<String>,
    /// Port
    pub port: Option<u16>,
    /// Protocol
    pub protocol: PingProtocol,
    /// Ping samples
    pub samples: Vec<PingSample>,
    /// Transmitted packets
    pub transmitted_count: usize,
    /// Received packets
    pub received_count: usize,
    /// Minimum RTT in milliseconds
    pub min: Option<u64>,
    /// Average RTT in milliseconds
    pub avg: Option<u64>,
    /// Maximum RTT in milliseconds
    pub max: Option<u64>,
}

impl PingStat {
    pub fn from_samples(
        hostname: Option<String>,
        ip_addr: IpAddr,
        port: Option<u16>,
        protocol: PingProtocol,
        samples: Vec<PingSample>,
    ) -> Self {
        let transmitted_count = samples.len();
        let received: Vec<_> = samples.iter().filter(|s| s.probe_status.is_ok()).collect();
        let received_count = received.len();
        let rtts: Vec<u64> = received.iter().filter_map(|s| s.rtt_ms).collect();

        let min = rtts.iter().min().copied();
        let max = rtts.iter().max().copied();
        let avg = if !rtts.is_empty() {
            Some(rtts.iter().sum::<u64>() / rtts.len() as u64)
        } else {
            None
        };

        PingStat {
            ip_addr,
            hostname,
            port,
            protocol,
            samples,
            transmitted_count,
            received_count,
            min,
            avg,
            max,
        }
    }

    pub fn loss_rate(&self) -> f64 {
        if self.transmitted_count == 0 {
            0.0
        } else {
            1.0 - (self.received_count as f64 / self.transmitted_count as f64)
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PingStartPayload {
    pub run_id: String,
    pub setting: PingSetting,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PingProgressPayload {
    pub run_id: String,
    pub sample: PingSample,
    pub transmitted: u32,
    pub received: u32,
    pub percent: f32, // 0.0 to 100.0
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PingDonePayload {
    pub run_id: String,
    pub stat: PingStat,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PingErrorPayload {
    pub run_id: String,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PingCancelledPayload {
    pub run_id: String,
}
