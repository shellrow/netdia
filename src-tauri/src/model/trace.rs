use std::net::IpAddr;
use serde::{Deserialize, Serialize};

/// Protocol used for traceroute
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TraceProtocol {
    Icmp,
    Udp,
}

/// Settings passed from the frontend for traceroute
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TracerouteSetting {
    /// Resolved destination IP
    pub ip_addr: IpAddr,
    /// Display hostname (optional)
    pub hostname: Option<String>,
    /// Maximum hops (rounded to 30 if 0 or unset)
    pub max_hops: u8,
    /// Number of tries per hop (rounded to 1 if 0 or unset)
    pub tries_per_hop: u8,
    /// Timeout per try (ms)
    pub timeout_ms: u64,
    /// icmp / udp
    pub protocol: TraceProtocol,
}

/// Result for one hop
#[derive(Clone, Debug, Serialize)]
pub struct TraceHop {
    pub hop: u8,
    pub ip_addr: Option<IpAddr>,
    pub rtt_ms: Option<u64>,
    /// True if the destination was reached and the trace ended
    pub reached: bool,
    /// Supplementary message such as timeout
    pub note: Option<String>,
}

impl TraceHop {
    #[allow(dead_code)]
    pub fn timeout(hop: u8) -> Self {
        Self {
            hop,
            ip_addr: None,
            rtt_ms: None,
            reached: false,
            note: Some("timeout".into()),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TraceStartPayload {
    pub run_id: String,
    pub setting: TracerouteSetting,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TraceErrorPayload {
    pub run_id: String,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TraceCancelledPayload {
    pub run_id: String,
}
