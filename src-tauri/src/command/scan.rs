use crate::events::EventEmitter;
use std::net::IpAddr;

use netdev::Interface;
use tauri::AppHandle;

use crate::model::scan::{
    HostScanReport, HostScanRequest, HostScanSetting, HostScanTargetPreview, PortInputPreview,
    PortScanProtocol, PortScanReport, PortScanSetting, TargetPortsPreset,
};

use crate::operation::{OP_HOSTSCAN, OP_NEIGHBORSCAN, OP_PORTSCAN};
use crate::probe::service::db::service::{
    init_port_probe_db, init_service_probe_db, init_tcp_service_db, init_udp_service_db,
    PORT_PROBE_DB, SERVICE_PROBE_DB, TCP_SERVICE_DB, UDP_SERVICE_DB,
};
use crate::probe::service::db::tls::{init_tls_oid_map, TLS_OID_MAP};

#[tauri::command]
pub async fn init_probe_db() -> Result<(), String> {
    static INIT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _initialization = INIT.lock().await;
    // Initialize service databases if not already initialized

    if TCP_SERVICE_DB.get().is_none() {
        init_tcp_service_db().map_err(|e| e.to_string())?;
    }

    if UDP_SERVICE_DB.get().is_none() {
        init_udp_service_db().map_err(|e| e.to_string())?;
    }

    if TLS_OID_MAP.get().is_none() {
        init_tls_oid_map().map_err(|e| e.to_string())?;
    }

    if PORT_PROBE_DB.get().is_none() {
        init_port_probe_db().map_err(|e| e.to_string())?;
    }

    if SERVICE_PROBE_DB.get().is_none() {
        init_service_probe_db().map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn port_scan(
    app: AppHandle,
    run_id: String,
    setting: PortScanSetting,
) -> Result<PortScanReport, String> {
    let operation = crate::operation::claim_op(OP_PORTSCAN, &run_id)?;
    let token = operation.token.clone();
    super::validation::validate_port_scan(&setting)?;
    init_probe_db().await?;
    let default_interface: Interface = netdev::get_default_interface()
        .map_err(|e| format!("Failed to get default interface: {}", e))?;
    let src_ip = match setting.ip_addr {
        std::net::IpAddr::V4(_) => {
            // Pick first IPv4 address of default interface
            let ipv4 = default_interface
                .ipv4_addrs()
                .into_iter()
                .next()
                .ok_or("No IPv4 address found on default interface")?;
            IpAddr::V4(ipv4)
        }
        std::net::IpAddr::V6(_) => {
            // Pick first IPv6 address of default interface
            let ipv6 = default_interface
                .ipv6_addrs()
                .into_iter()
                .next()
                .ok_or("No IPv6 address found on default interface")?;
            IpAddr::V6(ipv6)
        }
    };

    // Start event
    let _ = app.emit_logged(
        "portscan:start",
        crate::model::scan::PortScanStartPayload {
            run_id: run_id.clone(),
        },
    );

    match setting.protocol {
        PortScanProtocol::Tcp => {
            crate::probe::scan::tcp::port_scan(&app, &run_id, src_ip, setting, token)
                .await
                .map_err(|e| e.to_string())
        }
        PortScanProtocol::Quic => {
            crate::probe::scan::quic::port_scan(&app, &run_id, src_ip, setting, token)
                .await
                .map_err(|e| e.to_string())
        }
    }
}

#[tauri::command]
pub async fn cancel_portscan(run_id: String) -> bool {
    crate::operation::cancel_op(OP_PORTSCAN, &run_id).await
}

#[tauri::command]
pub async fn host_scan(
    app: AppHandle,
    run_id: String,
    setting: HostScanRequest,
) -> Result<HostScanReport, String> {
    let operation = crate::operation::claim_op(OP_HOSTSCAN, &run_id)?;
    let token = operation.token.clone();
    super::validation::validate_host_scan(&setting)?;
    let scan_setting: HostScanSetting = HostScanSetting::from_request(setting);

    let default_if = netdev::get_default_interface().map_err(|e| e.to_string())?;

    let src_ipv4_opt = default_if
        .ipv4_addrs()
        .into_iter()
        .next()
        .map(std::net::IpAddr::V4);
    let src_ipv6_opt = default_if
        .ipv6_addrs()
        .into_iter()
        .next()
        .map(std::net::IpAddr::V6);

    let _ = app.emit_logged(
        "hostscan:start",
        crate::model::scan::HostScanStartPayload {
            run_id: run_id.clone(),
        },
    );
    crate::probe::scan::icmp::host_scan(
        &app,
        &run_id,
        src_ipv4_opt,
        src_ipv6_opt,
        scan_setting,
        token,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cancel_hostscan(run_id: String) -> bool {
    crate::operation::cancel_op(OP_HOSTSCAN, &run_id).await
}

#[tauri::command]
pub async fn neighbor_scan(
    app: AppHandle,
    run_id: String,
    iface_name: Option<String>,
) -> Result<(), String> {
    let operation = crate::operation::claim_op(OP_NEIGHBORSCAN, &run_id)?;
    let token = operation.token.clone();

    let iface = if let Some(name) = iface_name {
        netdev::get_interfaces()
            .into_iter()
            .find(|i| i.name == name || i.friendly_name.as_deref() == Some(&name))
            .ok_or_else(|| format!("interface not found: {name}"))?
    } else {
        netdev::get_default_interface().map_err(|e| e.to_string())?
    };

    crate::probe::scan::neigh::neighbor_scan(&app, &run_id, iface, token)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cancel_neighborscan(run_id: String) -> bool {
    crate::operation::cancel_op(OP_NEIGHBORSCAN, &run_id).await
}

#[tauri::command]
pub async fn get_target_ports(preset: String, user_ports: Vec<u16>) -> Vec<u16> {
    let preset_enum = TargetPortsPreset::from_str(&preset);
    crate::probe::scan::expand_ports(&preset_enum, &user_ports)
}

fn parse_user_ports(text: &str) -> Vec<u16> {
    let mut ranges = Vec::new();
    for part in text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|part| !part.is_empty())
    {
        let range = if let Ok(port) = part.parse::<u16>() {
            (port, port)
        } else if let Some((lhs, rhs)) = part.split_once('-') {
            let (Ok(start), Ok(end)) = (lhs.parse::<u16>(), rhs.parse::<u16>()) else {
                continue;
            };
            (start.min(end), start.max(end))
        } else {
            continue;
        };
        if range.1 != 0 {
            ranges.push((range.0.max(1), range.1));
        }
    }
    ranges.sort_unstable();
    let mut out = Vec::new();
    let mut next = 1u32;
    // Expand only the unseen suffix of each range, never repeated overlaps.
    for (start, end) in ranges {
        let start = u32::from(start).max(next);
        let end = u32::from(end);
        if start <= end {
            out.extend((start..=end).map(|port| port as u16));
            next = end + 1;
        }
    }
    out
}

fn estimate_ipv4_hosts(cidr: &str) -> Option<usize> {
    let net = cidr.trim().parse::<netdev::ipnet::Ipv4Net>().ok()?;
    let prefix = net.prefix_len();
    let size = 1usize.checked_shl(u32::from(32u8.saturating_sub(prefix)))?;

    Some(if prefix <= 30 {
        size.saturating_sub(2)
    } else {
        size
    })
}

fn expand_ipv4_cidr(cidr: &str, max: usize) -> Option<Vec<String>> {
    let net = cidr.trim().parse::<netdev::ipnet::Ipv4Net>().ok()?;
    let total = estimate_ipv4_hosts(cidr)?;
    if total == 0 || total > max {
        return None;
    }

    Some(net.hosts().map(|ip| ip.to_string()).collect())
}

fn parse_target_list(text: &str) -> Vec<String> {
    let mut out: Vec<String> = text
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    out.sort();
    out.dedup();
    out
}

#[tauri::command]
pub async fn preview_port_input(preset: String, user_ports_text: String) -> PortInputPreview {
    let user_ports = parse_user_ports(&user_ports_text);
    let preset_enum = TargetPortsPreset::from_str(&preset);
    let target_ports = crate::probe::scan::expand_ports(&preset_enum, &user_ports);

    PortInputPreview {
        user_ports,
        target_ports,
    }
}

#[tauri::command]
pub async fn preview_host_scan_targets(
    mode: String,
    cidr: String,
    list: String,
    max_expand: usize,
) -> HostScanTargetPreview {
    let max_expand = max_expand.min(super::validation::MAX_HOST_SCAN_TARGETS);
    match mode.as_str() {
        "cidr" => {
            let estimated_count = estimate_ipv4_hosts(&cidr).unwrap_or(0);
            let exceeds_limit = estimated_count > max_expand;
            let targets = if exceeds_limit {
                Vec::new()
            } else {
                expand_ipv4_cidr(&cidr, max_expand).unwrap_or_default()
            };

            HostScanTargetPreview {
                targets,
                estimated_count,
                exceeds_limit,
            }
        }
        _ => {
            let targets = parse_target_list(&list);
            let estimated_count = targets.len();
            let exceeds_limit = estimated_count > max_expand;
            HostScanTargetPreview {
                estimated_count,
                exceeds_limit,
                targets: if exceeds_limit { Vec::new() } else { targets },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn probe_initialization_is_repeatable_and_concurrent() {
        let (first, second) = tokio::join!(init_probe_db(), init_probe_db());
        first.unwrap();
        second.unwrap();
        assert!(!crate::probe::service::db::service::port_probe_db().is_empty());
        assert!(!crate::probe::service::db::service::service_probe_db().is_empty());
    }

    #[test]
    fn parses_ports_ranges_and_removes_duplicates() {
        assert_eq!(
            parse_user_ports("443, 80, 82-80, 443, invalid"),
            vec![80, 81, 82, 443]
        );
    }

    #[tokio::test]
    async fn preview_enforces_server_limit() {
        let preview =
            preview_host_scan_targets("cidr".into(), "0.0.0.0/0".into(), String::new(), usize::MAX)
                .await;
        assert!(preview.exceeds_limit);
        assert!(preview.targets.is_empty());
        let preview =
            preview_host_scan_targets("list".into(), String::new(), "a,b,c".into(), 2).await;
        assert!(preview.exceeds_limit);
        assert_eq!(preview.estimated_count, 3);
        assert!(preview.targets.is_empty());
    }

    #[test]
    fn repeated_full_port_ranges_are_deduplicated() {
        let ports = parse_user_ports("1-65535,1-65535");
        assert_eq!(ports.len(), 65535);
        assert_eq!(ports[0], 1);
        assert_eq!(ports[65534], 65535);
    }

    #[test]
    fn estimates_usable_ipv4_hosts() {
        assert_eq!(estimate_ipv4_hosts("192.0.2.0/24"), Some(254));
        assert_eq!(estimate_ipv4_hosts("192.0.2.0/31"), Some(2));
        assert_eq!(estimate_ipv4_hosts("invalid"), None);
    }

    #[test]
    fn parses_and_deduplicates_host_lists() {
        assert_eq!(
            parse_target_list("example.com, 192.0.2.1; example.com"),
            vec!["192.0.2.1".to_string(), "example.com".to_string()]
        );
    }
}
