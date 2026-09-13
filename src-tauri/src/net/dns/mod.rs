mod pool;
pub mod resolver;
use anyhow::{Context, Result};
use futures::{stream, StreamExt};
use std::{collections::HashSet, net::IpAddr, time::Duration};

use crate::model::{dns::Domain, endpoint::Host};

/// Lookup a host by name or IP address string.
pub async fn lookup_host(host: &str, timeout: Duration) -> Result<Host> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        // Reverse lookup for IP address
        let hostname = reverse_lookup(ip, timeout)
            .await
            .unwrap_or_else(|| ip.to_string());
        Ok(Host {
            hostname: Some(hostname),
            ip,
        })
    } else {
        // Resolve hostname to IP address
        let ips = resolve_ip(host, timeout)
            .await
            .with_context(|| format!("failed to resolve host {host}"))?;
        match ips.first() {
            Some(ip) => Ok(Host {
                hostname: Some(host.to_string()),
                ip: *ip,
            }),
            None => Err(anyhow::anyhow!("failed to resolve host")),
        }
    }
}

/// Lookup a domain and return its associated IP addresses.
pub async fn lookup_domain(hostname: &str, timeout: Duration) -> Domain {
    let ips = lookup_ip(hostname, timeout).await.unwrap_or_default();
    Domain {
        name: hostname.to_string(),
        ips,
    }
}

/// Perform a DNS lookup for the given hostname with a timeout.
pub async fn lookup_ip(hostname: &str, timeout: Duration) -> Option<Vec<IpAddr>> {
    match resolve_ip(hostname, timeout).await {
        Ok(ips) => Some(ips),
        Err(error) => {
            tracing::warn!(hostname, error = %format!("{error:#}"), "DNS lookup failed");
            None
        }
    }
}

async fn resolve_ip(hostname: &str, timeout: Duration) -> Result<Vec<IpAddr>> {
    let resolver = resolver::get_resolver()?.with_timeout(timeout);
    let lookup = tokio::time::timeout(timeout, resolver.lookup_ip(hostname))
        .await
        .with_context(|| format!("DNS lookup timed out after {} ms", timeout.as_millis()))?
        .context("DNS query failed")?;
    let ips: Vec<_> = lookup.iter().collect();
    anyhow::ensure!(!ips.is_empty(), "DNS response contained no IP addresses");
    Ok(ips)
}

/// Perform a reverse DNS lookup for the given IP address with a timeout.
pub async fn reverse_lookup(ip: IpAddr, timeout: Duration) -> Option<String> {
    let resolver = resolver::get_resolver().ok()?.with_timeout(timeout);
    reverse_lookup_with_resolver(&resolver, ip, timeout).await
}

async fn reverse_lookup_with_resolver(
    resolver: &pool::ResolverPool,
    ip: IpAddr,
    timeout: Duration,
) -> Option<String> {
    match tokio::time::timeout(timeout, async move { resolver.reverse_lookup(ip).await }).await {
        Ok(Ok(names)) => names
            .answers()
            .iter()
            .find_map(|record| match &record.data {
                hickory_proto::rr::RData::PTR(name) => Some(name.to_string()),
                _ => None,
            }),
        _ => None,
    }
}

/// Resolve a mixed list of IP strings and hostnames into concrete hosts.
///
/// - Accepts strings like "192.168.0.1" and "example.com" in the same list.
/// - Hostnames may resolve to multiple IPs; all are returned.
/// - Duplicate IPs are removed while preserving input order as much as possible.
/// - Resolution runs concurrently with a bounded concurrency limit.
pub async fn resolve_hosts(inputs: &[String], timeout: Duration, concurrency: usize) -> Vec<Host> {
    let concurrency = concurrency.max(1);

    let mut out: Vec<Host> = Vec::new();
    let mut seen: HashSet<IpAddr> = HashSet::new();

    // Collect hostnames to resolve concurrently
    let mut hostnames: Vec<String> = Vec::new();
    for s in inputs {
        let t = s.trim();
        if t.is_empty() {
            continue;
        }
        if let Ok(ip) = t.parse::<IpAddr>() {
            if seen.insert(ip) {
                // Keep hostname None here (reverse lookup can be expensive and noisy...)
                out.push(Host { ip, hostname: None });
            }
        } else {
            hostnames.push(t.to_string());
        }
    }

    // Resolve hostnames concurrently
    let mut st = stream::iter(hostnames.into_iter())
        .map(|hn| async move {
            let ips = lookup_ip(&hn, timeout).await.unwrap_or_default();
            (hn, ips)
        })
        .buffer_unordered(concurrency);

    while let Some((hn, ips)) = st.next().await {
        for ip in ips {
            if seen.insert(ip) {
                out.push(Host {
                    ip,
                    hostname: Some(hn.clone()),
                });
            }
        }
    }

    out
}
