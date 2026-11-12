//! Tauri host for the tpt-zones visual editor.
//!
//! Commands here are thin wrappers over `zones-core`. Real window management
//! happens in the core service (`zones-win` / `zones-linux`), reached over IPC
//! once Phase 4 wiring lands.

// Prevents an extra console window on Windows release builds.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use serde::Serialize;
use zones_core::config::{Config, SCHEMA_VERSION};
use zones_core::ipc::PROTOCOL_VERSION;

/// Build and protocol information shown in the editor's About panel.
#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    platform: &'static str,
    config_schema: u32,
    protocol: u32,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        config_schema: SCHEMA_VERSION,
        protocol: PROTOCOL_VERSION,
    }
}

/// Returns the default configuration as TOML, for "reset to defaults" and new-file creation.
#[tauri::command]
fn default_config_toml() -> Result<String, String> {
    Config::default().to_toml().map_err(|e| e.to_string())
}

/// Validates a TOML configuration string and returns an error message if it is invalid.
#[tauri::command]
fn validate_config_toml(text: String) -> Result<(), String> {
    Config::from_toml(&text)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn main() {
    zones_core::logging::init("info");

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            app_info,
            default_config_toml,
            validate_config_toml
        ])
        .run(tauri::generate_context!())
        .expect("error while running tpt-zones editor");
}
