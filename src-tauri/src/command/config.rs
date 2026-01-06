use std::path::{Path, PathBuf};
use tauri::State;
use tokio::sync::RwLock;

use crate::config::AppConfig;

#[derive(Default)]
pub struct ConfigState(pub RwLock<AppConfig>);

#[tauri::command]
pub async fn get_config(state: State<'_, ConfigState>) -> Result<AppConfig, String> {
    // Return in-memory if already loaded, else load from disk once.
    let cfg = {
        let read = state.0.read().await;
        read.clone()
    };
    Ok(cfg)
}

#[tauri::command]
pub async fn reload_config(state: State<'_, ConfigState>) -> Result<AppConfig, String> {
    let cfg = AppConfig::load();
    {
        let mut write = state.0.write().await;
        *write = cfg.clone();
    }
    Ok(cfg)
}

#[tauri::command]
pub async fn save_config(state: State<'_, ConfigState>, cfg: AppConfig) -> Result<(), String> {
    // Persist to disk + update in-memory
    cfg.save();
    {
        let mut write = state.0.write().await;
        *write = cfg;
    }
    Ok(())
}

#[derive(serde::Serialize)]
pub struct LogsPath {
    pub folder: String,
    pub file: Option<String>,
}

#[tauri::command]
pub async fn logs_dir_path(
    state: tauri::State<'_, super::config::ConfigState>,
) -> Result<LogsPath, String> {
    let cfg = state.0.read().await.clone();

    // parent folder of logging.file_path, or fallback to ~/.netdia
    let (folder, maybe_file): (Option<PathBuf>, Option<PathBuf>) =
        if let Some(fp) = cfg.logging.file_path.as_deref() {
            let p = Path::new(fp);
            (p.parent().map(|pp| pp.to_path_buf()), Some(p.to_path_buf()))
        } else {
            (None, None)
        };

    // fallback to app dir
    let folder = folder
        .or_else(|| crate::fs::get_app_dir_path())
        .ok_or("Failed to resolve logs folder path")?;

    let folder_str = folder.to_string_lossy().to_string();
    let file_str = maybe_file.map(|p| p.to_string_lossy().to_string());

    Ok(LogsPath {
        folder: folder_str,
        file: file_str,
    })
}
