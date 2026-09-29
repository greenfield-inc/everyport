//! Terminal colors. Port colors come from the default theme's chart colors,
//! amber and red from the design, and text levels are the terminal's own
//! foreground blended over its background, like the popover's translucent text.

use ratatui::style::{Color, Modifier, Style};
use std::io::IsTerminal;
use std::time::Duration;

type Rgb = (u8, u8, u8);

/// `pastel-dreams-green` `--chart-1` to `--chart-5`, as (light, dark).
const PORTS: [(Rgb, Rgb); 5] = [
    ((0x31, 0xAA, 0x40), (0x57, 0x7E, 0x6F)),
    ((0x02, 0x95, 0x00), (0x4A, 0xB3, 0x41)),
    ((0x00, 0x85, 0x00), (0x00, 0x9D, 0x1E)),
    ((0x00, 0x78, 0x00), (0x00, 0x8E, 0x18)),
    ((0x00, 0x68, 0x00), (0x00, 0x7D, 0x1E)),
];
const AMBER: (Rgb, Rgb) = ((0x96, 0x60, 0x00), (0xFF, 0xB2, 0x24));
const RED: (Rgb, Rgb) = ((0xBA, 0x30, 0x28), (0xFF, 0x69, 0x61));

#[derive(Clone, Copy, PartialEq, Eq)]
enum Depth {
    None,
    Ansi256,
    TrueColor,
}

#[derive(Clone, Copy)]
pub struct Palette {
    depth: Depth,
    dark: bool,
    foreground: Option<Rgb>,
    background: Option<Rgb>,
}

impl Palette {
    /// Reads the terminal's colors. Call before entering raw mode.
    pub fn detect() -> Self {
        let depth = if std::env::var_os("NO_COLOR").is_some() || !std::io::stdout().is_terminal() {
            Depth::None
        } else if matches!(
            std::env::var("COLORTERM").as_deref(),
            Ok("truecolor" | "24bit")
        ) || std::env::var_os("WT_SESSION").is_some()
        {
            Depth::TrueColor
        } else {
            Depth::Ansi256
        };
        let options = {
            let mut options = terminal_colorsaurus::QueryOptions::default();
            options.timeout = Duration::from_millis(300);
            options
        };
        let colors = (depth != Depth::None)
            .then(|| terminal_colorsaurus::color_palette(options).ok())
            .flatten();
        let rgb = |c: &terminal_colorsaurus::Color| c.scale_to_8bit();
        Self {
            depth,
            dark: colors
                .as_ref()
                .is_none_or(|c| c.theme_mode() == terminal_colorsaurus::ThemeMode::Dark),
            foreground: colors.as_ref().map(|c| rgb(&c.foreground)),
            background: colors.as_ref().map(|c| rgb(&c.background)),
        }
    }

    pub fn has_color(&self) -> bool {
        self.depth != Depth::None
    }

    fn pick(&self, pair: (Rgb, Rgb)) -> Color {
        self.color(if self.dark { pair.1 } else { pair.0 })
    }

    fn color(&self, rgb: Rgb) -> Color {
        match self.depth {
            Depth::None => Color::Reset,
            Depth::Ansi256 => Color::Indexed(ansi256(rgb)),
            Depth::TrueColor => Color::Rgb(rgb.0, rgb.1, rgb.2),
        }
    }

    /// The terminal foreground at `alpha` over its background, or `None` when
    /// the terminal didn't report its colors.
    fn text(&self, dark: f32, light: f32) -> Option<Color> {
        let (fg, bg) = (self.foreground?, self.background?);
        Some(self.color(blend(fg, bg, if self.dark { dark } else { light })))
    }

    pub fn text1(&self) -> Color {
        Color::Reset
    }
    pub fn text2(&self) -> Option<Color> {
        self.text(0.6, 0.72)
    }
    pub fn text3(&self) -> Option<Color> {
        self.text(0.4, 0.62)
    }
    /// Unlit dots and the other-apps baseline.
    pub fn faint(&self) -> Option<Color> {
        self.text(0.28, 0.28)
    }
    /// Free memory and dividers.
    pub fn track(&self) -> Option<Color> {
        self.text(0.12, 0.14)
    }
    /// Background of the selected row, like a hovered row in the popover.
    pub fn highlight(&self) -> Option<Color> {
        self.text(0.08, 0.06)
    }
    pub fn amber(&self) -> Color {
        self.pick(AMBER)
    }
    pub fn red(&self) -> Color {
        self.pick(RED)
    }
    pub fn port(&self, index: usize) -> Color {
        self.pick(PORTS[index % PORTS.len()])
    }
    /// A port color at partial opacity, for unselected memory-bar segments.
    pub fn faded_port(&self, index: usize, alpha: f32) -> Style {
        self.faded(PORTS[index % PORTS.len()], alpha)
    }
    /// Amber at partial opacity, for the threshold line.
    pub fn faded_amber(&self, alpha: f32) -> Style {
        self.faded(AMBER, alpha)
    }
    /// Blended over the background, or dim when the terminal didn't report it.
    fn faded(&self, pair: (Rgb, Rgb), alpha: f32) -> Style {
        match self.background {
            Some(bg) => Style::new().fg(self.color(blend(
                if self.dark { pair.1 } else { pair.0 },
                bg,
                alpha,
            ))),
            None => Style::new().fg(self.pick(pair)).add_modifier(Modifier::DIM),
        }
    }

    /// An ANSI foreground escape for plain printed output, empty without color.
    pub fn ansi(&self, color: Color) -> String {
        match color {
            Color::Rgb(r, g, b) => format!("\x1b[38;2;{r};{g};{b}m"),
            Color::Indexed(i) => format!("\x1b[38;5;{i}m"),
            _ => String::new(),
        }
    }
}

fn blend(fg: Rgb, bg: Rgb, alpha: f32) -> Rgb {
    let mix = |f: u8, b: u8| (f as f32 * alpha + b as f32 * (1.0 - alpha)).round() as u8;
    (mix(fg.0, bg.0), mix(fg.1, bg.1), mix(fg.2, bg.2))
}

/// Nearest xterm 256-color index, from the 6x6x6 cube or the gray ramp.
fn ansi256((r, g, b): Rgb) -> u8 {
    const LEVELS: [i32; 6] = [0, 95, 135, 175, 215, 255];
    let nearest = |v: u8| {
        (0..6)
            .min_by_key(|&i| (LEVELS[i] - v as i32).abs())
            .unwrap_or(0)
    };
    let (ri, gi, bi) = (nearest(r), nearest(g), nearest(b));
    let (r, g, b) = (r as i32, g as i32, b as i32);
    let gray_index = (((r + g + b) / 3 - 8 + 5) / 10).clamp(0, 23);
    let gray = 8 + gray_index * 10;
    let distance = |x: (i32, i32, i32)| (r - x.0).pow(2) + (g - x.1).pow(2) + (b - x.2).pow(2);
    if distance((gray, gray, gray)) < distance((LEVELS[ri], LEVELS[gi], LEVELS[bi])) {
        (232 + gray_index) as u8
    } else {
        (16 + 36 * ri + 6 * gi + bi) as u8
    }
}
