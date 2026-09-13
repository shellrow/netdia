use serde::Serialize;

/// Log delivery errors even when a diagnostic deliberately continues after IPC failure.
pub trait EventEmitter {
    fn emit_logged<T: Serialize + Clone>(&self, event: &str, payload: T) -> tauri::Result<()>;
}

impl EventEmitter for tauri::AppHandle {
    fn emit_logged<T: Serialize + Clone>(&self, event: &str, payload: T) -> tauri::Result<()> {
        let result = tauri::Emitter::emit(self, event, payload);
        if let Err(error) = &result {
            tracing::warn!(event, %error, "application event delivery failed");
        }
        result
    }
}
