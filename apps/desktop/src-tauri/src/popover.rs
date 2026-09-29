//! The popover: created hidden at launch from tauri.conf.json, never
//! destroyed, shown next to the tray icon once the page has rendered data,
//! and hidden on blur, Escape, or another tray click. On macOS it is a
//! non-activating NSPanel so it floats over full-screen apps without taking
//! focus from them.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

use crate::placement::{place, Rect};
use crate::{machines, tray};

pub const LABEL: &str = "popover";
/// Points between the menu bar or taskbar and the popover.
const GAP: f64 = 6.0;
/// A tray click that lands this soon after a blur closed the popover was the
/// click that closed it, so it must not reopen it.
const REOPEN_GUARD: Duration = Duration::from_millis(300);

#[derive(Default)]
pub struct State {
    ready: bool,
    visible: bool,
    /// Asked to show before the page was ready.
    pending: Option<Option<ServerRef>>,
    hidden_at: Option<Instant>,
    vibrancy: bool,
}

/// A server to open the popover on.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerRef {
    pub machine_id: String,
    pub port: u16,
}

#[cfg(target_os = "macos")]
#[allow(clippy::unused_unit)] // panel_event! needs the explicit `-> ()`.
pub mod panel {
    use tauri_nspanel::PanelHandle;

    tauri_nspanel::tauri_panel! {
        panel!(PopoverPanel {
            config: {
                can_become_key_window: true,
                is_floating_panel: true
            }
        })

        panel_event!(PopoverEvents {
            window_did_resign_key(notification: &NSNotification) -> ()
        })
    }

    /// Calls `hide` when the panel loses key focus, such as on a click elsewhere.
    pub fn hide_on_resign_key(panel: &PanelHandle<tauri::Wry>, hide: impl Fn() + 'static) {
        let events = PopoverEvents::new();
        events.window_did_resign_key(move |_| hide());
        panel.set_event_handler(Some(events.as_ref()));
    }
}

/// Turns the configured window into the native popover.
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let window = window(app);
    let vibrancy = native_look(&window, 16.0);
    app.manage(Mutex::new(State {
        vibrancy,
        ..State::default()
    }));

    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::objc2_app_kit::NSWindowStyleMask;
        use tauri_nspanel::{CollectionBehavior, PanelLevel, WebviewWindowExt};

        let panel = window.to_panel::<panel::PopoverPanel>()?;
        panel.set_level(PanelLevel::Status.value());
        panel.set_collection_behavior(
            CollectionBehavior::new()
                .can_join_all_spaces()
                .full_screen_auxiliary()
                .stationary()
                .value(),
        );
        if let Err(error) = panel.add_style_mask(NSWindowStyleMask::NonactivatingPanel) {
            eprintln!("popover: could not make the panel non-activating: {error}");
        }
        let handle = app.clone();
        panel::hide_on_resign_key(&panel, move || hide(&handle));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let handle = app.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::Focused(false) = event {
                hide(&handle);
            }
        });
    }
    Ok(())
}

/// Native blur behind the transparent page, clipped to the page's corner
/// `radius` on macOS. On Windows also rounded corners and no browser
/// shortcuts or context menu. Returns whether blur applied.
pub fn native_look(window: &WebviewWindow, radius: f64) -> bool {
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
        apply_vibrancy(
            window,
            NSVisualEffectMaterial::Popover,
            Some(NSVisualEffectState::Active),
            Some(radius),
        )
        .is_ok()
    }
    #[cfg(windows)]
    {
        let _ = radius;
        crate::windows::round_corners(window);
        crate::windows::disable_browser_keys(window);
        window_vibrancy::apply_mica(window, None).is_ok()
            || window_vibrancy::apply_acrylic(window, Some((0, 0, 0, 0))).is_ok()
    }
    #[cfg(target_os = "linux")]
    {
        let _ = (window, radius);
        false
    }
}

fn window(app: &AppHandle) -> WebviewWindow {
    app.get_webview_window(LABEL)
        .expect("tauri.conf.json defines the popover window")
}

fn state(app: &AppHandle) -> std::sync::MutexGuard<'_, State> {
    app.state::<Mutex<State>>()
        .inner()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn is_visible(app: &AppHandle) -> bool {
    state(app).visible
}

/// Runs `f` on the main thread, which AppKit requires for window calls.
/// Callers include the single-instance socket and async commands.
pub fn on_main(app: &AppHandle, f: impl FnOnce(&AppHandle) + Send + 'static) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || f(&handle));
}

pub fn toggle(app: &AppHandle) {
    on_main(app, toggle_now);
}

fn toggle_now(app: &AppHandle) {
    let (visible, just_hidden) = {
        let state = state(app);
        let just_hidden = state
            .hidden_at
            .is_some_and(|at| at.elapsed() < REOPEN_GUARD);
        (state.visible, just_hidden)
    };
    if visible {
        hide_now(app);
    } else if !just_hidden {
        show_now(app, None);
    }
}

pub fn show(app: &AppHandle) {
    show_server(app, None);
}

/// Shows the popover, opened on `server`'s detail when given.
pub fn show_server(app: &AppHandle, server: Option<ServerRef>) {
    on_main(app, move |app| show_now(app, server));
}

fn show_now(app: &AppHandle, server: Option<ServerRef>) {
    {
        let mut state = state(app);
        if !state.ready {
            state.pending = Some(server);
            return;
        }
        state.visible = true;
    }
    let window = window(app);
    if let Some(server) = server {
        let _ = window.emit_to(LABEL, "popover:open-server", server);
    }
    position(app, &window);
    let _ = window.emit_to(LABEL, "popover:visible", true);
    machines::publish(app);
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        if let Ok(panel) = app.get_webview_panel(LABEL) {
            panel.show_and_make_key();
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn hide(app: &AppHandle) {
    on_main(app, hide_now);
}

fn hide_now(app: &AppHandle) {
    {
        let mut state = state(app);
        if !state.visible {
            return;
        }
        state.visible = false;
        state.hidden_at = Some(Instant::now());
    }
    let window = window(app);
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        if let Ok(panel) = app.get_webview_panel(LABEL) {
            panel.hide();
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = window.hide();
    let _ = window.emit_to(LABEL, "popover:visible", false);
}

/// Keeps the popover anchored to the tray after its size changes.
pub fn reposition(app: &AppHandle) {
    if is_visible(app) {
        position(app, &window(app));
    }
}

/// Places the popover next to the tray icon, or the cursor where the tray
/// can't report its rect (Linux), or the top center of the main screen.
fn position(app: &AppHandle, window: &WebviewWindow) {
    let anchor = tray::get(app)
        .and_then(|tray| tray.rect().ok().flatten())
        .map(|rect| {
            let position = rect.position.to_physical::<f64>(1.0);
            let size = rect.size.to_physical::<f64>(1.0);
            Rect {
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
            }
        })
        .or_else(|| {
            app.cursor_position().ok().map(|p| Rect {
                x: p.x,
                y: p.y,
                width: 0.0,
                height: 0.0,
            })
        });
    let monitor = anchor
        .and_then(|a| {
            app.monitor_from_point(a.x + a.width / 2.0, a.y + a.height / 2.0)
                .ok()
                .flatten()
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
    let anchor = anchor.unwrap_or(Rect {
        x: area.x + area.width / 2.0,
        y: area.y,
        width: 0.0,
        height: 0.0,
    });
    let (x, y) = place(
        anchor,
        (size.width.into(), size.height.into()),
        area,
        GAP * monitor.scale_factor(),
    );
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// The page rendered its first data: show it if someone already asked.
#[tauri::command]
pub fn popover_ready(app: AppHandle) {
    let pending = {
        let mut state = state(&app);
        state.ready = true;
        state.pending.take()
    };
    if let Some(server) = pending {
        show_server(&app, server);
    }
}

#[tauri::command]
pub fn hide_popover(app: AppHandle) {
    hide(&app);
}

/// Whether native blur sits behind the page, so it should draw a translucent
/// tint instead of a solid background.
#[tauri::command]
pub fn has_vibrancy(app: AppHandle) -> bool {
    state(&app).vibrancy
}
