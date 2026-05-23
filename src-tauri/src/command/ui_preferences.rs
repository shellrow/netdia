use tauri::State;

use crate::db::{
    DatabaseState, LegacyUiPreferencesMigrationResult, UiPreferences, UiPreferencesPatch,
};

#[tauri::command]
pub async fn get_ui_preferences(db: State<'_, DatabaseState>) -> Result<UiPreferences, String> {
    db.load_ui_preferences().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn patch_ui_preferences(
    db: State<'_, DatabaseState>,
    patch: UiPreferencesPatch,
) -> Result<UiPreferences, String> {
    db.patch_ui_preferences(patch)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn migrate_legacy_ui_preferences(
    db: State<'_, DatabaseState>,
    patch: UiPreferencesPatch,
) -> Result<LegacyUiPreferencesMigrationResult, String> {
    db.migrate_legacy_ui_preferences(patch)
        .await
        .map_err(|e| e.to_string())
}
