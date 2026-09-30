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
pub fn prepare_island(_win: &WebviewWindow) {
    // Activation policy is Accessory (set at startup) and the window is
    // created non-focusable. Sitting above the menu bar around the notch
    // needs an NSPanel (tauri-nspanel); not done in v1.
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn prepare_island(_win: &WebviewWindow) {}

/// Distance from the top of the screen in physical pixels.
pub fn top_offset(scale: f64) -> i32 {
    if cfg!(target_os = "macos") {
        // Below the menu bar.
        (26.0 * scale) as i32
    } else {
        0
    }
}
