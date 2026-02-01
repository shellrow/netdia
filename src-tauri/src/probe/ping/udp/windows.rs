use std::net::IpAddr;
use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tauri::AppHandle;
use crate::model::ping::{PingSetting, PingStat};

pub async fn udp_ping_icmp_unreach(
    _app: &AppHandle,
    _run_id: &str,
    _src_ip: IpAddr,
    _setting: PingSetting,
    _token: CancellationToken
) -> Result<PingStat> {
    // Currently, windows is not supported for UDP ping via ICMP Port Unreachable
    // because it requires enabling promiscuous mode on ICMP socket.
    // and it needs admin privileges.
    // For cross-platform, non-admin, and not rely on npcap/winpcap, we skip implementing this feature on Windows.
    // For now, just return an error.
    return Err(anyhow::anyhow!(
        "UDP ping via ICMP Port Unreachable is not supported on Windows."
    ));
}
