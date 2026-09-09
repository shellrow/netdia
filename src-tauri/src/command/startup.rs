use crate::db::DatabaseState;
use tauri::{AppHandle, State};

#[derive(serde::Serialize)]
pub struct StartupStatus {
    pub error: Option<String>,
    pub data_directory: Option<String>,
}

#[tauri::command]
pub fn get_startup_status(db: State<'_, DatabaseState>) -> StartupStatus {
    StartupStatus {
        error: db.startup_error().map(str::to_owned),
        data_directory: crate::fs::get_app_dir_path().map(|path| path.display().to_string()),
    }
}

/// Retry only by restarting, so all platform services use the recovered config.
#[tauri::command]
pub fn retry_startup(app: AppHandle, db: State<'_, DatabaseState>) -> Result<(), String> {
    if db.startup_error().is_none() {
        return Err("There is no startup failure to retry".into());
    }
    app.request_restart();
    Ok(())
}
