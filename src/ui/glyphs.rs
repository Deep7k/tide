//! Cell-level drawing helpers, ported from the mock's `Ln`, `bar`, `spark`,
//! `spark2` and `brl`.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
};

use crate::theme::TRACK;

/// Vertical eighth blocks, index = eighths filled.
pub const E: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
/// Horizontal eighth blocks for the partial cell of a bar.
const LE: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];

/// One row of coloured text. Adjacent runs with the same style are merged.
/// Every glyph tide uses is one cell wide, so width = char count.
#[derive(Default)]
pub struct Ln {
    spans: Vec<Span<'static>>,
    width: usize,
}

impl Ln {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn a(&mut self, t: &str, c: Color) -> &mut Self {
        self.push(t, Style::new().fg(c))
    }

    pub fn bold(&mut self, t: &str, c: Color) -> &mut Self {
        self.push(t, Style::new().fg(c).add_modifier(Modifier::BOLD))
    }

    fn push(&mut self, t: &str, style: Style) -> &mut Self {
        if t.is_empty() {
            return self;
        }
        self.width += t.chars().count();
        if let Some(last) = self.spans.last_mut()
            && last.style == style
        {
            last.content.to_mut().push_str(t);
        } else {
            self.spans.push(Span::styled(t.to_owned(), style));
        }
        self
    }

    pub fn append(&mut self, other: Ln) -> &mut Self {
        for s in other.spans {
            self.push(&s.content, s.style);
        }
        self
    }

    pub fn width(&self) -> usize {
        self.width
    }

    /// Draws at `(dx, dy)` inside `area`, clipped to it.
    pub fn put(&self, buf: &mut Buffer, area: Rect, dx: u16, dy: u16) {
        let area = area.intersection(*buf.area());
        if dy >= area.height || dx >= area.width {
            return;
        }
        let (mut x, y, right) = (area.x + dx, area.y + dy, area.right());
        for s in &self.spans {
            if x >= right {
                break;
            }
            x = buf.set_stringn(x, y, &s.content, (right - x) as usize, s.style).0;
        }
    }
}

/// Horizontal bar of width `w` at eighth-cell resolution on a `░` track.
pub fn bar(ln: &mut Ln, f: f32, w: usize, c: Color) -> &mut Ln {
    let e = (f.clamp(0.0, 1.0) * w as f32 * 8.0).round() as usize;
    let (full, part) = (e / 8, e % 8);
    for i in 0..w {
        if i < full {
            ln.a("█", c);
        } else if i == full && part > 0 {
            ln.a(LE[part], c);
        } else {
            ln.a("░", TRACK);
        }
    }
    ln
}

/// One-row sparkline of the last `w` values (0..1). Short history is
/// left-padded so the row keeps its width.
pub fn spark<'a>(ln: &'a mut Ln, vals: &[f32], w: usize, c: Color) -> &'a mut Ln {
    let vals = &vals[vals.len().saturating_sub(w)..];
    ln.a(&" ".repeat(w - vals.len()), c);
    for v in vals {
        ln.a(E[((v.clamp(0.0, 1.0) * 8.0).round() as usize).max(1)], c);
    }
    ln
}

/// Two-row sparkline (16 levels) of the last `w` values: `(top, bottom)`.
pub fn spark2(vals: &[f32], w: usize, c: Color) -> (Ln, Ln) {
    let vals = &vals[vals.len().saturating_sub(w)..];
    let (mut top, mut bot) = (Ln::new(), Ln::new());
    let pad = " ".repeat(w - vals.len());
    top.a(&pad, c);
    bot.a(&pad, c);
    for v in vals {
        let h = ((v.clamp(0.0, 1.0) * 16.0).round() as usize).max(1);
        bot.a(E[h.min(8)], c);
        top.a(if h > 8 { E[h - 8] } else { " " }, c);
    }
    (top, bot)
}

/// Braille canvas: each cell holds 2×4 dots.
pub struct Braille {
    pub cols: usize,
    pub rows: usize,
    g: Vec<u8>,
}

impl Braille {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            g: vec![0; cols * rows],
        }
    }

    pub fn dots_w(&self) -> usize {
        self.cols * 2
    }

    pub fn dots_h(&self) -> usize {
        self.rows * 4
    }

    pub fn set(&mut self, x: usize, y: usize) {
        const BITS: [[u8; 4]; 2] = [[1, 2, 4, 64], [8, 16, 32, 128]];
        if x < self.dots_w() && y < self.dots_h() {
            self.g[(y / 4) * self.cols + x / 2] |= BITS[x & 1][y & 3];
        }
    }

    /// Converts to text rows; `color(col, row)` picks each cell's colour.
    pub fn rows(&self, color: impl Fn(usize, usize) -> Color) -> Vec<Ln> {
        (0..self.rows)
            .map(|r| {
                let mut ln = Ln::new();
                for c in 0..self.cols {
                    let v = self.g[r * self.cols + c];
                    let ch = char::from_u32(0x2800 + v as u32).unwrap_or(' ');
                    let mut tmp = [0u8; 4];
                    ln.a(if v == 0 { " " } else { ch.encode_utf8(&mut tmp) }, color(c, r));
                }
                ln
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(ln: &Ln) -> String {
        ln.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn bar_eighths() {
        let mut ln = Ln::new();
        bar(&mut ln, 0.5, 4, Color::Red);
        assert_eq!(text(&ln), "██░░");
        let mut ln = Ln::new();
        bar(&mut ln, 0.3, 4, Color::Red); // 9.6 → 10 eighths = 1 full + 2/8
        assert_eq!(text(&ln), "█▎░░");
        let mut ln = Ln::new();
        bar(&mut ln, 1.0, 3, Color::Red);
        assert_eq!(text(&ln), "███");
    }

    #[test]
    fn spark_pads_short_history() {
        let mut ln = Ln::new();
        spark(&mut ln, &[0.0, 1.0], 4, Color::Red);
        assert_eq!(text(&ln), "  ▁█");
        assert_eq!(ln.width(), 4);
    }

    #[test]
    fn spark2_splits_sixteen_levels() {
        let (top, bot) = spark2(&[0.25, 0.75, 1.0], 3, Color::Red);
        assert_eq!(text(&bot), "▄██");
        assert_eq!(text(&top), " ▄█");
    }

    #[test]
    fn braille_bits() {
        let mut b = Braille::new(1, 1);
        b.set(0, 0);
        b.set(1, 3);
        let rows = b.rows(|_, _| Color::Red);
        assert_eq!(text(&rows[0]), "\u{2881}");
    }
}
