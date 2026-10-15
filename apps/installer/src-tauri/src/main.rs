// No console window next to the setup in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// The setup only ships for Windows; on other systems (CI builds the whole workspace) the
// registry and WebView2 parts are compiled out and would otherwise trip `-D warnings`.
#![cfg_attr(not(windows), allow(dead_code))]

mod install;

use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

const PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.exe"));
const LICENSE: &str = include_str!("../../../../LICENSE");
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupInfo {
    version: &'static str,
    default_dir: String,
    existing: Option<install::Existing>,
    has_payload: bool,
    app_running: bool,
    license: &'static str,
}

#[tauri::command]
fn setup_info() -> SetupInfo {
    let existing = install::existing(VERSION);
    let default_dir = existing
        .as_ref()
        .map(|e| e.dir.clone())
        .unwrap_or_else(|| install::default_dir().display().to_string());
    SetupInfo {
        version: VERSION,
        default_dir,
        existing,
        has_payload: !PAYLOAD.is_empty(),
        app_running: install::app_running(),
        license: LICENSE,
    }
}

#[tauri::command]
fn app_running() -> bool {
    install::app_running()
}

#[tauri::command]
fn check_folder(dir: String) -> Result<(), String> {
    install::check_folder(Path::new(dir.trim()))
}

#[tauri::command]
async fn pick_folder(app: AppHandle, current: String) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let start = Path::new(current.trim())
        .ancestors()
        .find(|p| p.is_dir())
        .map(Path::to_path_buf);
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app.dialog().file();
        if let Some(start) = start {
            dialog = dialog.set_directory(start);
        }
        let picked = dialog.blocking_pick_folder()?.into_path().ok()?;
        Some(install::install_dir_for_pick(&picked).display().to_string())
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
async fn install(app: AppHandle, dir: String, desktop_shortcut: bool) -> Result<(), String> {
    let desktop = app.path().desktop_dir().ok();
    let dir = PathBuf::from(dir.trim());
    tauri::async_runtime::spawn_blocking(move || {
        install::install(
            PAYLOAD,
            &dir,
            desktop.as_deref(),
            desktop_shortcut,
            |stage| {
                let _ = app.emit("setup-stage", stage);
            },
        )
    })
    .await
    .map_err(|_| "launch".to_owned())?
}

#[tauri::command]
fn launch_and_quit(app: AppHandle, dir: String) -> Result<(), String> {
    install::launch(Path::new(dir.trim()))?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

fn main() {
    #[cfg(windows)]
    if tauri::webview_version().is_err() && !PAYLOAD.is_empty() {
        std::process::exit(install::run_classic(PAYLOAD));
    }

    // Keep the setup's WebView2 profile out of %LOCALAPPDATA%. WebView2 often still holds it
    // when the setup exits, so earlier runs' profiles are removed here too.
    const WEBVIEW_PREFIX: &str = "nanomgg-setup-webview-";
    let temp = std::env::temp_dir();
    if let Ok(entries) = std::fs::read_dir(&temp) {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(WEBVIEW_PREFIX)
            {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let webview_data = temp.join(format!("{WEBVIEW_PREFIX}{}", std::process::id()));
    let window_data = webview_data.clone();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            setup_info,
            app_running,
            check_folder,
            pick_folder,
            install,
            launch_and_quit,
            quit
        ])
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("NANOMultipleGitGUI Setup")
                .inner_size(880.0, 580.0)
                .resizable(false)
                .maximizable(false)
                .decorations(false)
                .center()
                .data_directory(window_data.clone())
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to start the setup");
    app.run(move |_, event| {
        if let RunEvent::Exit = event {
            let _ = std::fs::remove_dir_all(&webview_data);
        }
    });
}
