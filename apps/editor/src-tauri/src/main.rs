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
    let config = Config::load(&path).map_err(|e| e.to_string())?;
    zones_core::crash::set_enabled(config.general.crash_reports);
    Ok(config)
}

/// Validates and saves the configuration atomically to the user's config file.
#[tauri::command]
fn save_config(config: Config) -> Result<(), String> {
    let path = user_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    config.save(&path).map_err(|e| e.to_string())?;
    zones_core::crash::set_enabled(config.general.crash_reports);
    Ok(())
}

/// Validates a configuration without saving it, so the editor can show problems live.
#[tauri::command]
fn validate_config(config: Config) -> Result<(), String> {
    config.validate().map_err(|e| e.to_string())
}

/// Where local crash reports are written, shown in Settings so they can be found and attached.
#[tauri::command]
fn crash_report_dir() -> Option<String> {
    zones_core::crash::crash_dir().map(|p| p.display().to_string())
}

/// Lists monitors for the live preview.
#[cfg(windows)]
#[tauri::command]
fn list_monitors() -> Result<Vec<Monitor>, String> {
    zones_win::display::enumerate().map_err(|e| e.to_string())
}

/// Draws the zones of `profile` (or the active profile) on every monitor for `seconds`.
/// Uses the configuration passed in, so unsaved edits can be previewed.
#[cfg(windows)]
#[tauri::command]
fn show_preview(config: Config, profile: Option<String>, seconds: u32) -> Result<(), String> {
    let monitors = zones_win::display::enumerate().map_err(|e| e.to_string())?;
    let name = profile.as_deref().or(config.active_profile.as_deref());
    let plan = zones_core::engine::preview_plan(&config, &monitors, name);
    let duration = std::time::Duration::from_secs(u64::from(seconds.clamp(1, 30)));
    zones_win::overlay::show(plan, duration).map_err(|e| e.to_string())
}

/// Preview overlays are only implemented on Windows for now.
#[cfg(not(windows))]
#[tauri::command]
fn show_preview(_config: Config, _profile: Option<String>, _seconds: u32) -> Result<(), String> {
    Err("previews are not available on this platform yet".into())
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
    zones_core::crash::install("editor");

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            app_info,
            load_config,
            save_config,
            validate_config,
            list_monitors,
            show_preview,
            crash_report_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running tpt-zones editor");
}
