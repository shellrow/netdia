use std::net::IpAddr;

use netdev::Interface;
use tauri::{AppHandle, Emitter};

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
pub async fn port_scan(app: AppHandle, setting: PortScanSetting) -> Result<PortScanReport, String> {
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
    let run_id = uuid::Uuid::new_v4().to_string();
    let token = crate::operation::start_op(OP_PORTSCAN);
    // Start event
    let _ = app.emit(
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
pub async fn cancel_portscan() -> bool {
    crate::operation::cancel_op(OP_PORTSCAN)
}

#[tauri::command]
pub async fn host_scan(app: AppHandle, setting: HostScanRequest) -> Result<HostScanReport, String> {
    let scan_setting: HostScanSetting = HostScanSetting::from_request(setting);
    let run_id = uuid::Uuid::new_v4().to_string();

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

    let token = crate::operation::start_op(OP_HOSTSCAN);

    let _ = app.emit(
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
pub async fn cancel_hostscan() -> bool {
    crate::operation::cancel_op(OP_HOSTSCAN)
}

#[tauri::command]
pub async fn neighbor_scan(app: AppHandle, iface_name: Option<String>) -> Result<(), String> {
    let run_id = uuid::Uuid::new_v4().to_string();
    let token = crate::operation::start_op(OP_NEIGHBORSCAN);

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
pub async fn cancel_neighborscan() -> bool {
    crate::operation::cancel_op(OP_NEIGHBORSCAN)
}

#[tauri::command]
pub async fn get_target_ports(preset: String, user_ports: Vec<u16>) -> Vec<u16> {
    let preset_enum = TargetPortsPreset::from_str(&preset);
    crate::probe::scan::expand_ports(&preset_enum, &user_ports)
}

fn parse_user_ports(text: &str) -> Vec<u16> {
    let mut out = Vec::new();

    for part in text
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if let Ok(port) = part.parse::<u16>() {
            if (1..=65535).contains(&port) {
                out.push(port);
            }
            continue;
        }

        if let Some((lhs, rhs)) = part.split_once('-') {
            let Ok(mut start) = lhs.trim().parse::<u16>() else {
                continue;
            };
            let Ok(mut end) = rhs.trim().parse::<u16>() else {
                continue;
            };

            if start > end {
                std::mem::swap(&mut start, &mut end);
            }

            for port in start..=end {
                if (1..=65535).contains(&port) {
                    out.push(port);
                }
            }
        }
    }

    out.sort_unstable();
    out.dedup();
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
            HostScanTargetPreview {
                estimated_count: targets.len(),
                exceeds_limit: false,
                targets,
            }
        }
    }
}
