use std::net::IpAddr;

use crate::net::dns::resolver::DnsResolver;
use crate::{
    model::{
        dns::{Domain, DomainLookupInfo},
        endpoint::Host,
    },
    net::dns,
};

#[tauri::command]
pub async fn lookup_host(host: &str) -> Result<Host, String> {
    crate::net::dns::lookup_host(host, std::time::Duration::from_secs(5))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn lookup_domain(hostname: &str) -> Result<Domain, String> {
    let timeout = std::time::Duration::from_secs(5);
    Ok(dns::lookup_domain(hostname, timeout).await)
}

#[tauri::command]
pub async fn lookup_ip(hostname: &str) -> Result<Vec<IpAddr>, String> {
    let timeout = std::time::Duration::from_secs(5);
    dns::lookup_ip(hostname, timeout)
        .await
        .ok_or_else(|| "failed to resolve IP addresses".to_string())
}

#[tauri::command]
pub async fn reverse_lookup(ip: IpAddr) -> Result<String, String> {
    let timeout = std::time::Duration::from_secs(5);
    dns::reverse_lookup(ip, timeout)
        .await
        .ok_or_else(|| "failed to perform reverse lookup".to_string())
}

#[tauri::command]
pub async fn lookup_all(hostname: &str) -> Result<DomainLookupInfo, String> {
    let resolver =
        DnsResolver::new().map_err(|e| format!("failed to create DNS resolver: {}", e))?;
    resolver
        .lookup_all(hostname)
        .await
        .map_err(|e| format!("failed to lookup domain info: {}", e))
}
