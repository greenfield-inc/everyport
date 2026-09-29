//! The tray icon: the socket mark with the server count, amber when a server
//! needs attention. macOS shows the count as the icon's title; Windows can't
//! show text by a tray icon and Linux panels may hide it, so there the count is
//! a badge drawn into the icon. Left-click toggles the popover, right-click
//! opens the menu (Linux desktops often send every click to the menu, so its
//! first item opens the popover).

use std::sync::Mutex;
use std::time::Duration;

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
        generation: 0,
        amber: PAPER_AMBER,
        dark_bar: dark_bar(
            app.get_webview_window(popover::LABEL)
                .and_then(|window| window.theme().ok()),
        ),
    };
    let first = icon(&look, false);
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

/// What the icon shows, which redraw it is on, its attention color (the
/// theme's warning color, which the page sends as sRGB), and whether the panel
/// it sits on is dark.
struct Look {
    state: State,
    generation: u64,
    amber: [u8; 3],
    dark_bar: bool,
}

impl State {
    fn count(self) -> Option<usize> {
        match self {
            State::Idle => None,
            State::Running(n) | State::Attention(n) => Some(n),
        }
    }
}

/// Most snapshots change nothing the tray shows, so it redraws only on change,
/// and pulses the badge when the count changes.
pub fn set_state(app: &AppHandle, state: State) {
    let old = std::mem::replace(&mut look(app).state, state);
    if old != state {
        redraw(app, old.count() != state.count() && state.count().is_some());
    }
}

/// Sets the attention color from the page's theme.
#[tauri::command]
pub fn set_attention_color(app: AppHandle, rgb: [u8; 3]) {
    look(&app).amber = rgb;
    redraw(&app, false);
}

/// Redraws for the system's light or dark mode. macOS tints the template itself.
pub fn set_theme(app: &AppHandle, theme: Theme) {
    let dark = dark_bar(Some(theme));
    let changed = std::mem::replace(&mut look(app).dark_bar, dark) != dark;
    if changed {
        redraw(app, false);
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

/// Shows the current look. A pulse draws the badge a size up first and settles
/// it 150 ms later, unless the system asks for reduced motion.
fn redraw(app: &AppHandle, pulse: bool) {
    let pulse = pulse && animations_enabled();
    let (state, generation, image) = {
        let mut look = look(app);
        look.generation += 1;
        (look.state, look.generation, icon(&look, pulse))
    };
    let Some(tray) = get(app) else { return };
    if pulse {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            let handle = app.clone();
            // Every icon change runs on the main thread, so a newer redraw
            // either lands after this one or makes it skip.
            let _ = app.run_on_main_thread(move || {
                let image = {
                    let look = look(&handle);
                    if look.generation != generation {
                        return;
                    }
                    icon(&look, false)
                };
                if let Some(tray) = get(&handle) {
                    let _ = tray.set_icon(Some(image));
                }
            });
        });
    }
    let label = match state {
        State::Idle => "Everyport, no servers".to_string(),
        State::Running(n) | State::Attention(n) => {
            format!(
                "Everyport, {n} server{}{}",
                if n == 1 { "" } else { "s" },
                if matches!(state, State::Attention(_)) {
                    ", needs attention"
                } else {
                    ""
                }
            )
        }
    };
    let _ = tray.set_icon(Some(image));
    // Template images follow the menu bar's appearance; the amber one keeps its color.
    let _ = tray.set_icon_as_template(!matches!(state, State::Attention(_)));
    if cfg!(target_os = "macos") {
        let _ = tray.set_title(state.count().map(|n| n.to_string()));
    }
    let _ = tray.set_tooltip(Some(label));
}

/// Windows' "Animation effects" switch. Linux has no single reduce-motion
/// setting to read, so the badge there doesn't pulse.
fn animations_enabled() -> bool {
    #[cfg(windows)]
    return crate::windows::animations_enabled();
    #[cfg(not(windows))]
    false
}

/// The notification area's icon size at the system DPI on Windows, so the
/// badge is drawn pixel for pixel rather than scaled. Linux panels scale the
/// 32 px image, which halves cleanly to 16.
fn panel_icon_size() -> usize {
    #[cfg(windows)]
    return crate::windows::tray_icon_size();
    #[cfg(not(windows))]
    32
}

/// Renders the socket mark (brand/socket.svg, a 24-unit grid). macOS gets an
/// 18 pt template at 2x, black with the right slot faint when idle. Other
/// systems get a color icon at their panel's size with the right slot green
/// while servers run, and the count badge. Attention is amber everywhere.
/// Antialiased by 4x4 supersampling.
fn icon(look: &Look, grow_badge: bool) -> Image<'static> {
    let mac = cfg!(target_os = "macos");
    let size = if mac { 36 } else { panel_icon_size() };
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
    // Darker on a light bar, so white digits read on it.
    let badge_green = if look.dark_bar {
        green
    } else {
        [0x1E, 0x7B, 0x46]
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
                    let (x, y) = sample(px, py, sx, sy);
                    let (x, y) = (x / unit + 1.5, y / unit + 1.5);
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
    match look.state {
        State::Running(n) if !mac => badge(&mut rgba, size, n, badge_green, grow_badge),
        State::Attention(n) if !mac => badge(&mut rgba, size, n, look.amber, grow_badge),
        _ => {}
    }
    Image::new_owned(rgba, size as u32, size as u32)
}

const SAMPLES: usize = 4;

/// The position of subsample `(sx, sy)` of pixel `(px, py)`.
fn sample(px: usize, py: usize, sx: usize, sy: usize) -> (f64, f64) {
    (
        px as f64 + (sx as f64 + 0.5) / SAMPLES as f64,
        py as f64 + (sy as f64 + 0.5) / SAMPLES as f64,
    )
}

/// A pixel font: one row of bits per line, leftmost pixel in the highest bit,
/// for the glyphs 1 to 9 and +. Counts above 9 show as 9+, so 0 never appears.
struct Font {
    width: usize,
    glyphs: [&'static [u8]; 10],
}

const FONT_3X5: Font = Font {
    width: 3,
    glyphs: [
        &[2, 6, 2, 2, 7],
        &[7, 1, 7, 4, 7],
        &[7, 1, 7, 1, 7],
        &[5, 5, 7, 1, 1],
        &[7, 4, 7, 1, 7],
        &[7, 4, 7, 5, 7],
        &[7, 1, 1, 2, 2],
        &[7, 5, 7, 5, 7],
        &[7, 5, 7, 1, 7],
        &[0, 2, 7, 2, 0],
    ],
};

const FONT_5X7: Font = Font {
    width: 5,
    glyphs: [
        &[0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        &[0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        &[0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        &[0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        &[0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        &[0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        &[0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        &[0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        &[0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        &[0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00],
    ],
};

/// The badge's text: the count, or 9+ above 9.
fn badge_text(count: usize) -> String {
    if count > 9 {
        "9+".to_string()
    } else {
        count.to_string()
    }
}

/// Black or white, whichever reads better on `fill` (WCAG contrast).
fn ink_on(fill: [u8; 3]) -> [u8; 3] {
    if contrast(fill, [0, 0, 0]) >= contrast(fill, [255, 255, 255]) {
        [0, 0, 0]
    } else {
        [255, 255, 255]
    }
}

/// WCAG 2 contrast ratio between two sRGB colors.
fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let luminance = |rgb: [u8; 3]| {
        let linear = |c: u8| {
            let c = c as f64 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
    };
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Draws the count on a rounded badge in the bottom-right corner, with a clear
/// gap cut from the mark around it so it reads on any panel. The digits sit on
/// whole pixels at the largest font and scale that fit about a third of the
/// icon's height, so they stay sharp at 16 px. `grow` draws it one step larger.
fn badge(rgba: &mut [u8], size: usize, count: usize, fill: [u8; 3], grow: bool) {
    let cap = size as f64 * 0.32;
    let (small, large) = ((cap / 5.0) as usize, (cap / 7.0) as usize);
    let (font, k) = if large * 7 > small * 5 {
        (&FONT_5X7, large)
    } else {
        (&FONT_3X5, small.max(1))
    };
    let glyphs: Vec<&[u8]> = badge_text(count)
        .chars()
        .map(|c| font.glyphs[c.to_digit(10).map_or(9, |d| d as usize - 1)])
        .collect();
    let rows = font.glyphs[0].len();
    let text_width = (glyphs.len() * (font.width + 1) - 1) * k;
    let height = (rows + 2) * k;
    let width = (text_width + 2 * k).max(height);
    let (left, top) = (size - width, size - height);
    let step = if grow { k as f64 } else { 0.0 };
    let body = [
        left as f64 - step,
        top as f64 - step,
        size as f64,
        size as f64,
    ];
    let radius = (height as f64 + step) * 0.3;
    let gap = [body[0] - k as f64, body[1] - k as f64, body[2], body[3]];
    for py in 0..size {
        for px in 0..size {
            let (mut in_body, mut in_gap) = (0, 0);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let (x, y) = sample(px, py, sx, sy);
                    in_body += in_round_rect(x, y, body, radius) as usize;
                    in_gap += in_round_rect(x, y, gap, radius + k as f64) as usize;
                }
            }
            let i = (py * size + px) * 4;
            let samples = SAMPLES * SAMPLES;
            let mark_alpha = rgba[i + 3] as usize * (samples - in_gap) / samples;
            let body_alpha = 255 * in_body / samples;
            rgba[i + 3] = (body_alpha + mark_alpha * (255 - body_alpha) / 255) as u8;
            if body_alpha > 0 {
                rgba[i..i + 3].copy_from_slice(&fill);
            }
        }
    }
    let ink = ink_on(fill);
    let text_left = left + (width - text_width) / 2;
    for (n, glyph) in glyphs.iter().enumerate() {
        for (row, bits) in glyph.iter().enumerate() {
            for column in 0..font.width {
                if bits >> (font.width - 1 - column) & 1 == 0 {
                    continue;
                }
                for dy in 0..k {
                    for dx in 0..k {
                        let x = text_left + (n * (font.width + 1) + column) * k + dx;
                        let y = top + (row + 1) * k + dy;
                        let i = (y * size + x) * 4;
                        rgba[i..i + 4].copy_from_slice(&[ink[0], ink[1], ink[2], 255]);
                    }
                }
            }
        }
    }
}

/// Whether `(x, y)` is inside the rectangle `[left, top, right, bottom]` with corner radius `r`.
fn in_round_rect(x: f64, y: f64, [left, top, right, bottom]: [f64; 4], r: f64) -> bool {
    let dx = (left + r - x).max(x - (right - r)).max(0.0);
    let dy = (top + r - y).max(y - (bottom - r)).max(0.0);
    dx * dx + dy * dy <= r * r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_above_nine_show_as_nine_plus() {
        assert_eq!(badge_text(1), "1");
        assert_eq!(badge_text(9), "9");
        assert_eq!(badge_text(10), "9+");
        assert_eq!(badge_text(128), "9+");
    }

    #[test]
    fn badge_digits_meet_wcag_aa_on_every_fill() {
        // The two greens, Paper's amber, the light-mode fallback amber, and a
        // mid grey where black and white read about equally.
        for fill in [
            [0x7F, 0xD8, 0x9D],
            [0x1E, 0x7B, 0x46],
            [0xFF, 0xB2, 0x24],
            [0xB4, 0x53, 0x09],
            [0x77, 0x77, 0x77],
        ] {
            assert!(contrast(ink_on(fill), fill) >= 4.5, "{fill:?}");
        }
    }
}
