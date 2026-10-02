//! Platform-specific window tweaks.

use tauri::WebviewWindow;

/// Keeps the island from ever taking focus or showing in Alt-Tab.
#[cfg(windows)]
pub fn prepare_island(win: &WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    let Ok(hwnd) = win.hwnd() else { return };
    let hwnd = hwnd.0 as windows_sys::Win32::Foundation::HWND;
    // SAFETY: hwnd is a live window owned by this process.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize);
    }
}

#[cfg(target_os = "macos")]
mod mac {
    tauri_nspanel::tauri_panel! {
        panel!(IslandPanel {
            config: {
                can_become_key_window: false,
                can_become_main_window: false,
                is_floating_panel: true
            }
        })
    }
}

/// Turns the island into a non-activating NSPanel at status-bar level, on
/// every Space and over full-screen apps. Clicking it never activates
/// islet or steals focus from the terminal. Main thread only.
#[cfg(target_os = "macos")]
pub fn prepare_island(win: &WebviewWindow) {
    use tauri_nspanel::{CollectionBehavior, PanelLevel, StyleMask, WebviewWindowExt};
    let panel = match win.to_panel::<mac::IslandPanel>() {
        Ok(p) => p,
        Err(e) => {
            log::warn!("island panel: {e}");
            return;
        }
    };
    // Status (25) is just above the menu bar (24).
    panel.set_level(PanelLevel::Status.value());
    if let Err(e) = panel.add_style_mask(StyleMask::empty().nonactivating_panel().value()) {
        log::warn!("island panel style: {e:?}");
    }
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .can_join_all_spaces()
            .stationary()
            .full_screen_auxiliary()
            .ignores_cycle()
            .value(),
    );
    panel.set_hides_on_deactivate(false);
}

/// Undoes `prepare_island` before the window is destroyed, so Tauri closes a
/// plain NSWindow and the panel registry drops its handle. Main thread only.
#[cfg(target_os = "macos")]
pub fn release_island(app: &tauri::AppHandle, label: &str) {
    use tauri_nspanel::ManagerExt;
    if let Ok(panel) = app.get_webview_panel(label) {
        panel.to_window();
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn prepare_island(_win: &WebviewWindow) {}

#[cfg(not(target_os = "macos"))]
pub fn release_island(_app: &tauri::AppHandle, _label: &str) {}

/// Distance from the top of the screen in physical pixels.
#[cfg(not(target_os = "macos"))]
pub fn top_offset(_scale: f64) -> i32 {
    0
}

/// Distance from the top of the screen in physical pixels: just below the
/// menu bar, or below the notch when the menu bar is hidden. On notched
/// MacBooks the menu bar is as tall as the notch, so the pill hangs directly
/// under it.
#[cfg(target_os = "macos")]
pub fn top_offset(scale: f64) -> i32 {
    (menu_bar_inset().unwrap_or(FALLBACK_INSET) * scale).round() as i32
}

/// Menu bar height on non-notched displays (points), used off the main thread.
#[cfg(target_os = "macos")]
const FALLBACK_INSET: f64 = 25.0;

/// Top inset of the primary (menu bar) screen in points.
#[cfg(target_os = "macos")]
fn menu_bar_inset() -> Option<f64> {
    use objc2::runtime::NSObjectProtocol;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;
    let mtm = MainThreadMarker::new()?;
    let screen = NSScreen::screens(mtm).firstObject()?;
    let (frame, visible) = (screen.frame(), screen.visibleFrame());
    let menu_bar = (frame.origin.y + frame.size.height) - (visible.origin.y + visible.size.height);
    // safeAreaInsets is macOS 12+; older systems have no notch anyway.
    let notch = if screen.respondsToSelector(objc2::sel!(safeAreaInsets)) {
        screen.safeAreaInsets().top
    } else {
        0.0
    };
    Some(menu_bar.max(notch).max(0.0))
}
