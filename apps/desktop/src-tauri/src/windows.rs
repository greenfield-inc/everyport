//! Windows-only window and tray setup.

use tauri::WebviewWindow;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
use windows::core::{w, Interface, BOOL};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SystemParametersInfoW, SM_CXSMICON, SPI_GETCLIENTAREAANIMATION,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

/// The notification area's icon size in pixels: 16 at 100% scaling, 32 at
/// 200%. It follows the scale of the taskbar's monitor, which can differ from
/// the system's when monitors have different scales.
pub fn tray_icon_size() -> usize {
    // SAFETY: plain queries on a window handle Windows just returned, or none.
    let size = unsafe {
        let dpi = FindWindowW(w!("Shell_TrayWnd"), None)
            .map(|taskbar| GetDpiForWindow(taskbar))
            .ok()
            .filter(|&dpi| dpi != 0)
            .unwrap_or_else(|| GetDpiForSystem());
        GetSystemMetricsForDpi(SM_CXSMICON, dpi)
    };
    size.max(16) as usize
}

/// Whether "Animation effects" is on, the switch Windows' reduce-motion
/// setting turns off.
pub fn animations_enabled() -> bool {
    let mut enabled = BOOL(1);
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL to the pointer passed.
    let _ = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut enabled as *mut BOOL as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    enabled.as_bool()
}

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
