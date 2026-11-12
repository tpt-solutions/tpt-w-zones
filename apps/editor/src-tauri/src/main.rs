//! Tauri host for the tpt-zones visual editor.
//!
//! Commands are thin wrappers over `zones-core` (config I/O and validation) and, on
//! Windows, `zones-win` (monitor list for the live preview). The editor never changes
//! windows itself; the running service picks up saved configs via `ctl reload`.

// Prevents an extra console window on Windows release builds.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::path::PathBuf;

use serde::Serialize;
use zones_core::config::{self, Config, SCHEMA_VERSION};
use zones_core::geometry::Monitor;
use zones_core::ipc::PROTOCOL_VERSION;

/// Build and protocol information shown in the editor's About panel.
#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    platform: &'static str,
    config_schema: u32,
    protocol: u32,
    config_path: Option<String>,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        config_schema: SCHEMA_VERSION,
        protocol: PROTOCOL_VERSION,
        config_path: config::default_path().map(|p| p.display().to_string()),
    }
}

/// Loads the user's configuration. A missing file yields the default configuration, so a
/// first run starts from a clean slate instead of failing.
#[tauri::command]
fn load_config() -> Result<Config, String> {
    let path = user_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    Config::load(&path).map_err(|e| e.to_string())
}

/// Validates and saves the configuration atomically to the user's config file.
#[tauri::command]
fn save_config(config: Config) -> Result<(), String> {
    let path = user_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    config.save(&path).map_err(|e| e.to_string())
}

/// Validates a configuration without saving it, so the editor can show problems live.
#[tauri::command]
fn validate_config(config: Config) -> Result<(), String> {
    config.validate().map_err(|e| e.to_string())
}

/// Lists monitors for the live preview.
#[cfg(windows)]
#[tauri::command]
fn list_monitors() -> Result<Vec<Monitor>, String> {
    zones_win::display::enumerate().map_err(|e| e.to_string())
}

/// Monitor enumeration is only implemented on Windows for now.
#[cfg(not(windows))]
#[tauri::command]
fn list_monitors() -> Result<Vec<Monitor>, String> {
    Err("monitor detection is not available on this platform yet".into())
}

fn user_path() -> Result<PathBuf, String> {
    config::default_path().ok_or_else(|| "cannot locate the user config directory".to_string())
}

fn main() {
    zones_core::logging::init("info");

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            app_info,
            load_config,
            save_config,
            validate_config,
            list_monitors
        ])
        .run(tauri::generate_context!())
        .expect("error while running tpt-zones editor");
}
