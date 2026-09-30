//! Tauri commands called from the island and settings windows.

use std::path::PathBuf;
use std::sync::Arc;

use clawed_proto::{now_ms, Behavior};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::installer;
use crate::settings::Settings;
use crate::state::{lock, Shared, Snapshot};

type Shr<'a> = State<'a, Arc<Shared>>;

#[tauri::command]
pub fn get_state(shared: Shr) -> Snapshot {
    shared.snapshot()
}

#[tauri::command]
pub fn decide(shared: Shr, id: String, allow: bool) -> bool {
    let behavior = if allow { Behavior::Allow } else { Behavior::Deny };
    let session = lock(&shared.approvals).decide(&id, behavior);
    if let Some(s) = &session {
        lock(&shared.store).approval_finished(s);
    }
    shared.mark_dirty();
    session.is_some()
}

#[tauri::command]
pub async fn set_island_size(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    crate::window::set_size(&app, width, height).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_interactive(app: AppHandle, interactive: bool) -> Result<(), String> {
    crate::window::set_interactive(&app, interactive).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_settings(app: AppHandle) {
    crate::window::open_settings(&app);
}

#[tauri::command]
pub fn get_settings(shared: Shr) -> Settings {
    lock(&shared.settings).clone()
}

#[tauri::command]
pub fn save_settings(shared: Shr, settings: Settings) -> Result<(), String> {
    let mut cur = lock(&shared.settings);
    // Flags owned by the tray / first run are kept as they are.
    let merged = Settings {
        pause_approvals: cur.pause_approvals,
        first_run_done: cur.first_run_done,
        hooks_notice_shown: cur.hooks_notice_shown,
        ..settings
    };
    merged.save(&shared.settings_path).map_err(|e| e.to_string())?;
    *cur = merged;
    drop(cur);
    shared.mark_dirty();
    Ok(())
}

#[derive(Serialize)]
pub struct InstallerStatus {
    pub settings_path: String,
    pub hook_path: String,
    pub hook_source_found: bool,
    pub installed: bool,
    pub foreign_statusline: bool,
    pub install_diff: String,
    pub uninstall_diff: String,
    pub error: Option<String>,
}

fn hook_file_name() -> &'static str {
    if cfg!(windows) { "clawed-hook.exe" } else { "clawed-hook" }
}

/// The hook shipped next to the app executable.
fn hook_source() -> Option<PathBuf> {
    let p = std::env::current_exe().ok()?.parent()?.join(hook_file_name());
    p.exists().then_some(p)
}

/// Stable per-user location the hook is registered from.
fn hook_target() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("clawed")
        .join("bin")
        .join(hook_file_name())
}

#[tauri::command]
pub fn installer_status() -> InstallerStatus {
    let path = installer::claude_settings_path();
    let hook = hook_target().to_string_lossy().into_owned();
    let base = InstallerStatus {
        settings_path: path.to_string_lossy().into_owned(),
        hook_path: hook.clone(),
        hook_source_found: hook_source().is_some(),
        installed: false,
        foreign_statusline: false,
        install_diff: String::new(),
        uninstall_diff: String::new(),
        error: None,
    };
    match installer::read_settings(&path) {
        Ok(current) => InstallerStatus {
            installed: installer::is_installed(&current),
            foreign_statusline: current.get("statusLine").is_some_and(|s| !installer::is_ours(s)),
            install_diff: installer::diff(&current, &installer::install(&current, &hook)),
            uninstall_diff: installer::diff(&current, &installer::uninstall(&current)),
            ..base
        },
        Err(e) => InstallerStatus { error: Some(format!("cannot read {}: {e}", path.display())), ..base },
    }
}

/// Applies install (true) or uninstall (false). Returns the backup path.
#[tauri::command]
pub fn installer_apply(shared: Shr, install: bool) -> Result<String, String> {
    let path = installer::claude_settings_path();
    let current = installer::read_settings(&path).map_err(|e| e.to_string())?;
    let target = hook_target();
    let new = if install {
        let src = hook_source().ok_or("clawed-hook was not found next to the app")?;
        let data_dir = target.parent().and_then(|p| p.parent()).ok_or("bad data dir")?;
        let installed = installer::install_hook_binary(&src, data_dir).map_err(|e| e.to_string())?;
        installer::install(&current, &installed.to_string_lossy())
    } else {
        installer::uninstall(&current)
    };
    let backup = installer::write_with_backup(&path, &new, now_ms()).map_err(|e| e.to_string())?;
    lock(&shared.settings).hooks_notice_shown = true;
    Ok(backup.map(|b| b.to_string_lossy().into_owned()).unwrap_or_default())
}
