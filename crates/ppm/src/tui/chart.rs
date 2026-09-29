//! Braille drawing for sparklines and the memory chart: each character holds
//! 2x4 dots.

pub struct Canvas {
    pub columns: usize,
    pub rows: usize,
    bits: Vec<u8>,
}

const MASKS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

impl Canvas {
    pub fn new(columns: usize, rows: usize) -> Self {
        Self {
            columns,
            rows,
            bits: vec![0; columns * rows],
        }
    }

    pub fn dot_width(&self) -> usize {
        self.columns * 2
    }

    pub fn dot_height(&self) -> usize {
        self.rows * 4
    }

    pub fn set(&mut self, x: i64, y: i64) {
        if x < 0 || y < 0 || x as usize >= self.dot_width() || y as usize >= self.dot_height() {
            return;
        }
        let (x, y) = (x as usize, y as usize);
        self.bits[(y / 4) * self.columns + x / 2] |= MASKS[y % 4][x % 2];
    }

    /// Joins the points with straight lines, so the plot has no gaps.
    pub fn plot(&mut self, points: &[(i64, i64)]) {
        if let Some(&(x, y)) = points.first() {
            self.set(x, y);
        }
        for pair in points.windows(2) {
            self.line(pair[0], pair[1]);
        }
    }

    fn line(&mut self, (mut x, mut y): (i64, i64), (x1, y1): (i64, i64)) {
        let (dx, dy) = ((x1 - x).abs(), -(y1 - y).abs());
        let (sx, sy) = (if x < x1 { 1 } else { -1 }, if y < y1 { 1 } else { -1 });
        let mut error = dx + dy;
        loop {
            self.set(x, y);
            if x == x1 && y == y1 {
                break;
            }
            let doubled = 2 * error;
            if doubled >= dy {
                error += dy;
                x += sx;
            }
            if doubled <= dx {
                error += dx;
                y += sy;
            }
        }
    }

    pub fn is_empty(&self, row: usize, column: usize) -> bool {
        self.bits[row * self.columns + column] == 0
    }

    pub fn char(&self, row: usize, column: usize) -> char {
        char::from_u32(0x2800 + self.bits[row * self.columns + column] as u32).unwrap_or(' ')
    }

    pub fn text(&self, row: usize) -> String {
        (0..self.columns)
            .map(|column| self.char(row, column))
            .collect()
    }
}

/// Evenly spaced values on one row, scaled to their own range.
pub fn sparkline(values: &[f64], columns: usize) -> String {
    let (Some(low), Some(high)) = (
        values.iter().copied().reduce(f64::min),
        values.iter().copied().reduce(f64::max),
    ) else {
        return "⠄".repeat(columns);
    };
    if values.len() < 2 {
        return "⠄".repeat(columns);
    }
    let mut canvas = Canvas::new(columns, 1);
    let range = (high - low).max(high * 0.05).max(1.0);
    let width = (canvas.dot_width() - 1) as f64;
    let mut points: Vec<(i64, i64)> = Vec::new();
    for (index, value) in values.iter().enumerate() {
        let x = (index as f64 / (values.len() - 1) as f64 * width).round() as i64;
        let y = (((1.0 - (value - low) / range) * 0.8 + 0.1) * 3.0).round() as i64;
        match points.last_mut() {
            Some(last) if last.0 == x => last.1 = y,
            _ => points.push((x, y)),
        }
    }
    canvas.plot(&points);
    canvas.text(0)
}
