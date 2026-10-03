//! Tauri commands called from the island and settings windows.

use std::path::PathBuf;
use std::sync::Arc;

use islet_proto::{now_ms, Behavior};
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
    if std::env::var_os("ISLET_DEBUG").is_some() {
        eprintln!("[islet] decide {id} allow={allow}");
    }
    let behavior = if allow { Behavior::Allow } else { Behavior::Deny };
    let session = lock(&shared.approvals).decide(&id, behavior);
    if let Some(s) = &session {
        lock(&shared.store).approval_finished(s);
    }
    shared.mark_dirty();
    session.is_some()
}

#[tauri::command]
pub async fn set_island_height(app: AppHandle, height: f64) -> Result<(), String> {
    if std::env::var_os("ISLET_DEBUG").is_some() {
        eprintln!("[islet] set_island_height {height}");
    }
    crate::window::set_height(&app, height).map_err(|e| e.to_string())
}

/// Called when the island expands (true) or finishes collapsing (false).
#[tauri::command]
pub async fn set_interactive(app: AppHandle, shared: Shr<'_>, interactive: bool) -> Result<(), String> {
    shared.expanded.store(interactive, std::sync::atomic::Ordering::Relaxed);
    shared.expanded_changed.notify_one();
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
pub fn save_settings(app: AppHandle, shared: Shr, settings: Settings) -> Result<(), String> {
    let mut cur = lock(&shared.settings);
    // Flags owned by the tray / first run are kept as they are.
    let merged = Settings {
        pause_approvals: cur.pause_approvals,
        first_run_done: cur.first_run_done,
        hooks_notice_shown: cur.hooks_notice_shown,
        colors: settings.colors.clone().sanitized(),
        ..settings
    };
    merged.save(&shared.settings_path).map_err(|e| e.to_string())?;
    let show = cur.low_memory && !merged.low_memory;
    *cur = merged;
    drop(cur);
    // Turning low memory mode off shows the island right away.
    if show {
        shared.user_hidden.store(false, std::sync::atomic::Ordering::Relaxed);
        crate::window::ensure(&app);
    }
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
    if cfg!(windows) { "islet-hook.exe" } else { "islet-hook" }
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
        .join("islet")
        .join("bin")
        .join(hook_file_name())
}

/// Replaces the registered hook copy when the bundled one differs (after an
/// app update). Does nothing when hooks were never installed.
pub fn refresh_installed_hook() {
    let Some(src) = hook_source() else { return };
    let target = hook_target();
    match installer::refresh_hook_binary(&src, &target) {
        Ok(true) => log::info!("refreshed {}", target.display()),
        Ok(false) => {}
        Err(e) => log::warn!("refresh {}: {e}", target.display()),
    }
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
        Ok(current) => {
            let (install_diff, error) = match installer::install(&current, &hook) {
                Ok(new) => (installer::diff(&current, &new), None),
                Err(e) => (String::new(), Some(e)),
            };
            InstallerStatus {
                installed: installer::is_installed(&current),
                foreign_statusline: current.get("statusLine").is_some_and(|s| !installer::is_ours(s)),
                install_diff,
                uninstall_diff: installer::diff(&current, &installer::uninstall(&current)),
                error,
                ..base
            }
        }
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
        let src = hook_source().ok_or("islet-hook was not found next to the app")?;
        let data_dir = target.parent().and_then(|p| p.parent()).ok_or("bad data dir")?;
        let installed = installer::install_hook_binary(&src, data_dir).map_err(|e| e.to_string())?;
        installer::install(&current, &installed.to_string_lossy())?
    } else {
        installer::uninstall(&current)
    };
    let backup = installer::write_with_backup(&path, &new, now_ms()).map_err(|e| e.to_string())?;
    lock(&shared.settings).hooks_notice_shown = true;
    Ok(backup.map(|b| b.to_string_lossy().into_owned()).unwrap_or_default())
}
