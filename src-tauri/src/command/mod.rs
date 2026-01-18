pub mod config;
pub mod dns;
pub mod interfaces;
pub mod internet;
pub mod ping;
pub mod routes;
pub mod scan;
pub mod socket;
pub mod system;
pub mod trace;
pub mod speedtest;
pub mod latency;

use crate::model::AppInfo;

/// Get application information
#[tauri::command]
pub async fn about() -> AppInfo {
    AppInfo::current()
}
