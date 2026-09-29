//! The tray icon: a 5x5 dot grid with the server count as its title, amber
//! when a server needs attention. Left-click toggles the popover, right-click
//! opens the menu (Linux desktops often send every click to the menu, so its
//! first item opens the popover).

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_autostart::ManagerExt as _;

use crate::{popover, settings};

const ID: &str = "tray";

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
            &MenuItem::with_id(app, "open", "Open Port Process Manager", true, None::<&str>)?,
            &MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?,
            &launch_at_login,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                "quit",
                "Quit Port Process Manager",
                true,
                Some("CmdOrCtrl+Q"),
            )?,
        ],
    )?;
    app.manage(LoginItem(launch_at_login));
    TrayIconBuilder::with_id(ID)
        .icon(icon(State::Idle))
        .icon_as_template(true)
        .tooltip("Port Process Manager")
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

pub fn set_state(app: &AppHandle, state: State) {
    let Some(tray) = get(app) else { return };
    let (count, label) = match state {
        State::Idle => (None, "Port Process Manager, no servers".to_string()),
        State::Running(n) | State::Attention(n) => (
            Some(n.to_string()),
            format!(
                "Port Process Manager, {n} server{}{}",
                if n == 1 { "" } else { "s" },
                if matches!(state, State::Attention(_)) {
                    ", needs attention"
                } else {
                    ""
                }
            ),
        ),
    };
    let _ = tray.set_icon(Some(icon(state)));
    // Template images follow the menu bar's appearance; the amber one keeps its color.
    let _ = tray.set_icon_as_template(!matches!(state, State::Attention(_)));
    let _ = tray.set_title(count);
    let _ = tray.set_tooltip(Some(label));
}

/// `#` is lit, `a` is amber, `.` is unlit.
fn glyph(state: State) -> [&'static str; 5] {
    match state {
        State::Idle => [".....", ".....", ".....", ".....", "....."],
        State::Running(_) => [".....", "..#..", ".....", "..#..", "....."],
        State::Attention(_) => ["aaaaa", "aa.aa", "aaaaa", "aa.aa", "aaaaa"],
    }
}

/// Renders the grid at 2x for an 18 pt menu bar: 36 px square with a 32 px
/// grid, each dot 76% of its cell, antialiased by 4x4 supersampling.
fn icon(state: State) -> Image<'static> {
    const SIZE: usize = 36;
    const GRID: f64 = 32.0;
    const SAMPLES: usize = 4;
    let rows = glyph(state);
    let pitch = GRID / 5.0;
    let radius = pitch * 0.76 / 2.0;
    let inset = (SIZE as f64 - GRID) / 2.0;
    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    for py in 0..SIZE {
        for px in 0..SIZE {
            let (row, column) = (
                ((py as f64 - inset) / pitch).floor(),
                ((px as f64 - inset) / pitch).floor(),
            );
            if !(0.0..5.0).contains(&row) || !(0.0..5.0).contains(&column) {
                continue;
            }
            let cx = inset + (column + 0.5) * pitch;
            let cy = inset + (row + 0.5) * pitch;
            let mut covered = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = px as f64 + (sx as f64 + 0.5) / SAMPLES as f64 - cx;
                    let y = py as f64 + (sy as f64 + 0.5) / SAMPLES as f64 - cy;
                    covered += usize::from(x * x + y * y <= radius * radius);
                }
            }
            let dot = rows[row as usize].as_bytes()[column as usize];
            // Unlit dots are faint in the template icon, and dark holes in the amber one.
            let [r, g, b, a] = match dot {
                b'a' => [0xFF, 0xB2, 0x24, 255],
                b'#' => [0, 0, 0, 255],
                _ if matches!(state, State::Attention(_)) => [0x40, 0x40, 0x40, 255],
                _ => [0, 0, 0, 80],
            };
            let alpha = a as usize * covered / (SAMPLES * SAMPLES);
            let i = (py * SIZE + px) * 4;
            rgba[i..i + 4].copy_from_slice(&[r, g, b, alpha as u8]);
        }
    }
    Image::new_owned(rgba, SIZE as u32, SIZE as u32)
}
