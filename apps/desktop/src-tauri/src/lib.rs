//! Port Process Manager's desktop app: a tray icon and popover over the
//! bundled `ppm` sidecar.

mod launch;
mod machines;
mod notify;
mod placement;
mod popover;
mod settings;
mod tray;
mod updates;
#[cfg(windows)]
mod windows;

use std::sync::Mutex;

use tauri::{AppHandle, LogicalSize, Manager, WebviewWindow};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        // First, so a second launch exits before it sets anything up.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            popover::show(app)
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build());
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());
    builder
        .manage(Mutex::new(notify::State::default()))
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let handle = app.handle();
            tray::create(handle)?;
            popover::setup(handle)?;
            settings::setup(handle);
            machines::start(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::ThemeChanged(theme) = event {
                tray::set_theme(window.app_handle(), *theme);
            }
        })
        .invoke_handler(tauri::generate_handler![
            fit_window,
            tray::set_attention_color,
            popover::popover_ready,
            popover::hide_popover,
            popover::has_vibrancy,
            machines::machines_list,
            machines::machines_sync,
            machines::call_machine,
            machines::connect_machine,
            machines::install_ppm,
            launch::open_server_url,
            launch::open_external,
            launch::open_workspace,
            launch::reveal_folder,
            launch::open_in_editor,
            launch::resume_session,
            notify::notification_current,
            notify::notification_action,
            settings::open_settings,
            settings::launch_at_login,
            settings::set_launch_at_login,
            settings::settings_get,
            settings::settings_set_config,
            settings::settings_set_app,
            settings::settings_machines,
            settings::machine_add,
            settings::machine_remove,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Port Process Manager");
}

/// Sizes the calling window to its page's content and keeps it in place.
#[tauri::command]
fn fit_window(app: AppHandle, window: WebviewWindow, width: f64, height: f64) {
    let size = LogicalSize::new(width, height);
    match window.label() {
        popover::LABEL => popover::fit(&app, size),
        notify::LABEL => notify::present(&app, size),
        _ => {
            let _ = window.set_size(size);
        }
    }
}
