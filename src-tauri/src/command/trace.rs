use crate::events::EventEmitter;
use std::net::IpAddr;

use anyhow::Result;
use netdev::Interface;
use tauri::AppHandle;

use crate::model::trace::{TraceErrorPayload, TraceProtocol, TraceStartPayload, TracerouteSetting};
use crate::operation::OP_TRACEROUTE;
use crate::probe::trace;

#[tauri::command]
pub async fn traceroute(
    app: AppHandle,
    run_id: String,
    setting: TracerouteSetting,
) -> Result<(), String> {
    let operation = crate::operation::claim_op(OP_TRACEROUTE, &run_id)?;
    super::validation::validate_traceroute(&setting)?;
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
        "traceroute:start",
        TraceStartPayload {
            run_id: run_id.clone(),
            setting: setting.clone(),
        },
    );

    tauri::async_runtime::spawn(async move {
        let _operation = operation;
        let cancellation = token.clone();
        let work = async {
            match setting.protocol {
                TraceProtocol::Icmp => {
                    trace::icmp::icmp_traceroute(&app, &run_id, src_ip, &setting, token).await
                }
                TraceProtocol::Udp => {
                    trace::udp::udp_traceroute(&app, &run_id, src_ip, &setting, token).await
                }
            }
        };
        let res = tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(anyhow::anyhow!("cancelled")),
            result = work => result,
        };

        match res {
            Ok(reached) => {
                // Send done event
                app.emit_logged(
                    "traceroute:done",
                    &serde_json::json!({
                        "run_id": run_id,
                        "reached": reached,
                        "hops": setting.max_hops,
                        "ip_addr": setting.ip_addr,
                        "hostname": setting.hostname,
                        "protocol": setting.protocol,
                    }),
                )
                .ok();
            }
            Err(e) => {
                if cancellation.is_cancelled() {
                    let _ = app.emit_logged(
                        "traceroute:cancelled",
                        serde_json::json!({ "run_id": run_id }),
                    );
                    return;
                }
                // Emit error event
                let _ = app.emit_logged(
                    "traceroute:error",
                    TraceErrorPayload {
                        run_id: run_id.clone(),
                        message: e.to_string(),
                    },
                );
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn cancel_traceroute(run_id: String) -> bool {
    crate::operation::cancel_op(OP_TRACEROUTE, &run_id).await
}
