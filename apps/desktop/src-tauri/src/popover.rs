//! The popover: created hidden at launch from tauri.conf.json, never
//! destroyed, shown next to the tray icon once the page has rendered data,
//! and hidden on blur, Escape, or another tray click. On macOS it is a
//! non-activating NSPanel so it floats over full-screen apps without taking
//! focus from them.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, Monitor, PhysicalPosition, WebviewWindow};

use crate::placement::{clock_end, on_taskbar, place, Rect};
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

#[derive(Debug, PartialEq)]
enum Toggle {
    Show,
    Hide,
}

impl State {
    /// Whether to show now. Before the page is ready, keeps the request (and
    /// its server) for `ready`.
    fn show(&mut self, server: Option<ServerRef>) -> bool {
        if !self.ready {
            self.pending = Some(server);
            return false;
        }
        self.visible = true;
        true
    }

    /// Whether it was showing.
    fn hide(&mut self, now: Instant) -> bool {
        if !self.visible {
            return false;
        }
        self.visible = false;
        self.hidden_at = Some(now);
        true
    }

    /// What a tray click or the shortcut does. Nothing when a blur just closed
    /// it, because that blur was this same click.
    fn toggle(&self, now: Instant) -> Option<Toggle> {
        if self.visible {
            Some(Toggle::Hide)
        } else if self
            .hidden_at
            .is_some_and(|at| now.duration_since(at) < REOPEN_GUARD)
        {
            None
        } else {
            Some(Toggle::Show)
        }
    }

    /// The page rendered data. Returns a show request that came before.
    fn ready(&mut self) -> Option<Option<ServerRef>> {
        self.ready = true;
        self.pending.take()
    }

    /// Showing, or not yet ready: the page needs data to render its first frame.
    fn wants_data(&self) -> bool {
        self.visible || !self.ready
    }
}

/// A server to open the popover on.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerRef {
    pub machine_id: String,
    pub port: u16,
}

#[cfg(target_os = "macos")]
#[allow(clippy::unused_unit)] // panel_event! needs the explicit `-> ()`.
pub mod panel {
    use tauri::WebviewWindow;
    use tauri_nspanel::objc2_app_kit::{NSWindowAnimationBehavior, NSWindowStyleMask};
    use tauri_nspanel::{CollectionBehavior, PanelHandle, PanelLevel, WebviewWindowExt};

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

    /// Makes `window` a non-activating panel above the menu bar, on every
    /// Space and over full-screen apps. AppKit's own show animation is off,
    /// since the page animates the open.
    pub fn floating(window: &WebviewWindow) -> tauri::Result<PanelHandle<tauri::Wry>> {
        let panel = window.to_panel::<PopoverPanel>()?;
        panel.set_level(PanelLevel::Status.value());
        panel.set_collection_behavior(
            CollectionBehavior::new()
                .can_join_all_spaces()
                .full_screen_auxiliary()
                .stationary()
                .value(),
        );
        if let Err(error) = panel.add_style_mask(NSWindowStyleMask::NonactivatingPanel) {
            eprintln!("could not make the panel non-activating: {error}");
        }
        panel
            .as_panel()
            .setAnimationBehavior(NSWindowAnimationBehavior::None);
        Ok(panel)
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
        let panel = panel::floating(&window)?;
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

fn is_visible(app: &AppHandle) -> bool {
    state(app).visible
}

/// Showing, or not yet ready: the page needs data to render its first frame.
pub fn wants_data(app: &AppHandle) -> bool {
    state(app).wants_data()
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
    let toggle = state(app).toggle(Instant::now());
    match toggle {
        Some(Toggle::Show) => show_now(app, None),
        Some(Toggle::Hide) => hide_now(app),
        None => {}
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
    if !state(app).show(server.clone()) {
        return;
    }
    let window = window(app);
    if let Some(server) = server {
        let _ = window.emit_to(LABEL, "popover:open-server", server);
    }
    if let (Ok(size), Ok(scale)) = (window.outer_size(), window.scale_factor()) {
        position(app, &window, size.to_logical(scale));
    }
    let _ = window.emit_to(LABEL, "popover:visible", true);
    machines::reload_if_changed(app);
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
    if !state(app).hide(Instant::now()) {
        return;
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

/// Resizes the popover to its page and keeps it anchored to the tray. Both
/// happen in one main-thread step, so placement sees the new size.
pub fn fit(app: &AppHandle, size: LogicalSize<f64>) {
    on_main(app, move |app| {
        let window = window(app);
        let _ = window.set_size(size);
        if is_visible(app) {
            position(app, &window, size);
        }
    });
}

/// Places the popover next to the tray icon. Where the tray can't report its
/// rect, it goes next to the cursor on Linux, above the bottom right corner on
/// Windows (the icon may be in the ^ overflow), and top center on macOS.
/// Takes the size to place rather than reading it back, since a resize may not
/// have reached the window yet.
fn position(app: &AppHandle, window: &WebviewWindow, size: LogicalSize<f64>) {
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
        .filter(|icon| !cfg!(windows) || icon_on_taskbar(app, *icon))
        .or_else(|| {
            cfg!(target_os = "linux")
                .then(|| app.cursor_position().ok())
                .flatten()
                .map(|p| Rect {
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
    let Some(monitor) = monitor else { return };
    let scale = monitor.scale_factor();
    let area = work_area(&monitor);
    let anchor = anchor.unwrap_or_else(|| {
        if cfg!(windows) {
            clock_end(bounds(&monitor), area)
        } else {
            Rect {
                x: area.x + area.width / 2.0,
                y: area.y,
                width: 0.0,
                height: 0.0,
            }
        }
    });
    let (x, y) = place(
        anchor,
        (size.width * scale, size.height * scale),
        area,
        GAP * scale,
    );
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

fn icon_on_taskbar(app: &AppHandle, icon: Rect) -> bool {
    let (x, y) = (icon.x + icon.width / 2.0, icon.y + icon.height / 2.0);
    app.monitor_from_point(x, y)
        .ok()
        .flatten()
        .is_some_and(|monitor| on_taskbar(icon, bounds(&monitor), work_area(&monitor)))
}

fn bounds(monitor: &Monitor) -> Rect {
    Rect {
        x: monitor.position().x.into(),
        y: monitor.position().y.into(),
        width: monitor.size().width.into(),
        height: monitor.size().height.into(),
    }
}

fn work_area(monitor: &Monitor) -> Rect {
    let work = monitor.work_area();
    Rect {
        x: work.position.x.into(),
        y: work.position.y.into(),
        width: work.size.width.into(),
        height: work.size.height.into(),
    }
}

/// The page rendered its first data: show it if someone already asked.
#[tauri::command]
pub fn popover_ready(app: AppHandle) {
    let pending = state(&app).ready();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Option<ServerRef> {
        Some(ServerRef {
            machine_id: "local".into(),
            port: 6006,
        })
    }

    #[test]
    fn a_show_before_the_first_frame_waits_for_it() {
        let mut state = State::default();
        // An alert's Details at launch, before the page has data.
        assert!(!state.show(server()));
        assert_eq!(state.ready(), Some(server()));
        assert!(state.show(None));
    }

    #[test]
    fn ready_without_a_request_stays_hidden() {
        let mut state = State::default();
        assert_eq!(state.ready(), None);
        assert!(!state.visible);
    }

    #[test]
    fn the_click_that_closed_it_does_not_reopen_it() {
        let mut state = State::default();
        state.ready();
        let t0 = Instant::now();
        assert_eq!(state.toggle(t0), Some(Toggle::Show));
        state.show(None);
        assert_eq!(state.toggle(t0), Some(Toggle::Hide));
        // A click on the tray blurs the popover first, which hides it...
        assert!(state.hide(t0));
        // ...then the same click arrives as a toggle.
        assert_eq!(state.toggle(t0 + Duration::from_millis(100)), None);
        assert_eq!(
            state.toggle(t0 + Duration::from_millis(400)),
            Some(Toggle::Show)
        );
    }

    #[test]
    fn hiding_twice_is_one_hide() {
        let mut state = State::default();
        state.ready();
        state.show(None);
        assert!(state.hide(Instant::now()));
        assert!(!state.hide(Instant::now()));
    }

    #[test]
    fn data_flows_until_ready_and_then_only_while_showing() {
        let mut state = State::default();
        assert!(state.wants_data());
        state.ready();
        assert!(!state.wants_data());
        state.show(None);
        assert!(state.wants_data());
    }
}
