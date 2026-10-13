//! Cell-level drawing helpers, ported from the mock's `Ln`, `bar`, `spark`,
//! `spark2` (as `spark_rows`) and `brl`.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
};

use crate::theme::th;

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

    /// A line holding one run of text.
    pub fn with(t: &str, c: Color) -> Self {
        let mut ln = Self::new();
        ln.a(t, c);
        ln
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

    /// Plain text without styles.
    #[cfg(test)]
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.content.as_ref()).collect()
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
        // Every glyph tide draws is one cell wide, so characters go straight into
        // cells; ratatui's string path does grapheme segmentation per span.
        let (mut x, y, right) = (area.x + dx, area.y + dy, area.right());
        for s in &self.spans {
            for ch in s.content.chars() {
                if x >= right {
                    return;
                }
                buf[(x, y)].set_char(ch).set_style(s.style);
                x += 1;
            }
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
            ln.a("░", th().track);
        }
    }
    ln
}

/// Ten-step `▮▮▮▯▯` meter with `lit` steps on.
pub fn pips(ln: &mut Ln, lit: usize, c: Color) -> &mut Ln {
    ln.a(&"▮".repeat(lit), c).a(&"▯".repeat(10 - lit), th().track)
}

/// One-row sparkline of the last `w` values (0..1).
pub fn spark<'a>(ln: &'a mut Ln, vals: &[f32], w: usize, c: Color) -> &'a mut Ln {
    ln.append(spark_rows(vals, w, 1, c).remove(0))
}

/// Multi-row sparkline of the last `w` values (0..1), top row first (8 levels
/// per row; the mock's net graphs are the 2-row case). Short history is
/// left-padded so the rows keep their width.
pub fn spark_rows(vals: &[f32], w: usize, rows: usize, c: Color) -> Vec<Ln> {
    let vals = &vals[vals.len().saturating_sub(w)..];
    let mut out: Vec<Ln> = (0..rows).map(|_| Ln::new()).collect();
    let pad = " ".repeat(w - vals.len());
    for ln in &mut out {
        ln.a(&pad, c);
    }
    for v in vals {
        let h = ((v.clamp(0.0, 1.0) * (rows * 8) as f32).round() as usize).max(1);
        // Row 0 is the top; each row shows the eighths that fall within it.
        for (r, ln) in out.iter_mut().enumerate() {
            let below = (rows - 1 - r) * 8;
            ln.a(E[h.saturating_sub(below).min(8)], c);
        }
    }
    out
}

/// Braille canvas: each cell holds 2×4 dots.
#[derive(Clone)]
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

    #[test]
    fn bar_eighths() {
        let mut ln = Ln::new();
        bar(&mut ln, 0.5, 4, Color::Red);
        assert_eq!(ln.text(), "██░░");
        let mut ln = Ln::new();
        bar(&mut ln, 0.3, 4, Color::Red); // 9.6 → 10 eighths = 1 full + 2/8
        assert_eq!(ln.text(), "█▎░░");
        let mut ln = Ln::new();
        bar(&mut ln, 1.0, 3, Color::Red);
        assert_eq!(ln.text(), "███");
    }

    #[test]
    fn spark_pads_short_history() {
        let mut ln = Ln::new();
        spark(&mut ln, &[0.0, 1.0], 4, Color::Red);
        assert_eq!(ln.text(), "  ▁█");
        assert_eq!(ln.width(), 4);
    }

    #[test]
    fn spark_rows_splits_levels_across_rows() {
        let rows = spark_rows(&[0.25, 0.75, 1.0], 3, 2, Color::Red);
        let (top, bot) = (&rows[0], &rows[1]);
        assert_eq!(bot.text(), "▄██");
        assert_eq!(top.text(), " ▄█");
    }

    #[test]
    fn braille_bits() {
        let mut b = Braille::new(1, 1);
        b.set(0, 0);
        b.set(1, 3);
        let rows = b.rows(|_, _| Color::Red);
        assert_eq!(rows[0].text(), "\u{2881}");
    }
}
