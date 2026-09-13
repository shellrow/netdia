use crate::events::EventEmitter;
pub mod task;

use std::{sync::Arc, time::Duration};
use tauri::async_runtime;
use tauri::async_runtime::JoinHandle;
use tauri::AppHandle;
use tauri::Manager;
use tokio::time::interval;

use crate::{
    service::task::{reload_interfaces, update_interface_state},
    state::AppState,
};

/// Spawn a background task that periodically
/// - updates interface stats at the configured refresh interval
/// - reloads interface list every 30 seconds
pub fn spawn_supervisor(app: AppHandle, state: Arc<AppState>) -> JoinHandle<()> {
    async_runtime::spawn(async move {
        let config = app.state::<crate::command::config::ConfigState>();
        let refresh = config.0.read().await.refresh_interval_ms.clamp(100, 60_000);
        let mut tick_stats = interval(Duration::from_millis(refresh));
        let mut tick_ifaces = interval(Duration::from_secs(30));

        if let Err(e) = reload_interfaces(&state).await {
            tracing::warn!("initial reload_interfaces failed: {e}");
        } else {
            let _ = app.emit_logged("interfaces_updated", ());
        }

        loop {
            tokio::select! {
                _ = tick_stats.tick() => {
                    let refresh = config.0.read().await.refresh_interval_ms.clamp(100, 60_000);
                    let period = Duration::from_millis(refresh);
                    if tick_stats.period() != period {
                        tick_stats = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
                    }
                    if let Err(e) = update_interface_state(&state).await {
                        tracing::warn!("update_interface_state failed: {e}");
                    } else {
                        let _ = app.emit_logged("stats_updated", ());
                        //tracing::info!("interface stats updated");
                    }
                },
                _ = tick_ifaces.tick() => {
                    if let Err(e) = reload_interfaces(&state).await {
                        tracing::warn!("reload_interfaces failed: {e}");
                    } else {
                        let _ = app.emit_logged("interfaces_updated", ());
                        //tracing::info!("network interfaces reloaded");
                    }
                }
            }
        }
    })
}
