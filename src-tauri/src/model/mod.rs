#![allow(dead_code)]

pub mod dns;
pub mod endpoint;
pub mod interface;
pub mod ping;
pub mod probe;
pub mod scan;
pub mod speedtest;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub version: &'static str,
    pub authors: &'static [&'static str],
    pub repository: &'static str,
}

impl AppInfo {
    pub fn current() -> Self {
        Self {
            name: "NetDia",
            description: "Cross-platform network information tool",
            version: env!("CARGO_PKG_VERSION"),
            authors: &["shellrow <https://github.com/shellrow>"],
            repository: env!("CARGO_PKG_REPOSITORY"),
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct RouteEntry {
    pub family: u8,
    pub dst: String,
    pub gateway: Option<String>,
    pub on_link: bool,
    pub ifindex: Option<u32>,
    pub ifname: Option<String>,
    pub metric: Option<u32>,
    // U,G,S,H,R, ...
    pub flags: Vec<String>,
    pub proto: Option<String>,
    pub scope: Option<String>,
    pub table: Option<u32>,
    pub lifetime_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IpInfo {
    pub ip_version: String,
    pub ip_addr_dec: String,
    pub ip_addr: String,
    pub host_name: String,
    pub network: String,
    pub asn: String,
    pub as_name: String,
    pub country_code: String,
    pub country_name: String,
}

#[derive(Debug, Serialize)]
pub struct IpInfoDual {
    pub ipv4: Option<IpInfo>,
    pub ipv6: Option<IpInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SysInfo {
    pub hostname: String,
    pub os_type: String,
    pub os_version: String,
    pub kernel_version: Option<String>,
    pub edition: String,
    pub codename: String,
    pub bitness: String,
    pub architecture: String,
    pub proxy: ProxyEnv,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyEnv {
    pub http: Option<String>,
    pub https: Option<String>,
    pub all: Option<String>,
    pub no_proxy: Option<String>,
}
