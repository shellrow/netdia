use tauri::State;

use crate::db::{AppNotification, DatabaseState, UpdateNotificationPayload};

#[tauri::command]
pub async fn list_notifications(
    db: State<'_, DatabaseState>,
) -> Result<Vec<AppNotification>, String> {
    db.list_notifications().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mark_all_notifications_read(
    db: State<'_, DatabaseState>,
) -> Result<Vec<AppNotification>, String> {
    db.mark_all_notifications_read()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn dismiss_notification(
    db: State<'_, DatabaseState>,
    id: i64,
) -> Result<Vec<AppNotification>, String> {
    db.dismiss_notification(id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn upsert_update_notification(
    db: State<'_, DatabaseState>,
    payload: UpdateNotificationPayload,
) -> Result<Vec<AppNotification>, String> {
    db.upsert_update_notification(payload)
        .await
        .map_err(|e| e.to_string())
}
