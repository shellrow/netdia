use tauri::AppHandle;

use crate::{
    model::speedtest::{SpeedtestDonePayload, SpeedtestResult, SpeedtestSetting},
    net::{self, speedtest::MAX_DURATION},
    operation::{cancel_op, claim_op, RunEmitter, OP_SPEEDTEST},
};

#[tauri::command]
pub async fn start_speedtest(
    app: AppHandle,
    run_id: String,
    setting: SpeedtestSetting,
) -> Result<(), String> {
    let operation = claim_op(OP_SPEEDTEST, &run_id)?;
    super::validation::validate_speedtest(&setting)?;
    let max_ms = setting
        .max_duration_ms
        .unwrap_or(MAX_DURATION.as_millis() as u64);
    let max = std::time::Duration::from_millis(max_ms);

    tauri::async_runtime::spawn(async move {
        let events = RunEmitter {
            app: &app,
            run_id: &run_id,
        };
        let result = tokio::select! {
            biased;
            _ = operation.token.cancelled() => Err(anyhow::anyhow!("cancelled")),
            result = net::speedtest::run_speedtest(
                &events, setting.direction.clone(), setting.test_type, setting.target_bytes, max,
            ) => result,
        };
        if let Err(error) = result {
            let _ = events.emit(
                "speedtest:done",
                SpeedtestDonePayload {
                    direction: setting.direction,
                    result: if operation.token.is_cancelled() {
                        SpeedtestResult::Canceled
                    } else {
                        SpeedtestResult::Error
                    },
                    elapsed_ms: 0,
                    transferred_bytes: 0,
                    target_bytes: setting.target_bytes,
                    avg_mbps: 0.0,
                    message: Some(error.to_string()),
                },
            );
        }
        drop(operation);
    });
    Ok(())
}

#[tauri::command]
pub async fn stop_speedtest(run_id: String) -> bool {
    cancel_op(OP_SPEEDTEST, &run_id).await
}
