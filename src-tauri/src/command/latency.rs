use std::sync::Arc;
use tauri::{AppHandle, State};

use crate::{net::{self, latency::DEFAULT_PING_COUNT}, state::AppState};

#[tauri::command]
pub async fn measure_latency(app: AppHandle, _state: State<'_, Arc<AppState>>) -> Result<(), String> {
    net::latency::measure_latency_jitter(&app, DEFAULT_PING_COUNT).await.map_err(|e| e.to_string())
}
