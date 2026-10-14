use serde::Serialize;
use specta::Type;
use tauri::State;

use crate::error::AppResult;
use crate::settings::{self, AppSettings, SettingsPatch};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub os: String,
    pub arch: String,
}

impl AppInfo {
    pub fn current(name: &str, version: &str) -> Self {
        Self {
            name: name.to_owned(),
            version: version.to_owned(),
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn app_info(app: tauri::AppHandle) -> AppInfo {
    let pkg = app.package_info();
    AppInfo::current(&pkg.name, &pkg.version.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn get_settings(state: State<'_, AppState>) -> AppResult<AppSettings> {
    settings::load(&state.storage)
}

#[tauri::command]
#[specta::specta]
pub fn update_settings(state: State<'_, AppState>, patch: SettingsPatch) -> AppResult<AppSettings> {
    tracing::debug!(?patch, "update_settings");
    settings::apply(&state.storage, patch)
}
