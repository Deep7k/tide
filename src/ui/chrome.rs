//! Panel boxes, header and footer.

use ratatui::{buffer::Buffer, layout::Rect, style::Color};

use super::{fmt, glyphs::Ln};
use crate::app::App;
use crate::theme::th;

/// Draws a rounded box with `¹ name · suffix` on its top border and returns
/// the content area (inside the border plus 1 col of padding each side).
pub fn panel(buf: &mut Buffer, r: Rect, hotkey: &str, name: &str, color: Color, suffix: &str) -> Rect {
    if r.width < 4 || r.height < 2 {
        return Rect::default();
    }
    let all = *buf.area();
    let inner_w = (r.width - 2) as usize;
    let mut top = Ln::new();
    top.a("╭", th().border);
    let mut title = Ln::new();
    title
        .a(" ", th().dim)
        .a(hotkey, th().dim)
        .a(" ", th().dim)
        .a(name, color)
        .a(suffix, th().dim)
        .a(" ", th().dim);
    if title.width() <= inner_w {
        top = top_with_title(top, title, inner_w);
    } else {
        top.a(&"─".repeat(inner_w), th().border);
    }
    top.a("╮", th().border);
    top.put(buf, all, r.x, r.y);

    let mut side = Ln::new();
    side.a("│", th().border);
    for y in r.y + 1..r.bottom() - 1 {
        side.put(buf, all, r.x, y);
        side.put(buf, all, r.right() - 1, y);
    }
    let mut bottom = Ln::new();
    bottom
        .a("╰", th().border)
        .a(&"─".repeat(inner_w), th().border)
        .a("╯", th().border);
    bottom.put(buf, all, r.x, r.bottom() - 1);

    Rect::new(r.x + 2, r.y + 1, r.width.saturating_sub(4), r.height - 2)
}

/// Appends `title` and fills the rest of `w` with border dashes.
fn top_with_title(mut top: Ln, title: Ln, w: usize) -> Ln {
    let rest = w - title.width();
    top.append(title);
    top.a(&"─".repeat(rest), th().border);
    top
}

/// `▶ Title  ·  Artist  ·  Album` on the left, audio info on the right.
pub fn header(buf: &mut Buffer, r: Rect, app: &App) {
    let mut left = Ln::new();
    left.a("▶ ", th().accent);
    match &app.media {
        Some(m) => {
            left.bold(&m.title, th().hi);
            for part in [&m.artist, &m.album].into_iter().filter(|s| !s.is_empty()) {
                left.a("  ·  ", th().dim).a(part, th().dim);
            }
        }
        None => {
            left.a("—", th().dim);
        }
    }
    let a = &app.audio;
    let info = format!(
        "{} · {} · fft {} · {:.0} fps",
        a.source,
        fmt::khz(a.sample_rate),
        a.fft,
        app.fps
    );
    let mut right = Ln::new();
    right.a(&info, th().dim);

    // 1 col of padding each side; the right text yields to the title.
    let pad = Rect::new(r.x + 1, r.y, r.width.saturating_sub(2), r.height);
    let right_x = pad.width as usize - right.width().min(pad.width as usize);
    if left.width() + 3 <= right_x {
        right.put(buf, pad, right_x as u16, 0);
    }
    left.put(buf, pad, 0, 0);
}

/// Key hints, right-aligned.
pub fn footer(buf: &mut Buffer, r: Rect) {
    let mut ln = Ln::new();
    for (i, (key, what)) in [("␣", " pause"), ("← →", " seek"), ("q", " quit")]
        .into_iter()
        .enumerate()
    {
        if i > 0 {
            ln.a("  ", th().dim);
        }
        ln.a(key, th().text).a(what, th().dim);
    }
    let pad = Rect::new(r.x + 1, r.y, r.width.saturating_sub(2), r.height);
    ln.put(buf, pad, pad.width.saturating_sub(ln.width() as u16), 0);
}
