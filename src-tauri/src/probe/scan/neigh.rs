use std::net::IpAddr;

use anyhow::Result;
use tauri::{AppHandle, Emitter};

use crate::model::scan::{NeighborHost, NeighborScanReport};

pub async fn neighbor_scan(
    app: &AppHandle,
    run_id: &str,
    iface: netdev::Interface,
) -> Result<NeighborScanReport> {
    //let iface = netdev::get_default_interface().map_err(|e| anyhow::anyhow!("Failed to get default interface: {}", e))?;
    let src_ipv4_opt = iface
        .ipv4_addrs()
        .into_iter()
        .next()
        .map(std::net::IpAddr::V4);
    let src_ipv6_opt = iface
        .ipv6_addrs()
        .into_iter()
        .next()
        .map(std::net::IpAddr::V6);

    let _ = app.emit(
        "hostscan:start",
        crate::model::scan::HostScanStartPayload {
            run_id: run_id.to_string(),
        },
    );

    let setting = crate::model::scan::HostScanSetting::neighbor_scan_default(&iface);

    // Perform host scan
    // hostscan:progress and hostscan:done events will be emitted during the scan
    let hostscan_result =
        crate::probe::scan::icmp::host_scan(&app, &run_id, src_ipv4_opt, src_ipv6_opt, setting)
            .await?;

    let neigh_table = crate::net::neigh::get_neighbor_table()?;

    let oui_db = ndb_oui::OuiDb::bundled();
    let self_ips: Vec<IpAddr> = iface.ip_addrs();

    let mut neighbors: Vec<NeighborHost> = Vec::new();

    for (host, rtt) in hostscan_result.alive {
        let mac_addr = neigh_table.get(&host.ip).cloned();
        let vendor = match mac_addr {
            Some(mac) => match oui_db.lookup_mac(&mac) {
                Some(oui_info) => oui_info.vendor_detail.clone(),
                None => None,
            },
            None => None,
        };

        // Classify tags
        let mut tags = Vec::new();
        if self_ips.contains(&host.ip) {
            tags.push("Self".to_string());
        }
        if let Some(gw) = &iface.gateway {
            match host.ip {
                IpAddr::V4(ipv4) => {
                    if gw.ipv4.contains(&ipv4) {
                        tags.push("Gateway".to_string());
                    }
                }
                IpAddr::V6(ipv6) => {
                    if gw.ipv6.contains(&ipv6) {
                        tags.push("Gateway".to_string());
                    }
                }
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

    let total = hostscan_result.total;

    let _ = app.emit("neighborscan:done", run_id.to_string());

    Ok(NeighborScanReport {
        run_id: run_id.to_string(),
        neighbors,
        total,
    })
}
