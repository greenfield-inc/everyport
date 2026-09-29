//! The tray icon: the socket mark with the server count as its title, amber
//! when a server needs attention. Left-click toggles the popover, right-click
//! opens the menu (Linux desktops often send every click to the menu, so its
//! first item opens the popover).

use std::sync::Mutex;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Theme, Wry};
use tauri_plugin_autostart::ManagerExt as _;

use crate::{popover, settings};

const ID: &str = "tray";
/// Paper's amber, until the page sends the theme's.
const PAPER_AMBER: [u8; 3] = [0xFF, 0xB2, 0x24];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Running(usize),
    Attention(usize),
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let launch_at_login = CheckMenuItem::with_id(
        app,
        "login",
        "Launch at Login",
        true,
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", "Open Everyport", true, None::<&str>)?,
            &MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?,
            &launch_at_login,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit Everyport", true, Some("CmdOrCtrl+Q"))?,
        ],
    )?;
    app.manage(LoginItem(launch_at_login));
    let look = Look {
        state: State::Idle,
        amber: PAPER_AMBER,
        dark_bar: dark_bar(
            app.get_webview_window(popover::LABEL)
                .and_then(|window| window.theme().ok()),
        ),
    };
    let first = icon(&look);
    app.manage(Mutex::new(look));
    TrayIconBuilder::with_id(ID)
        .icon(first)
        .icon_as_template(true)
        .tooltip("Everyport")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => popover::show(app),
            "settings" => settings::open(app),
            "login" => {
                let enable = !settings::launch_at_login(app.clone());
                if let Err(error) = settings::set_launch_at_login(app.clone(), enable) {
                    eprintln!("launch at login: {error}");
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Down,
                ..
            } = event
            {
                popover::toggle(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

struct LoginItem(CheckMenuItem<Wry>);

/// Shows the current launch-at-login state in the menu.
pub fn sync_launch_at_login(app: &AppHandle) {
    let enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let _ = app.state::<LoginItem>().0.set_checked(enabled);
}

pub fn get(app: &AppHandle) -> Option<TrayIcon<Wry>> {
    app.tray_by_id(ID)
}

/// What the icon shows, its attention color (the theme's warning color, which
/// the page sends as sRGB), and whether the panel it sits on is dark.
struct Look {
    state: State,
    amber: [u8; 3],
    dark_bar: bool,
}

/// Most snapshots change nothing the tray shows, so it redraws only on change.
pub fn set_state(app: &AppHandle, state: State) {
    let changed = std::mem::replace(&mut look(app).state, state) != state;
    if changed {
        redraw(app);
    }
}

/// Sets the attention color from the page's theme.
#[tauri::command]
pub fn set_attention_color(app: AppHandle, rgb: [u8; 3]) {
    look(&app).amber = rgb;
    redraw(&app);
}

/// Redraws for the system's light or dark mode. macOS tints the template itself.
pub fn set_theme(app: &AppHandle, theme: Theme) {
    let dark = dark_bar(Some(theme));
    let changed = std::mem::replace(&mut look(app).dark_bar, dark) != dark;
    if changed {
        redraw(app);
    }
}

/// Windows' taskbar follows the system theme; Linux panels are nearly always dark.
fn dark_bar(theme: Option<Theme>) -> bool {
    cfg!(target_os = "linux") || theme != Some(Theme::Light)
}

fn look(app: &AppHandle) -> std::sync::MutexGuard<'_, Look> {
    app.state::<Mutex<Look>>()
        .inner()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn redraw(app: &AppHandle) {
    let (state, image) = {
        let look = look(app);
        (look.state, icon(&look))
    };
    let Some(tray) = get(app) else { return };
    let (count, label) = match state {
        State::Idle => (None, "Everyport, no servers".to_string()),
        State::Running(n) | State::Attention(n) => (
            Some(n.to_string()),
            format!(
                "Everyport, {n} server{}{}",
                if n == 1 { "" } else { "s" },
                if matches!(state, State::Attention(_)) {
                    ", needs attention"
                } else {
                    ""
                }
            ),
        ),
    };
    let _ = tray.set_icon(Some(image));
    // Template images follow the menu bar's appearance; the amber one keeps its color.
    let _ = tray.set_icon_as_template(!matches!(state, State::Attention(_)));
    let _ = tray.set_title(count);
    let _ = tray.set_tooltip(Some(label));
}

/// Renders the socket mark (brand/socket.svg, a 24-unit grid). macOS gets an
/// 18 pt template at 2x, black with the right slot faint when idle. Other
/// systems get a 32 px color icon for their panel with the right slot green
/// while servers run. Attention is amber everywhere. Antialiased by 4x4
/// supersampling.
fn icon(look: &Look) -> Image<'static> {
    const SAMPLES: usize = 4;
    let mac = cfg!(target_os = "macos");
    let size: usize = if mac { 36 } else { 32 };
    let ink = match (mac, look.dark_bar) {
        (true, _) => [0, 0, 0],
        (false, true) => [0xF2, 0xF4, 0xF0],
        (false, false) => [0x1B, 0x1D, 0x1B],
    };
    let green = if look.dark_bar {
        [0x7F, 0xD8, 0x9D]
    } else {
        [0x2F, 0x9A, 0x5D]
    };
    let (main, slot) = match look.state {
        State::Attention(_) => ((look.amber, 255), (look.amber, 255)),
        State::Running(_) if !mac => ((ink, 255), (green, 255)),
        State::Running(_) => ((ink, 255), (ink, 255)),
        State::Idle => ((ink, 255), (ink, 90)),
    };
    // Units 1.5 to 22.5 fill the canvas, leaving a hair of margin round the face.
    let unit = size as f64 / 21.0;
    let mut rgba = vec![0u8; size * size * 4];
    for py in 0..size {
        for px in 0..size {
            let (mut on_main, mut on_slot) = (0usize, 0usize);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (px as f64 + (sx as f64 + 0.5) / SAMPLES as f64) / unit + 1.5;
                    let y = (py as f64 + (sy as f64 + 0.5) / SAMPLES as f64) / unit + 1.5;
                    if in_round_rect(x, y, [13.6, 8.0, 16.0, 14.5], 1.2) {
                        on_slot += 1;
                    } else if (in_round_rect(x, y, [2.0, 2.0, 22.0, 22.0], 7.0)
                        && !in_round_rect(x, y, [4.0, 4.0, 20.0, 20.0], 5.0))
                        || in_round_rect(x, y, [8.0, 8.0, 10.4, 14.5], 1.2)
                        || (x - 12.0).powi(2) + (y - 16.4).powi(2) <= 1.3 * 1.3
                    {
                        on_main += 1;
                    }
                }
            }
            let (main_weight, slot_weight) = (on_main * main.1 as usize, on_slot * slot.1 as usize);
            let weight = main_weight + slot_weight;
            if weight == 0 {
                continue;
            }
            let channel = |c: usize| {
                ((main.0[c] as usize * main_weight + slot.0[c] as usize * slot_weight) / weight)
                    as u8
            };
            let i = (py * size + px) * 4;
            rgba[i..i + 4].copy_from_slice(&[
                channel(0),
                channel(1),
                channel(2),
                (weight / (SAMPLES * SAMPLES)) as u8,
            ]);
        }
    }
    Image::new_owned(rgba, size as u32, size as u32)
}

/// Whether `(x, y)` is inside the rectangle `[left, top, right, bottom]` with corner radius `r`.
fn in_round_rect(x: f64, y: f64, [left, top, right, bottom]: [f64; 4], r: f64) -> bool {
    let dx = (left + r - x).max(x - (right - r)).max(0.0);
    let dy = (top + r - y).max(y - (bottom - r)).max(0.0);
    dx * dx + dy * dy <= r * r
}
