use std::net::IpAddr;

use anyhow::Result;
use netdev::Interface;
use tauri::{AppHandle, Emitter};

use crate::operation::OP_TRACEROUTE;
use crate::probe::trace;
use crate::model::trace::{TraceErrorPayload, TraceProtocol, TraceStartPayload, TracerouteSetting};

fn sanitize_setting(mut setting: TracerouteSetting) -> TracerouteSetting {
    if setting.max_hops == 0 {
        setting.max_hops = 30;
    }
    if setting.tries_per_hop == 0 {
        setting.tries_per_hop = 1;
    }
    setting
}

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

    let setting = sanitize_setting(setting);

    let run_id = uuid::Uuid::new_v4().to_string();

    let token = crate::operation::start_op(OP_TRACEROUTE);

    let _ = app.emit(
        "traceroute:start",
        TraceStartPayload {
            run_id: run_id.clone(),
            setting: setting.clone(),
        },
    );

    tauri::async_runtime::spawn(async move {
        let res = match setting.protocol {
            TraceProtocol::Icmp => trace::icmp::icmp_traceroute(&app, &run_id, src_ip, &setting, token).await,
            TraceProtocol::Udp => trace::udp::udp_traceroute(&app, &run_id, src_ip, &setting, token).await,
        };

        match res {
            Ok(reached) => {
                // Send done event
                app.emit(
                    "traceroute:done",
                    &serde_json::json!({
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
                // Emit error event
                let _ = app.emit(
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
pub async fn cancel_traceroute() -> bool {
    crate::operation::cancel_op(OP_TRACEROUTE)
}
