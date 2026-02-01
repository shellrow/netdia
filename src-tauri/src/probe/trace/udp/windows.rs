use crate::model::trace::TracerouteSetting;
use anyhow::Result;
use std::net::IpAddr;
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

pub async fn udp_traceroute(
    _app: &AppHandle,
    _run_id: &str,
    _src_ip: IpAddr,
    _setting: &TracerouteSetting,
    _token: CancellationToken,
) -> Result<bool> {
    // Currently, windows is not supported for UDP traceroute via ICMP Port Unreachable
    // because it requires enabling promiscuous mode on ICMP socket.
    // and it needs admin privileges.
    // For cross-platform, non-admin, and not rely on npcap/winpcap, we skip implementing this feature on Windows.
    // For now, just return an error.
    Err(anyhow::anyhow!(
        "UDP traceroute is not supported on Windows (ICMP capture limitation)."
    ))
}
