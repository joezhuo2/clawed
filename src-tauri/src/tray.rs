//! Tray icon and menu. The menu is rebuilt only when its text changes.

use std::sync::{Arc, Mutex};

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_autostart::ManagerExt;

use crate::state::{lock, Shared, Snapshot};
use crate::store::SessionState;
use crate::usage::Ring;

const TRAY_ID: &str = "main";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Variant {
    Idle,
    Working,
    Approval,
}

/// Last menu signature, to skip rebuilding an identical menu.
static LAST: Mutex<Option<(String, Variant)>> = Mutex::new(None);

pub fn build(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let shared = app.state::<Arc<Shared>>().inner().clone();
    let menu = menu(app, &shared.snapshot(), &shared)?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon(Variant::Idle))
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("clawed")
        .menu(&menu)
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(on_menu)
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                crate::window::toggle(tray.app_handle());
            }
        })
        .build(app)
}

pub fn variant(snap: &Snapshot) -> Variant {
    if !snap.approvals.is_empty() {
        Variant::Approval
    } else if snap.sessions.iter().any(|s| s.state == SessionState::Working) {
        Variant::Working
    } else {
        Variant::Idle
    }
}

fn ring_text(label: &str, r: Option<&Ring>, source: &str, now_s: u64) -> String {
    let Some(r) = r else { return format!("{label}: --") };
    let mut s = format!("{label}: {:.0}%", r.pct);
    if let Some(reset) = r.resets_at.filter(|t| *t > now_s) {
        s += &format!(" (resets in {})", crate::format::duration_short(reset - now_s));
    }
    if source == "estimated" {
        s += " est.";
    }
    s
}

fn signature(snap: &Snapshot, shared: &Shared, autostart: bool) -> String {
    let now_s = snap.now / 1000;
    let mut sig = format!(
        "{}|{}|{}|{}|{}",
        ring_text("5h", snap.usage.five_hour.as_ref(), snap.usage.source, now_s),
        ring_text("7d", snap.usage.seven_day.as_ref(), snap.usage.source, now_s),
        snap.paused,
        autostart,
        lock(&shared.settings).low_memory,
    );
    for s in &snap.sessions {
        sig += &format!("|{}:{}:{}", s.id, s.repo, s.state.label());
    }
    sig
}

fn menu(app: &AppHandle, snap: &Snapshot, shared: &Shared) -> tauri::Result<Menu<Wry>> {
    let now_s = snap.now / 1000;
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);
    let low_memory = lock(&shared.settings).low_memory;
    let text = |id: &str, t: String| MenuItem::with_id(app, id, t, false, None::<&str>);
    let item = |id: &str, t: &str| MenuItem::with_id(app, id, t, true, None::<&str>);
    let sep = || PredefinedMenuItem::separator(app);

    let menu = Menu::new(app)?;
    menu.append(&item("toggle_island", "Show / hide island")?)?;
    menu.append(&sep()?)?;
    menu.append(&text("u5", ring_text("5h", snap.usage.five_hour.as_ref(), snap.usage.source, now_s))?)?;
    menu.append(&text("u7", ring_text("7d", snap.usage.seven_day.as_ref(), snap.usage.source, now_s))?)?;
    menu.append(&sep()?)?;
    if snap.sessions.is_empty() {
        menu.append(&text("none", "No active sessions".into())?)?;
    }
    for s in &snap.sessions {
        let label = format!("{} — {}", if s.repo.is_empty() { "session" } else { &s.repo }, s.state.label());
        menu.append(&item(&format!("session:{}", s.id), &label)?)?;
    }
    menu.append(&sep()?)?;
    menu.append(&CheckMenuItem::with_id(app, "pause", "Pause approvals", true, snap.paused, None::<&str>)?)?;
    menu.append(&CheckMenuItem::with_id(app, "autostart", "Launch at login", true, autostart, None::<&str>)?)?;
    menu.append(&CheckMenuItem::with_id(app, "low_memory", "Low memory mode", true, low_memory, None::<&str>)?)?;
    menu.append(&sep()?)?;
    menu.append(&item("hooks", "Install / uninstall hooks…")?)?;
    menu.append(&item("settings", "Settings…")?)?;
    menu.append(&sep()?)?;
    menu.append(&item("quit", "Quit clawed")?)?;
    Ok(menu)
}

/// Updates icon and menu from a snapshot, only when something changed.
pub fn refresh(app: &AppHandle, shared: &Shared, snap: &Snapshot) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);
    let sig = signature(snap, shared, autostart);
    let v = variant(snap);
    let mut last = lock(&LAST);
    let (menu_changed, icon_changed) = match last.as_ref() {
        Some((s, lv)) => (*s != sig, *lv != v),
        None => (true, true),
    };
    if icon_changed {
        let _ = tray.set_icon(Some(icon(v)));
        let _ = tray.set_icon_as_template(cfg!(target_os = "macos"));
        let tip = match v {
            Variant::Idle => "clawed",
            Variant::Working => "clawed — working",
            Variant::Approval => "clawed — approval needed",
        };
        let _ = tray.set_tooltip(Some(tip));
    }
    if menu_changed {
        if let Ok(m) = menu(app, snap, shared) {
            let _ = tray.set_menu(Some(m));
        }
    }
    *last = Some((sig, v));
}

fn on_menu(app: &AppHandle, ev: MenuEvent) {
    let shared = app.state::<Arc<Shared>>().inner().clone();
    let id = ev.id().as_ref();
    match id {
        "toggle_island" => crate::window::toggle(app),
        "pause" => {
            let paused = {
                let mut a = lock(&shared.approvals);
                a.paused = !a.paused;
                if a.paused {
                    let sessions = a.release_all();
                    drop(a);
                    let mut store = lock(&shared.store);
                    for s in sessions {
                        store.approval_finished(&s);
                    }
                    true
                } else {
                    false
                }
            };
            let mut s = lock(&shared.settings);
            s.pause_approvals = paused;
            let _ = s.save(&shared.settings_path);
        }
        "autostart" => {
            let al = app.autolaunch();
            let _ = if al.is_enabled().unwrap_or(false) { al.disable() } else { al.enable() };
        }
        "low_memory" => {
            let mut s = lock(&shared.settings);
            s.low_memory = !s.low_memory;
            let _ = s.save(&shared.settings_path);
            shared.touch_active();
        }
        "hooks" | "settings" => crate::window::open_settings(app),
        "quit" => {
            let sessions = lock(&shared.approvals).release_all();
            drop(sessions);
            app.exit(0);
        }
        other => {
            if other.starts_with("session:") {
                crate::window::ensure(app);
            }
        }
    }
    shared.mark_dirty();
}

/// 32x32 capsule glyph. macOS: black template image; elsewhere colored.
pub fn icon(v: Variant) -> Image<'static> {
    const S: usize = 32;
    let template = cfg!(target_os = "macos");
    let (fill, rgb): (bool, [u8; 3]) = match (v, template) {
        (Variant::Idle, true) => (false, [0, 0, 0]),
        (_, true) => (true, [0, 0, 0]),
        (Variant::Idle, false) => (false, [230, 230, 230]),
        (Variant::Working, false) => (true, [76, 141, 255]),
        (Variant::Approval, false) => (true, [169, 112, 255]),
    };
    // Capsule centered, 28x14, stroke 2.5 when outlined.
    let (cx, cy, half_w, r) = (16.0f64, 16.0f64, 14.0f64, 7.0f64);
    let dist = |x: f64, y: f64| {
        let dx = ((x - cx).abs() - (half_w - r)).max(0.0);
        let dy = y - cy;
        (dx * dx + dy * dy).sqrt() - r
    };
    let mut rgba = vec![0u8; S * S * 4];
    for py in 0..S {
        for px in 0..S {
            let mut cov = 0.0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let (x, y) = (px as f64 + (sx as f64 + 0.5) / 4.0, py as f64 + (sy as f64 + 0.5) / 4.0);
                    let d = dist(x, y);
                    let mut hit = if fill { d <= 0.0 } else { d <= 0.0 && d >= -2.5 };
                    // Approval on macOS: punch a dot so it differs from Working.
                    if template && v == Variant::Approval {
                        let (ddx, ddy) = (x - 22.0, y - cy);
                        if ddx * ddx + ddy * ddy <= 9.0 {
                            hit = false;
                        }
                    }
                    if hit {
                        cov += 1.0 / 16.0;
                    }
                }
            }
            let i = (py * S + px) * 4;
            rgba[i..i + 3].copy_from_slice(&rgb);
            rgba[i + 3] = (cov * 255.0f64).round() as u8;
        }
    }
    Image::new_owned(rgba, S as u32, S as u32)
}
