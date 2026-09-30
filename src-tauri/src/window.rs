//! The island window: creation, teardown, sizing, click-through and hover.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const LABEL: &str = "island";
pub const SETTINGS_LABEL: &str = "settings";

/// Collapsed window size in logical pixels; the UI draws the pill inside it.
pub const COLLAPSED: (f64, f64) = (260.0, 48.0);
const HOVER_POLL: Duration = Duration::from_millis(80);

static CREATING: AtomicBool = AtomicBool::new(false);

pub fn get(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

/// Creates the island if it does not exist. Cheap when it does.
pub fn ensure(app: &AppHandle) {
    if get(app).is_some() || CREATING.swap(true, Ordering::AcqRel) {
        return;
    }
    let result = create(app);
    CREATING.store(false, Ordering::Release);
    if let Err(e) = result {
        log::error!("island window: {e}");
    }
}

fn create(app: &AppHandle) -> tauri::Result<()> {
    let (w, h) = COLLAPSED;
    let win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("clawed")
        .inner_size(w, h)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .focused(false)
        .focusable(false)
        .visible(false)
        .build()?;
    crate::platform::prepare_island(&win);
    place(&win, w, h)?;
    win.set_ignore_cursor_events(true)?;
    win.show()?;
    spawn_hover_poll(app.clone());
    Ok(())
}

pub fn destroy(app: &AppHandle) {
    if let Some(win) = get(app) {
        let _ = win.destroy();
    }
}

pub fn toggle(app: &AppHandle) {
    match get(app) {
        Some(_) => destroy(app),
        None => ensure(app),
    }
}

/// Sizes the window (logical px) and keeps it pinned top-center.
pub fn set_size(app: &AppHandle, w: f64, h: f64) -> tauri::Result<()> {
    let Some(win) = get(app) else { return Ok(()) };
    let (w, h) = (w.clamp(80.0, 800.0), h.clamp(24.0, 900.0));
    win.set_size(LogicalSize::new(w, h))?;
    place(&win, w, h)
}

fn place(win: &WebviewWindow, w: f64, _h: f64) -> tauri::Result<()> {
    let Some(mon) = win.primary_monitor()?.or(win.current_monitor()?) else { return Ok(()) };
    let scale = mon.scale_factor();
    let pos = mon.position();
    let size = mon.size();
    let x = pos.x as f64 + (size.width as f64 - w * scale) / 2.0;
    win.set_position(PhysicalPosition::new(x.round() as i32, pos.y + crate::platform::top_offset(scale)))
}

pub fn set_interactive(app: &AppHandle, interactive: bool) -> tauri::Result<()> {
    match get(app) {
        Some(win) => win.set_ignore_cursor_events(!interactive),
        None => Ok(()),
    }
}

/// While the island exists, polls the cursor and emits `hover` on changes.
/// The collapsed window ignores the mouse, so it cannot see hover itself.
fn spawn_hover_poll(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut inside = false;
        loop {
            tokio::time::sleep(HOVER_POLL).await;
            let Some(win) = get(&app) else { return };
            let (Ok(cur), Ok(pos), Ok(size)) = (app.cursor_position(), win.outer_position(), win.outer_size())
            else {
                continue;
            };
            let now = cur.x >= pos.x as f64
                && cur.x < (pos.x + size.width as i32) as f64
                && cur.y >= pos.y as f64 - 2.0
                && cur.y < (pos.y + size.height as i32) as f64;
            if now != inside {
                inside = now;
                let _ = win.emit_to(LABEL, "hover", inside);
            }
        }
    });
}

pub fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(SETTINGS_LABEL) {
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App("settings.html".into()))
        .title("clawed settings")
        .inner_size(620.0, 720.0)
        .min_inner_size(480.0, 480.0)
        .center()
        .build();
    if let Err(e) = built {
        log::error!("settings window: {e}");
    }
}
