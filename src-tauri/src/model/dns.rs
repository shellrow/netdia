use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// A domain with its associated IP addresses
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Domain {
    pub name: String,
    pub ips: Vec<IpAddr>,
}

/// Detailed DNS lookup information for a domain
#[derive(Serialize, Deserialize)]
pub struct DomainLookupInfo {
    pub name: String,
    pub a: Vec<Ipv4Addr>,
    pub aaaa: Vec<Ipv6Addr>,
    pub mx: Vec<MxRecord>,
    pub ns: Vec<String>,
    pub soa: Vec<SoaRecord>,
    pub srv: Vec<SrvRecord>,
    pub tlsa: Vec<TlsaRecord>,
    pub txt: Vec<TxtRecord>,
    pub cert: Vec<CertRecord>,
}

#[derive(Serialize, Deserialize)]
pub struct MxRecord {
    pub preference: u16,
    pub exchange: String,
}

#[derive(Serialize, Deserialize)]
pub struct SoaRecord {
    pub mname: String,
    pub rname: String,
    pub serial: u32,
    pub refresh: i32,
    pub retry: i32,
    pub expire: i32,
    pub minimum: u32,
}

#[derive(Serialize, Deserialize)]
pub struct SrvRecord {
    pub priority: u16,
    pub weight: u16,
    pub port: u16,
    pub target: String,
}

#[derive(Serialize, Deserialize)]
pub struct TlsaRecord {
    pub cert_usage: u8,
    pub selector: u8,
    pub matching: u8,
    pub cert_data_base64: String,
}

#[derive(Serialize, Deserialize)]
pub struct TxtRecord {
    pub key: String,
    pub value: String,
}

#[derive(Serialize, Deserialize)]
pub struct CertRecord {
    pub cert_type: u16,
    pub key_tag: u16,
    pub algorithm: u8,
    pub cert_data_base64: String,
}
