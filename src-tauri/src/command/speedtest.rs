use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::{
    model::speedtest::{SpeedtestDonePayload, SpeedtestResult, SpeedtestSetting},
    net::{self, speedtest::MAX_DURATION},
    state::AppState,
};

#[tauri::command]
pub async fn start_speedtest(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    setting: SpeedtestSetting,
) -> Result<(), String> {
    // If a speedtest is already running, abort it
    {
        let mut h = state.speedtest_task.lock().await;
        if let Some(handle) = h.take() {
            handle.abort();
        }
        let mut last = state.speedtest_last.lock().await;
        *last = Some((setting.direction.clone(), setting.target_bytes));
    }

    let max_ms = setting.max_duration_ms.unwrap_or(MAX_DURATION.as_millis() as u64);
    let max = std::time::Duration::from_millis(max_ms);

    let app2 = app.clone();
    let state2 = state.inner().clone();

    let handle = tauri::async_runtime::spawn(async move {
        let r = net::speedtest::run_speedtest(&app2, setting.direction.clone(), setting.target_bytes, max).await;

        // Send done event with error
        if let Err(e) = r {
            let _ = app2.emit("speedtest:done", SpeedtestDonePayload{
                direction: setting.direction,
                result: SpeedtestResult::Error,
                elapsed_ms: 0,
                transferred_bytes: 0,
                target_bytes: setting.target_bytes,
                avg_mbps: 0.0,
                message: Some(e.to_string()),
            });
        }

        // Clear handle
        let mut h = state2.speedtest_task.lock().await;
        *h = None;
    });

    {
        let mut h = state.speedtest_task.lock().await;
        *h = Some(handle);
    }

    Ok(())
}

#[tauri::command]
pub async fn stop_speedtest(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let last = { state.speedtest_last.lock().await.clone() };

    let aborted = {
        let mut h = state.speedtest_task.lock().await;
        if let Some(handle) = h.take() {
            handle.abort();
            true
        } else {
            false
        }
    };

    // Notify canceled
    if aborted {
        if let Some((direction, target_bytes)) = last {
            let _ = app.emit("speedtest:done", SpeedtestDonePayload{
                direction,
                result: SpeedtestResult::Canceled,
                elapsed_ms: 0,
                transferred_bytes: 0,
                target_bytes,
                avg_mbps: 0.0,
                message: None,
            });
        }
    }

    Ok(())
}
