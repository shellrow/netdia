use std::net::IpAddr;

use netdev::Interface;
use tauri::{AppHandle, Emitter};

use crate::model::scan::{
    HostScanReport, HostScanRequest, HostScanSetting, PortScanProtocol, PortScanReport,
    PortScanSetting, TargetPortsPreset,
};

use crate::operation::{OP_HOSTSCAN, OP_NEIGHBORSCAN, OP_PORTSCAN};
use crate::probe::service::db::service::{
    init_port_probe_db, init_response_signatures_db, init_service_probe_db, init_tcp_service_db,
    init_udp_service_db, PORT_PROBE_DB, RESPONSE_SIGNATURES_DB, SERVICE_PROBE_DB, TCP_SERVICE_DB,
    UDP_SERVICE_DB,
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

    if RESPONSE_SIGNATURES_DB.get().is_none() {
        init_response_signatures_db().map_err(|e| e.to_string())?;
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
    let preset_enum = match preset.as_str() {
        "Custom" => TargetPortsPreset::Custom,
        "Common" => TargetPortsPreset::Common,
        "WellKnown" => TargetPortsPreset::WellKnown,
        "Top1000" => TargetPortsPreset::Top1000,
        "Full" => TargetPortsPreset::Full,
        _ => TargetPortsPreset::Common,
    };
    crate::probe::scan::expand_ports(&preset_enum, &user_ports)
}
