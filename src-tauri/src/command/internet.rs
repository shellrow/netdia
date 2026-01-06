use crate::model::IpInfoDual;
use crate::net::internet::get_public_ip;

/// Tauri command wrapper
#[tauri::command]
pub async fn get_public_ip_info() -> Result<IpInfoDual, String> {
    get_public_ip().await.map_err(|e| e.to_string())
}
