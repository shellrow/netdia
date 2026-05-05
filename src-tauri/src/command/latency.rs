use std::sync::Arc;
use tauri::{AppHandle, State};

use crate::{
    model::speedtest::SpeedtestServer,
    net::{self, latency::DEFAULT_PING_COUNT},
    state::AppState,
};

#[tauri::command]
pub async fn measure_latency(
    app: AppHandle,
    _state: State<'_, Arc<AppState>>,
    server: SpeedtestServer,
) -> Result<(), String> {
    net::latency::measure_latency_jitter(&app, server, DEFAULT_PING_COUNT)
        .await
        .map_err(|e| e.to_string())
}
