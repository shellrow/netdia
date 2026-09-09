use crate::{
    net::{self, latency::DEFAULT_PING_COUNT},
    operation::{claim_op, RunEmitter, OP_LATENCY},
};
use tauri::AppHandle;

#[tauri::command]
pub async fn measure_latency(app: AppHandle, run_id: String) -> Result<(), String> {
    let operation = claim_op(OP_LATENCY, &run_id)?;
    let events = RunEmitter {
        app: &app,
        run_id: &run_id,
    };
    tokio::select! {
        biased;
        _ = operation.token.cancelled() => Err("cancelled".to_string()),
        result = net::latency::measure_latency_jitter(&events, DEFAULT_PING_COUNT) =>
            result.map_err(|error| error.to_string()),
    }
}
