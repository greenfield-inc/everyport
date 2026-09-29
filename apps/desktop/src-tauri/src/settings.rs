//! The Settings window: a normal window, created on first open and hidden
//! instead of closed.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

use crate::tray;

pub const LABEL: &str = "settings";

pub fn open(app: &AppHandle) {
    let window = match app.get_webview_window(LABEL) {
        Some(window) => window,
        None => match WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
            .title("Port Process Manager Settings")
            .inner_size(480.0, 320.0)
            .resizable(false)
            .center()
            .build()
        {
            Ok(window) => {
                let hide = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = hide.hide();
                    }
                });
                window
            }
            Err(error) => return eprintln!("settings: {error}"),
        },
    };
    // An accessory app has to activate itself to bring a window forward.
    #[cfg(target_os = "macos")]
    let _ = app.show();
    let _ = window.show();
    let _ = window.set_focus();
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    open(&app);
}

#[tauri::command]
pub fn launch_at_login(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_launch_at_login(app: AppHandle, enabled: bool) -> Result<(), String> {
    let autolaunch = app.autolaunch();
    let result = if enabled {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    tray::sync_launch_at_login(&app);
    result.map_err(|e| e.to_string())
}
