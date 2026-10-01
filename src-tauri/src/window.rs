//! The island window: creation, teardown, sizing, click-through and hover.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const LABEL: &str = "island";
pub const SETTINGS_LABEL: &str = "settings";

use crate::state::Shared;

/// Window width in logical pixels. Fixed, so resizing never moves the window
/// horizontally (a width change plus re-centering flashed for a frame).
pub const WIDTH: f64 = 420.0;
/// Collapsed window height; the UI draws the pill inside it.
pub const COLLAPSED_H: f64 = 48.0;
/// Pill geometry, must match theme.css (--pill-w, --pill-h, --top-gap).
const PILL_W: f64 = 260.0;
const PILL_H: f64 = 34.0;
const TOP_GAP: f64 = 6.0;
const HOVER_POLL: Duration = Duration::from_millis(80);

static CREATING: AtomicBool = AtomicBool::new(false);

/// WebView2 flags. Tauri's defaults are kept; `CLAWED_WEBVIEW_ARGS` replaces
/// the whole string (for memory experiments). All webviews must share them.
#[cfg(windows)]
fn browser_args() -> String {
    std::env::var("CLAWED_WEBVIEW_ARGS").unwrap_or_else(|_| DEFAULT_BROWSER_ARGS.to_string())
}

#[cfg(windows)]
// Software compositing: the GPU process was ~120 MB private for a small,
// mostly static island. Measured in docs/memory.md.
const DEFAULT_BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-gpu";

pub fn get(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

/// Creates the island if it does not exist. Cheap when it does.
pub fn ensure(app: &AppHandle) {
    if get(app).is_some() || CREATING.swap(true, Ordering::AcqRel) {
        return;
    }
    // macOS: the NSPanel conversion and NSScreen lookups need the main thread.
    #[cfg(target_os = "macos")]
    {
        let handle = app.clone();
        if let Err(e) = app.run_on_main_thread(move || finish_create(&handle)) {
            CREATING.store(false, Ordering::Release);
            log::error!("island window: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    finish_create(app);
}

fn finish_create(app: &AppHandle) {
    let result = create(app);
    CREATING.store(false, Ordering::Release);
    if let Err(e) = result {
        log::error!("island window: {e}");
    }
}

fn create(app: &AppHandle) -> tauri::Result<()> {
    let (w, h) = (WIDTH, COLLAPSED_H);
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()));
    #[cfg(windows)]
    let builder = builder.additional_browser_args(&browser_args());
    let win = builder
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
    place(&win)?;
    win.set_ignore_cursor_events(true)?;
    win.show()?;
    spawn_hover_poll(app.clone());
    Ok(())
}

pub fn destroy(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || finish_destroy(&handle));
    }
    #[cfg(not(target_os = "macos"))]
    finish_destroy(app);
}

fn finish_destroy(app: &AppHandle) {
    if let Some(win) = get(app) {
        crate::platform::release_island(app, LABEL);
        let _ = win.destroy();
    }
}

pub fn toggle(app: &AppHandle) {
    match get(app) {
        Some(_) => destroy(app),
        None => ensure(app),
    }
}

/// Sets the window height (logical px). The top edge and width never change,
/// so the island does not move while the window grows or shrinks.
pub fn set_height(app: &AppHandle, h: f64) -> tauri::Result<()> {
    let Some(win) = get(app) else { return Ok(()) };
    win.set_size(LogicalSize::new(WIDTH, h.clamp(COLLAPSED_H, 900.0)))
}

/// Pins the window top-center on the primary monitor.
fn place(win: &WebviewWindow) -> tauri::Result<()> {
    let w = WIDTH;
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
            let (Ok(cur), Ok(pos), Ok(size), Ok(scale)) =
                (app.cursor_position(), win.outer_position(), win.outer_size(), win.scale_factor())
            else {
                continue;
            };
            let expanded = app.state::<Arc<Shared>>().expanded.load(Ordering::Relaxed);
            let (top, center) = (pos.y as f64, pos.x as f64 + size.width as f64 / 2.0);
            // Collapsed: only the pill counts, not the transparent margins.
            let (half_w, bottom) = if expanded {
                (size.width as f64 / 2.0, top + size.height as f64)
            } else {
                ((PILL_W / 2.0 + 4.0) * scale, top + (TOP_GAP + PILL_H + 4.0) * scale)
            };
            let now = (cur.x - center).abs() < half_w && cur.y >= top - 2.0 && cur.y < bottom;
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
    let builder = WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App("settings.html".into()));
    #[cfg(windows)]
    let builder = builder.additional_browser_args(&browser_args());
    let built = builder
        .title("clawed settings")
        .inner_size(620.0, 720.0)
        .min_inner_size(480.0, 480.0)
        .center()
        .build();
    if let Err(e) = built {
        log::error!("settings window: {e}");
    }
}
