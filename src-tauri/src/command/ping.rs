use std::net::IpAddr;

use netdev::Interface;
use tauri::{AppHandle, Emitter};

use crate::model::ping::{PingProtocol, PingSetting, PingStartPayload, PingStat};
use crate::probe::ping;

#[tauri::command]
pub async fn ping(app: AppHandle, setting: PingSetting) -> Result<PingStat, String> {
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
    // Start event
    let _ = app.emit(
        "ping:start",
        PingStartPayload {
            run_id: run_id.clone(),
            setting: setting.clone(),
        },
    );
    match setting.protocol {
        PingProtocol::Icmp => ping::icmp::icmp_ping(&app, &run_id, src_ip, setting)
            .await
            .map_err(|e| e.to_string()),
        PingProtocol::Tcp => ping::tcp::tcp_ping(&app, &run_id, src_ip, setting)
            .await
            .map_err(|e| e.to_string()),
        PingProtocol::Udp => ping::udp::udp_ping_icmp_unreach(&app, &run_id, src_ip, setting)
            .await
            .map_err(|e| e.to_string()),
        PingProtocol::Quic => ping::quic::quic_ping(&app, &run_id, src_ip, setting)
            .await
            .map_err(|e| e.to_string()),
        PingProtocol::Http => ping::http::http_ping(&app, &run_id, setting)
            .await
            .map_err(|e| e.to_string()),
    }
}
