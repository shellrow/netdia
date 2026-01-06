use crate::model::interface::{NetworkInterface, TrafficStats};
use crate::state::SharedState;
use anyhow::Result;
use netdev::ipnet::Ipv4Net;
use netdev::Interface;
use std::collections::HashMap;
use std::time::SystemTime;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::MutexGuard;

/// Get list of network interfaces with extended stats
#[tauri::command]
pub async fn get_network_interfaces(
    state: State<'_, SharedState>,
) -> Result<Vec<NetworkInterface>, String> {
    // Acquire locks
    let ifaces_guard: MutexGuard<'_, HashMap<u32, Interface>> = state.interfaces.lock().await;
    let stats_guard: MutexGuard<'_, HashMap<u32, crate::state::IfStats>> = state.stats.lock().await;

    let mut out = Vec::with_capacity(ifaces_guard.len());

    for (ifindex, iface) in ifaces_guard.iter() {
        let (rx_bytes, tx_bytes, ts_sys) = if let Some(s) = &iface.stats {
            (
                s.rx_bytes,
                s.tx_bytes,
                s.timestamp.unwrap_or_else(SystemTime::now),
            )
        } else {
            (0u64, 0u64, SystemTime::now())
        };

        let (rx_bytes_per_sec, tx_bytes_per_sec) = if let Some(s) = stats_guard.get(ifindex) {
            (s.rx_bytes_per_sec, s.tx_bytes_per_sec)
        } else {
            (0.0, 0.0)
        };

        let stats = TrafficStats {
            rx_bytes,
            tx_bytes,
            rx_bytes_per_sec,
            tx_bytes_per_sec,
            timestamp: ts_sys,
        };

        out.push(NetworkInterface {
            index: iface.index,
            name: iface.name.clone(),
            display_name: crate::net::interface::get_display_name(iface),
            friendly_name: iface.friendly_name.clone(),
            description: iface.description.clone(),
            if_type: iface.if_type,
            mac_addr: iface.mac_addr,
            ipv4: iface.ipv4.clone(),
            ipv6: iface.ipv6.clone(),
            ipv6_scope_ids: iface.ipv6_scope_ids.clone(),
            flags: iface.flags,
            oper_state: iface.oper_state,
            transmit_speed: iface.transmit_speed,
            receive_speed: iface.receive_speed,
            stats: stats,
            gateway: iface.gateway.clone(),
            dns_servers: iface.dns_servers.clone(),
            mtu: iface.mtu,
            default: iface.default,
        });
    }

    Ok(out)
}

/// Reload network interfaces and notify UI
#[tauri::command]
pub async fn reload_interfaces(
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<(), String> {
    crate::service::task::reload_interfaces(&state)
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit("interfaces_updated", ());
    Ok(())
}

#[tauri::command]
pub async fn get_default_network_interface() -> Result<NetworkInterface, String> {
    match netdev::get_default_interface() {
        Ok(iface) => Ok(NetworkInterface::from(iface)),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn get_network_address_map() -> Result<HashMap<String, Ipv4Net>, String> {
    // Key: Interface name, Value: Network address
    let mut map: HashMap<String, Ipv4Net> = HashMap::new();

    // Get the list of network interfaces
    let interfaces = netdev::get_interfaces();

    // Populate the map with the interface names and their corresponding network addresses
    for iface in interfaces {
        if !iface.is_oper_up() || iface.gateway.is_none() {
            continue;
        }
        let name: String = if let Some(friendly_name) = &iface.friendly_name {
            friendly_name.clone()
        } else {
            iface.name.clone()
        };
        if let Some(ipv4) = iface.ipv4.first() {
            let network =
                Ipv4Net::new(ipv4.network(), ipv4.prefix_len()).map_err(|e| e.to_string())?;
            map.insert(name, network);
        }
    }

    Ok(map)
}
