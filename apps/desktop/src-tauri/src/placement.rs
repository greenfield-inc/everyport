//! Where a tray window goes: next to its anchor (the tray icon, or the cursor
//! when the tray can't report its rect), on the side that faces the screen's
//! work area, and kept inside that work area. The menu bar on macOS and a
//! taskbar on any edge on Windows are the same case.

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    fn right(&self) -> f64 {
        self.x + self.width
    }
    fn bottom(&self) -> f64 {
        self.y + self.height
    }
}

/// Top-left corner for a window of `size` next to `anchor`, `gap` away from
/// the edge it opens from.
pub fn place(anchor: Rect, size: (f64, f64), area: Rect, gap: f64) -> (f64, f64) {
    let (width, height) = size;
    let center_x = anchor.x + anchor.width / 2.0 - width / 2.0;
    let center_y = anchor.y + anchor.height / 2.0 - height / 2.0;
    let (x, y) = if anchor.bottom() <= area.y {
        (center_x, area.y + gap)
    } else if anchor.y >= area.bottom() {
        (center_x, area.bottom() - height - gap)
    } else if anchor.right() <= area.x {
        (area.x + gap, center_y)
    } else if anchor.x >= area.right() {
        (area.right() - width - gap, center_y)
    } else {
        (center_x, anchor.bottom() + gap)
    };
    (
        clamp(x, area.x + gap, area.right() - width - gap),
        clamp(y, area.y + gap, area.bottom() - height - gap),
    )
}

/// Whether a tray icon at `icon` sits on the taskbar, outside the work area.
/// On Windows an icon in the ^ overflow reports a rect inside the flyout,
/// which is hidden by the time the popover shows. A taskbar that hides itself
/// leaves the work area the whole screen, so any rect counts.
pub fn on_taskbar(icon: Rect, bounds: Rect, area: Rect) -> bool {
    let (x, y) = (icon.x + icon.width / 2.0, icon.y + icon.height / 2.0);
    area == bounds || !(area.x..area.right()).contains(&x) || !(area.y..area.bottom()).contains(&y)
}

/// The clock end of the taskbar, as an anchor for `place`: the right end of a
/// top or bottom taskbar, the bottom end of a side one, or the bottom right
/// corner when the taskbar hides itself.
pub fn clock_end(bounds: Rect, area: Rect) -> Rect {
    let (top, bottom) = (area.y - bounds.y, bounds.bottom() - area.bottom());
    let (left, right) = (area.x - bounds.x, bounds.right() - area.right());
    if top > 0.0 {
        Rect {
            x: bounds.right() - top,
            y: bounds.y,
            width: top,
            height: top,
        }
    } else if left > 0.0 {
        Rect {
            x: bounds.x,
            y: bounds.bottom() - left,
            width: left,
            height: left,
        }
    } else if right > 0.0 {
        Rect {
            x: area.right(),
            y: bounds.bottom() - right,
            width: right,
            height: right,
        }
    } else {
        Rect {
            x: bounds.right() - bottom,
            y: area.bottom(),
            width: bottom,
            height: bottom,
        }
    }
}

/// Like `f64::clamp`, but keeps the low edge when the window is bigger than
/// the space.
fn clamp(value: f64, low: f64, high: f64) -> f64 {
    value.min(high).max(low)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A 1512x982 pt MacBook screen at 2x: a 74 px (37 pt) menu bar, and a
    // popover 400x520 pt.
    const MAC_AREA: Rect = Rect {
        x: 0.0,
        y: 74.0,
        width: 3024.0,
        height: 1890.0,
    };
    const POPOVER: (f64, f64) = (800.0, 1040.0);

    #[test]
    fn opens_centered_under_a_menu_bar_icon() {
        let icon = Rect {
            x: 2000.0,
            y: 0.0,
            width: 60.0,
            height: 74.0,
        };
        // Centered: 2030 - 400. Six points (12 px) below the menu bar.
        assert_eq!(place(icon, POPOVER, MAC_AREA, 12.0), (1630.0, 86.0));
    }

    #[test]
    fn stays_on_screen_for_an_icon_near_the_right_edge() {
        let icon = Rect {
            x: 2950.0,
            y: 0.0,
            width: 60.0,
            height: 74.0,
        };
        assert_eq!(
            place(icon, POPOVER, MAC_AREA, 12.0),
            (3024.0 - 800.0 - 12.0, 86.0)
        );
    }

    #[test]
    fn opens_above_a_bottom_taskbar() {
        // 1920x1080 with a 48 px taskbar at the bottom, flyout 400x520.
        let area = Rect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1032.0,
        };
        let icon = Rect {
            x: 1500.0,
            y: 1040.0,
            width: 24.0,
            height: 32.0,
        };
        // Centered: 1512 - 200. Above the taskbar: 1032 - 520 - 12.
        assert_eq!(place(icon, (400.0, 520.0), area, 12.0), (1312.0, 500.0));
    }

    #[test]
    fn opens_beside_a_left_taskbar() {
        let area = Rect {
            x: 48.0,
            y: 0.0,
            width: 1872.0,
            height: 1080.0,
        };
        let icon = Rect {
            x: 8.0,
            y: 900.0,
            width: 32.0,
            height: 24.0,
        };
        // Vertically centered on the icon (912 - 260 = 652), clamped to 1080 - 520 - 12.
        assert_eq!(place(icon, (400.0, 520.0), area, 12.0), (60.0, 548.0));
    }

    // 1920x1080 at 1x, flyout 400x520, 12 px gap.
    const SCREEN: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
    };

    #[test]
    fn an_overflow_icon_opens_above_the_clock() {
        // A 48 px bottom taskbar; the icon reports a rect in the ^ flyout above it.
        let area = Rect {
            height: 1032.0,
            ..SCREEN
        };
        let flyout = Rect {
            x: 1700.0,
            y: 950.0,
            width: 24.0,
            height: 24.0,
        };
        assert!(!on_taskbar(flyout, SCREEN, area));
        // The clock end is the taskbar's last 48 px: centered on 1896, clamped
        // to 1920 - 400 - 12. Above the taskbar: 1032 - 520 - 12.
        let anchor = clock_end(SCREEN, area);
        assert_eq!(place(anchor, (400.0, 520.0), area, 12.0), (1508.0, 500.0));
    }

    #[test]
    fn the_clock_end_follows_the_taskbar_edge() {
        let top = Rect {
            y: 48.0,
            height: 1032.0,
            ..SCREEN
        };
        // Below a top taskbar, at the right.
        assert_eq!(
            place(clock_end(SCREEN, top), (400.0, 520.0), top, 12.0),
            (1508.0, 60.0)
        );
        let left = Rect {
            x: 60.0,
            width: 1860.0,
            ..SCREEN
        };
        // Beside a left taskbar, at the bottom.
        assert_eq!(
            place(clock_end(SCREEN, left), (400.0, 520.0), left, 12.0),
            (72.0, 548.0)
        );
    }

    #[test]
    fn an_auto_hide_taskbar_trusts_the_icon_and_opens_at_the_bottom_right() {
        let icon = Rect {
            x: 1700.0,
            y: 1050.0,
            width: 24.0,
            height: 24.0,
        };
        assert!(on_taskbar(icon, SCREEN, SCREEN));
        assert_eq!(
            place(clock_end(SCREEN, SCREEN), (400.0, 520.0), SCREEN, 12.0),
            (1508.0, 548.0)
        );
    }

    #[test]
    fn opens_below_a_cursor_inside_the_work_area() {
        let area = Rect {
            x: 0.0,
            y: 27.0,
            width: 1920.0,
            height: 1053.0,
        };
        let cursor = Rect {
            x: 1000.0,
            y: 300.0,
            width: 0.0,
            height: 0.0,
        };
        assert_eq!(place(cursor, (400.0, 520.0), area, 6.0), (800.0, 306.0));
    }
}
