use std::net::IpAddr;

use anyhow::Result;
use netdev::Interface;
use tauri::{AppHandle, Emitter};

use crate::probe::trace::{self, TracerouteSetting};

#[tauri::command]
pub async fn traceroute(app: AppHandle, setting: TracerouteSetting) -> Result<(), String> {
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

    if let Err(e) = trace::traceroute(&app, src_ip, setting).await {
        // Emit error event
        let _ = app.emit(
            "traceroute:error",
            &serde_json::json!({ "message": e.to_string() }),
        );
    }

    Ok(())
}
