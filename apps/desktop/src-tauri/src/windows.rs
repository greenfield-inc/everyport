//! Windows-only window setup.

use tauri::WebviewWindow;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
use windows::core::Interface;
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};

/// Asks Windows 11 for rounded corners. Windows 10 ignores it.
pub fn round_corners(window: &WebviewWindow) {
    let Ok(hwnd) = window.hwnd() else { return };
    let preference = DWMWCP_ROUND;
    // SAFETY: `hwnd` is this window's live handle, and the attribute takes a
    // DWM_WINDOW_CORNER_PREFERENCE of the size passed.
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const _ as *const _,
            std::mem::size_of_val(&preference) as u32,
        )
    };
}

/// Turns off WebView2's browser shortcuts (reload, find, print) and its
/// default context menu, which Tauri doesn't expose.
pub fn disable_browser_keys(window: &WebviewWindow) {
    let _ = window.with_webview(|webview| {
        // SAFETY: COM calls on the live controller, on the thread Tauri runs this closure on.
        unsafe {
            let Ok(core) = webview.controller().CoreWebView2() else {
                return;
            };
            let Ok(settings) = core.Settings() else {
                return;
            };
            let _ = settings.SetAreDefaultContextMenusEnabled(false);
            if let Ok(settings) = settings.cast::<ICoreWebView2Settings3>() {
                let _ = settings.SetAreBrowserAcceleratorKeysEnabled(false);
            }
        }
    });
}
