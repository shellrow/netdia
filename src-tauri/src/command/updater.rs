use serde::Serialize;
use tauri::{ipc::Channel, AppHandle, State};

use tauri_plugin_updater::Update;

#[cfg(all(desktop, not(windows)))]
use tauri_plugin_updater::UpdaterExt;

use std::sync::Mutex;

#[cfg(all(desktop, not(windows)))]
use time::format_description::well_known::Rfc3339;

#[cfg(windows)]
const WINDOWS_STORE_URL: &str = "ms-windows-store://pdp/?productid=9NLQ03PT1DXQ";
//const WINDOWS_STORE_URL: &str = "https://apps.microsoft.com/detail/9NLQ03PT1DXQ";

#[allow(dead_code)]
#[derive(Default)]
pub struct PendingUpdate(pub Mutex<Option<Update>>);

#[derive(Clone, Debug, Serialize)]
pub struct UpdateInfo {
    pub available: bool,
    pub version: Option<String>,
    pub current_version: Option<String>,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
    // For Microsoft Store URL (Windows)
    pub store_url: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum DownloadEvent {
    Started {
        content_length: Option<u64>,
    },
    Progress {
        chunk_length: usize,
        downloaded: u64,
        content_length: Option<u64>,
    },
    Finished,
    Error {
        message: String,
    },
}

#[cfg(all(desktop, not(windows)))]
#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<UpdateInfo, String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;

    let info = if let Some(up) = update.as_ref() {
        UpdateInfo {
            available: true,
            version: Some(up.version.clone()),
            current_version: Some(up.current_version.clone()),
            notes: up.body.clone(),
            pub_date: up.date.map(|d| d.format(&Rfc3339).unwrap_or_default()),
            store_url: None,
        }
    } else {
        UpdateInfo {
            available: false,
            version: None,
            current_version: None,
            notes: None,
            pub_date: None,
            store_url: None,
        }
    };

    // Store the pending update for later installation
    *pending.0.lock().unwrap() = update;

    Ok(info)
}

#[cfg(windows)]
#[tauri::command]
pub async fn check_update(
    _app: AppHandle,
    _pending: State<'_, PendingUpdate>,
) -> Result<UpdateInfo, String> {
    // Windows: DO NOT support in-app update, open Microsoft Store instead
    return Ok(UpdateInfo {
        available: false,
        version: None,
        current_version: None,
        notes: None,
        pub_date: None,
        store_url: Some(WINDOWS_STORE_URL.to_string()),
    });
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn check_update(
    _app: AppHandle,
    _pending: State<'_, PendingUpdate>,
) -> Result<UpdateInfo, String> {
    // Mobile: Updater not supported
    Ok(UpdateInfo {
        available: false,
        version: None,
        current_version: None,
        notes: None,
        pub_date: None,
        store_url: None,
    })
}

#[cfg(all(desktop, not(windows)))]
#[tauri::command]
pub async fn install_update(
    pending: State<'_, PendingUpdate>,
    on_event: Channel<DownloadEvent>,
) -> Result<(), String> {
    let Some(update) = pending.0.lock().unwrap().take() else {
        let _ = on_event.send(DownloadEvent::Error {
            message: "No pending update. Call check_update first.".to_string(),
        });
        return Ok(());
    };

    let mut downloaded: u64 = 0;
    // NOTE: download_and_install may call the progress callback multiple times.
    // Send Started only once on the first chunk.
    let mut started = false;

    let r = update
        .download_and_install(
            |chunk_length, content_length| {
                if !started {
                    let _ = on_event.send(DownloadEvent::Started { content_length });
                    started = true;
                }
                downloaded += chunk_length as u64;
                let _ = on_event.send(DownloadEvent::Progress {
                    chunk_length,
                    downloaded,
                    content_length,
                });
            },
            || {
                let _ = on_event.send(DownloadEvent::Finished);
            },
        )
        .await;

    if let Err(e) = r {
        let _ = on_event.send(DownloadEvent::Error {
            message: e.to_string(),
        });
        return Err(e.to_string());
    }

    Ok(())
}

#[cfg(windows)]
#[tauri::command]
pub async fn install_update(
    _pending: State<'_, PendingUpdate>,
    _on_event: Channel<DownloadEvent>,
) -> Result<(), String> {
    // Windows: DO NOT support in-app update, open Microsoft Store instead
    return Ok(());
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn install_update(
    pending: State<'_, PendingUpdate>,
    on_event: Channel<DownloadEvent>,
) -> Result<(), String> {
    let _ = on_event.send(DownloadEvent::Error {
        message: "Updater is not available on mobile.".to_string(),
    });
    Ok(())
}
