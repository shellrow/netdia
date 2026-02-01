use std::net::IpAddr;

use anyhow::Result;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::model::scan::{
    NeighborHost, NeighborScanCancelledPayload, NeighborScanErrorPayload, NeighborScanReport,
    NeighborScanStartPayload,
};

pub async fn neighbor_scan(
    app: &AppHandle,
    run_id: &str,
    iface: netdev::Interface,
    token: CancellationToken,
) -> Result<NeighborScanReport> {
    let app = app.clone();
    let run_id = run_id.to_string();

    let _ = app.emit(
        "neighborscan:start",
        NeighborScanStartPayload {
            run_id: run_id.clone(),
        },
    );

    let src_ipv4_opt = iface.ipv4_addrs().into_iter().next().map(IpAddr::V4);
    let src_ipv6_opt = iface.ipv6_addrs().into_iter().next().map(IpAddr::V6);

    let _ = app.emit(
        "hostscan:start",
        crate::model::scan::HostScanStartPayload {
            run_id: run_id.clone(),
        },
    );

    let setting = crate::model::scan::HostScanSetting::neighbor_scan_default(&iface);

    let hostscan_result = match crate::probe::scan::icmp::host_scan(
        &app,
        &run_id,
        src_ipv4_opt,
        src_ipv6_opt,
        setting,
        token.clone(),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            if token.is_cancelled() {
                let _ = app.emit(
                    "neighborscan:cancelled",
                    NeighborScanCancelledPayload {
                        run_id: run_id.clone(),
                    },
                );
            } else {
                let _ = app.emit(
                    "neighborscan:error",
                    NeighborScanErrorPayload {
                        run_id: run_id.clone(),
                        message: e.to_string(),
                    },
                );
            }
            return Err(e);
        }
    };

    if token.is_cancelled() {
        let _ = app.emit(
            "neighborscan:cancelled",
            NeighborScanCancelledPayload {
                run_id: run_id.clone(),
            },
        );
        return Err(anyhow::anyhow!("cancelled"));
    }

    let neigh_table = crate::net::neigh::get_neighbor_table()?;
    let oui_db = ndb_oui::OuiDb::bundled();
    let self_ips: Vec<IpAddr> = iface.ip_addrs();

    let mut neighbors: Vec<NeighborHost> = Vec::new();

    for (host, rtt) in hostscan_result.alive {
        let mac_addr = neigh_table.get(&host.ip).cloned();
        let vendor = match mac_addr {
            Some(mac) => oui_db
                .lookup_mac(&mac)
                .map(|o| o.vendor_detail.clone())
                .flatten(),
            None => None,
        };

        let mut tags = Vec::new();
        if self_ips.contains(&host.ip) {
            tags.push("Self".to_string());
        }
        if let Some(gw) = &iface.gateway {
            match host.ip {
                IpAddr::V4(ipv4) if gw.ipv4.contains(&ipv4) => tags.push("Gateway".to_string()),
                IpAddr::V6(ipv6) if gw.ipv6.contains(&ipv6) => tags.push("Gateway".to_string()),
                _ => {}
            }
        }
        if iface.dns_servers.contains(&host.ip) {
            tags.push("DNS".to_string());
        }

        neighbors.push(NeighborHost {
            ip_addr: host.ip,
            mac_addr,
            vendor,
            rtt_ms: Some(rtt),
            tags,
        });
    }

    let report = NeighborScanReport {
        run_id: run_id.clone(),
        neighbors,
        total: hostscan_result.total,
    };

    let _ = app.emit("neighborscan:done", report.clone());
    Ok(report)
}
