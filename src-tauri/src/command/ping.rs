use crate::events::EventEmitter;
use std::net::IpAddr;

use netdev::Interface;
use tauri::AppHandle;

use crate::model::ping::{PingErrorPayload, PingProtocol, PingSetting, PingStartPayload};
use crate::operation::OP_PING;
use crate::probe::ping;

#[tauri::command]
pub async fn ping(app: AppHandle, run_id: String, setting: PingSetting) -> Result<(), String> {
    let operation = crate::operation::claim_op(OP_PING, &run_id)?;
    super::validation::validate_ping(&setting)?;
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

    let token = operation.token.clone();

    let _ = app.emit_logged(
        "ping:start",
        PingStartPayload {
            run_id: run_id.clone(),
            setting: setting.clone(),
        },
    );

    tauri::async_runtime::spawn(async move {
        let _operation = operation;
        let cancellation = token.clone();
        let work = async {
            match setting.protocol {
                PingProtocol::Icmp => {
                    ping::icmp::icmp_ping(&app, &run_id, src_ip, setting, token).await
                }
                PingProtocol::Tcp => {
                    ping::tcp::tcp_ping(&app, &run_id, src_ip, setting, token).await
                }
                PingProtocol::Udp => {
                    ping::udp::udp_ping_icmp_unreach(&app, &run_id, src_ip, setting, token).await
                }
                PingProtocol::Quic => {
                    ping::quic::quic_ping(&app, &run_id, src_ip, setting, token).await
                }
                PingProtocol::Http => ping::http::http_ping(&app, &run_id, setting, token).await,
            }
        };
        let res = tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(anyhow::anyhow!("cancelled")),
            result = work => result,
        };

        if let Err(e) = res {
            if cancellation.is_cancelled() {
                let _ = app.emit_logged("ping:cancelled", serde_json::json!({ "run_id": run_id }));
                return;
            }
            let _ = app.emit_logged(
                "ping:error",
                PingErrorPayload {
                    run_id: run_id.clone(),
                    message: e.to_string(),
                },
            );
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn cancel_ping(run_id: String) -> bool {
    crate::operation::cancel_op(OP_PING, &run_id).await
}
