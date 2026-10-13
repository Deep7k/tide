//! Panel boxes, top panel and footer.

use ratatui::{buffer::Buffer, layout::Rect, style::Color};

use super::glyphs::{Ln, pips};
use crate::app::App;
use crate::theme::th;

/// Battery meter turns hot below this percentage.
const LOW_BATTERY: u8 = 20;

/// Draws a rounded box with `¹ name · suffix` on its top border and returns
/// its content area.
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
    // An empty name means a plain box with no title.
    let dashes = if !name.is_empty() && title.width() <= inner_w {
        let rest = inner_w - title.width();
        top.append(title);
        rest
    } else {
        inner_w
    };
    top.a(&"─".repeat(dashes), th().border).a("╮", th().border);
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

    content(r)
}

/// A panel's content area: inside the border plus 1 col of padding each side.
pub fn content(r: Rect) -> Rect {
    Rect::new(r.x + 2, r.y + 1, r.width.saturating_sub(4), r.height.saturating_sub(2))
}

/// `r` less one column of padding each side.
fn pad_x(r: Rect) -> Rect {
    Rect::new(r.x + 1, r.y, r.width.saturating_sub(2), r.height)
}

/// Full-width top panel: `◆ user@host` on the left, battery and fps on the
/// right. A 3-row box when there's room, otherwise a single plain line.
pub fn top_panel(buf: &mut Buffer, r: Rect, app: &App) {
    let inner = if r.height >= 3 {
        panel(buf, r, "", "", th().text, "")
    } else {
        pad_x(r)
    };

    let mut left = Ln::new();
    left.a("◆ ", th().accent)
        .bold(&app.user, th().hi)
        .a(&format!("@{}", app.host), th().dim);

    // Right side, from most to least detailed; use the first that fits.
    let fps = |ln: &mut Ln| {
        ln.bold(&format!("{:.0}", app.fps), th().hi).a(" fps", th().dim);
    };
    let mut options = Vec::new();
    if let Some(b) = &app.battery {
        let color = if b.percent < LOW_BATTERY {
            th().cpu_hot
        } else {
            th().battery
        };
        let lit = (b.percent as usize * 10 + 5) / 100;
        let pct = format!(" {}%", b.percent);
        let mut full = Ln::new();
        full.a("bat ", th().dim);
        pips(&mut full, lit, color).bold(&pct, th().hi).a("   ", th().dim);
        fps(&mut full);
        let mut short = Ln::new();
        short.a("bat", th().dim).bold(&pct, th().hi).a("   ", th().dim);
        fps(&mut short);
        options.extend([full, short]);
    }
    let mut only_fps = Ln::new();
    fps(&mut only_fps);
    options.push(only_fps);

    let width = inner.width as usize;
    if let Some(right) = options.into_iter().find(|ln| left.width() + 3 + ln.width() <= width) {
        right.put(buf, inner, (width - right.width()) as u16, 0);
    }
    left.put(buf, inner, 0, 0);
}

/// Key hints, right-aligned, and a transient notice on the left.
pub fn footer(buf: &mut Buffer, r: Rect, notice: Option<&str>) {
    let pad = pad_x(r);
    if let Some(msg) = notice {
        let mut ln = Ln::new();
        ln.a("● ", th().accent).a(msg, th().text);
        ln.put(buf, pad, 0, 0);
    }
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
    ln.put(buf, pad, pad.width.saturating_sub(ln.width() as u16), 0);
}
