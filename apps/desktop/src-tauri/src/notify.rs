//! Memory alerts, shown as our own borderless card at the top right for 8 s
//! with Details, Stop and Snooze 1h. The system notification APIs Tauri
//! exposes on desktop can't carry action buttons. The window is created on
//! the first alert and kept hidden between alerts.

use std::sync::Mutex;
use std::time::Duration;

use ppm_client::protocol::{Alert, Call, Server};
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::machines;
use crate::placement::Rect;
use crate::popover::{self, ServerRef};

pub const LABEL: &str = "notification";
const SHOW_FOR: Duration = Duration::from_secs(8);
/// Points from the screen's top right corner.
const MARGIN: f64 = 12.0;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    machine_id: String,
    server: Server,
    alert: Alert,
}

#[derive(Default)]
pub struct State {
    current: Option<Notice>,
    /// Bumped on every alert, so an older timer doesn't hide a newer card.
    shown: u64,
    /// Between an alert and its timeout or action.
    active: bool,
}

fn state(app: &AppHandle) -> std::sync::MutexGuard<'_, State> {
    app.state::<Mutex<State>>()
        .inner()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn show(app: &AppHandle, machine_id: &str, server: Server, alert: Alert) {
    let notice = Notice {
        machine_id: machine_id.to_string(),
        server,
        alert,
    };
    let shown = {
        let mut state = state(app);
        state.current = Some(notice.clone());
        state.shown += 1;
        state.active = true;
        state.shown
    };
    // Alerts arrive on a Tokio thread; AppKit builds the panel on the main one.
    // The page renders the card, then calls `fit_window`, which shows it.
    popover::on_main(app, move |app| match window(app) {
        Ok(window) => {
            let _ = window.emit_to(LABEL, "notification", notice);
        }
        Err(error) => eprintln!("notification: {error}"),
    });
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SHOW_FOR).await;
        if state(&app).shown == shown {
            hide(&app);
        }
    });
}

/// Shows the rendered card, if its alert is still current.
pub fn present(app: &AppHandle) {
    popover::on_main(app, present_now);
}

fn present_now(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if !state(app).active {
        return;
    }
    position(app, &window);
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        if let Ok(panel) = app.get_webview_panel(LABEL) {
            panel.order_front_regardless();
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = window.show();
}

fn hide(app: &AppHandle) {
    state(app).active = false;
    popover::on_main(app, |app| {
        if let Some(window) = app.get_webview_window(LABEL) {
            let _ = window.hide();
        }
    });
}

fn window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(LABEL) {
        return Ok(window);
    }
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Port Process Manager")
        .inner_size(356.0, 120.0)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focused(false)
        .visible(false)
        .build()?;
    popover::native_look(&window, 22.0);
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::objc2_app_kit::NSWindowStyleMask;
        use tauri_nspanel::{CollectionBehavior, PanelLevel, WebviewWindowExt};

        let panel = window.to_panel::<popover::panel::PopoverPanel>()?;
        panel.set_level(PanelLevel::Status.value());
        panel.set_collection_behavior(
            CollectionBehavior::new()
                .can_join_all_spaces()
                .full_screen_auxiliary()
                .stationary()
                .value(),
        );
        let _ = panel.add_style_mask(NSWindowStyleMask::NonactivatingPanel);
    }
    Ok(window)
}

/// Top right of the screen with the tray icon.
fn position(app: &AppHandle, window: &WebviewWindow) {
    let monitor = crate::tray::get(app)
        .and_then(|tray| tray.rect().ok().flatten())
        .and_then(|rect| {
            let p = rect.position.to_physical::<f64>(1.0);
            app.monitor_from_point(p.x, p.y).ok().flatten()
        })
        .or_else(|| app.primary_monitor().ok().flatten());
    let (Some(monitor), Ok(size)) = (monitor, window.outer_size()) else {
        return;
    };
    let work = monitor.work_area();
    let area = Rect {
        x: work.position.x.into(),
        y: work.position.y.into(),
        width: work.size.width.into(),
        height: work.size.height.into(),
    };
    let margin = MARGIN * monitor.scale_factor();
    let x = area.x + area.width - f64::from(size.width) - margin;
    let _ = window.set_position(PhysicalPosition::new(x as i32, (area.y + margin) as i32));
}

#[tauri::command]
pub fn notification_current(app: AppHandle) -> Option<Notice> {
    state(&app).current.clone()
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Details,
    Stop,
    Snooze,
}

#[tauri::command]
pub async fn notification_action(app: AppHandle, action: Action) -> Result<(), String> {
    let Some(notice) = state(&app).current.clone() else {
        return Ok(());
    };
    hide(&app);
    let (machine_id, port) = (notice.machine_id, notice.server.port);
    match action {
        Action::Details => popover::show_server(&app, Some(ServerRef { machine_id, port })),
        Action::Snooze => machines::snooze(&app, &machine_id, port),
        Action::Stop => {
            let call = Call::Stop {
                port,
                root: notice.server.root,
                force: false,
            };
            machines::call(&app, &machine_id, call).await?;
        }
    }
    Ok(())
}
