pub mod config;
pub mod dns;
pub mod interfaces;
pub mod internet;
pub mod latency;
pub mod ping;
pub mod routes;
pub mod scan;
pub mod socket;
pub mod speedtest;
pub mod system;
pub mod trace;
pub mod updater;

use crate::model::AppInfo;

/// Get application information
#[tauri::command]
pub async fn about() -> AppInfo {
    AppInfo::current()
}
