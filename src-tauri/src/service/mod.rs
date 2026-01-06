pub mod task;

use std::{sync::Arc, time::Duration};
use tauri::async_runtime;
use tauri::async_runtime::JoinHandle;
use tauri::AppHandle;
use tauri::Emitter;
use tokio::time::interval;

use crate::{
    service::task::{reload_interfaces, update_interface_state},
    state::AppState,
};

/// Spawn a background task that periodically
/// - updates interface stats every second
/// - reloads interface list every 30 seconds
pub fn spawn_supervisor(app: AppHandle, state: Arc<AppState>) -> JoinHandle<()> {
    async_runtime::spawn(async move {
        let mut tick_stats = interval(Duration::from_secs(1));
        let mut tick_ifaces = interval(Duration::from_secs(30));

        if let Err(e) = reload_interfaces(&state).await {
            tracing::warn!("initial reload_interfaces failed: {e}");
        } else {
            let _ = app.emit("interfaces_updated", ());
        }

        loop {
            tokio::select! {
                _ = tick_stats.tick() => {
                    if let Err(e) = update_interface_state(&state).await {
                        tracing::warn!("update_interface_state failed: {e}");
                    } else {
                        let _ = app.emit("stats_updated", ());
                        //tracing::info!("interface stats updated");
                    }
                },
                _ = tick_ifaces.tick() => {
                    if let Err(e) = reload_interfaces(&state).await {
                        tracing::warn!("reload_interfaces failed: {e}");
                    } else {
                        let _ = app.emit("interfaces_updated", ());
                        //tracing::info!("network interfaces reloaded");
                    }
                }
            }
        }
    })
}
