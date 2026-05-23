pub mod icmp;
pub mod neigh;
pub mod progress;
pub mod quic;
pub mod tcp;
pub mod tuner;

use crate::model::scan::TargetPortsPreset;
use std::sync::OnceLock;

static TOP_100_PORTS: OnceLock<Vec<u16>> = OnceLock::new();
static TOP_1000_PORTS: OnceLock<Vec<u16>> = OnceLock::new();
static WELLKNOWN_PORTS: OnceLock<Vec<u16>> = OnceLock::new();

fn load_ports(json: &str, label: &str) -> Vec<u16> {
    serde_json::from_str(json).unwrap_or_else(|_| panic!("Invalid {label} format"))
}

fn top_100_ports() -> &'static [u16] {
    TOP_100_PORTS.get_or_init(|| {
        load_ports(
            crate::resources::TOP_100_PORTS_JSON,
            "nd-top-100-ports.json",
        )
    })
}

fn top_1000_ports() -> &'static [u16] {
    TOP_1000_PORTS.get_or_init(|| {
        load_ports(
            crate::resources::TOP_1000_PORTS_JSON,
            "nd-top-1000-ports.json",
        )
    })
}

fn wellknown_ports() -> &'static [u16] {
    WELLKNOWN_PORTS.get_or_init(|| {
        load_ports(
            crate::resources::WELLKNOWN_PORTS_JSON,
            "nd-wellknown-ports.json",
        )
    })
}

pub fn expand_ports(preset: &TargetPortsPreset, user_ports: &[u16]) -> Vec<u16> {
    match preset {
        TargetPortsPreset::Custom => {
            // Just use user ports
            let mut v = user_ports.to_vec();
            v.sort_unstable();
            v.dedup();
            v
        }
        TargetPortsPreset::Common => {
            let mut v = vec![
                20, 21, 22, 23, 25, 53, 67, 68, 69, 80, 110, 123, 135, 137, 138, 139, 143, 161,
                162, 179, 389, 443, 445, 465, 514, 587, 636, 993, 995, 1433, 1521, 2049, 2375,
                2376, 3306, 3389, 5432, 5800, 5900, 5901, 5984, 5985, 5986, 6379, 8000, 8008, 8080,
                8081, 8088, 8443, 8888, 9000, 9090, 9200, 9300, 11211, 27017,
            ];
            v.extend_from_slice(user_ports);
            v.sort_unstable();
            v.dedup();
            v
        }
        TargetPortsPreset::Top100 => {
            let mut v = top_100_ports().to_vec();
            v.extend_from_slice(user_ports);
            v.sort_unstable();
            v.dedup();
            v
        }
        TargetPortsPreset::WellKnown => {
            let mut v = wellknown_ports().to_vec();
            v.extend_from_slice(user_ports);
            v.sort_unstable();
            v.dedup();
            v
        }
        TargetPortsPreset::Full => {
            // caution: heavy
            let mut v: Vec<u16> = (1u16..=65535u16).collect();
            for &p in user_ports {
                if !v.contains(&p) {
                    v.push(p);
                }
            }
            v
        }
        TargetPortsPreset::Top1000 => {
            let mut v = top_1000_ports().to_vec();
            v.extend_from_slice(user_ports);
            v.sort_unstable();
            v.dedup();
            v
        }
    }
}
