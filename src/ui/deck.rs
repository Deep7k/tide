//! ² deck: a spinning braille vinyl record, progress and volume.

use std::cell::RefCell;
use std::f32::consts::{PI, TAU};

use ratatui::{buffer::Buffer, layout::Rect, style::Color};

use super::{
    fmt,
    glyphs::{Braille, Ln},
};
use crate::app::App;
use crate::theme::th;

/// The mock's record is 80×80 dots with outer radius 31; every radius below
/// is in those units and scales with the record.
const REF_DOTS: f32 = 80.0;
const RADIUS: f32 = 31.0;
/// Narrowest control block: "m:ss " + 32-cell bar + " m:ss" as in the design.
const MIN_BLOCK_W: usize = 42;

/// Rows: song line, record, then progress, volume and audio source at the bottom.
pub fn draw(buf: &mut Buffer, inner: Rect, app: &App) {
    if inner.height < 5 || inner.width < 4 {
        return;
    }
    let h = inner.height;
    let avail_h = (h - 4) as usize;
    let dots = ((inner.width as usize * 2).min(avail_h * 4)) / 4 * 4;
    if dots >= 8 {
        let x0 = (inner.width as usize - dots / 2) / 2;
        let y0 = 1 + (avail_h - dots / 4) / 2;
        draw_record(buf, inner, x0 as u16, y0 as u16, dots, app.record_angle);
    }

    // Volume on the left, format details on the right.
    let mut vol_ln = Ln::new();
    if let Some(vol) = app.volume {
        let lit = (vol.clamp(0.0, 1.0) * 10.0).round() as usize;
        vol_ln
            .a("vol ", th().dim)
            .a(&"▮".repeat(lit), th().accent)
            .a(&"▯".repeat(10 - lit), th().track);
        let level = if app.muted {
            " muted".to_string()
        } else {
            format!(" {}%", (vol * 100.0).round() as i32)
        };
        vol_ln.a(&level, th().text);
    }
    let extra = format!("33⅓ rpm · {}", app.audio.bits);
    let gap = if vol_ln.width() == 0 { 0 } else { 3 };
    let vol_w = vol_ln.width() + gap + extra.chars().count();

    // Progress and volume form a block centred under the record, at least as
    // wide as the design's progress line and the volume line, if the deck allows.
    let block_w = (dots / 2).max(MIN_BLOCK_W).max(vol_w).min(inner.width as usize);
    let block = Rect::new(
        inner.x + (inner.width - block_w as u16) / 2,
        inner.y,
        block_w as u16,
        inner.height,
    );

    if let Some(m) = app.media.as_ref().filter(|m| m.dur > 0.0) {
        let (pos, dur) = (fmt::mmss(m.pos), fmt::mmss(m.dur));
        let w = block_w.saturating_sub(pos.len() + dur.len() + 2);
        if w >= 2 {
            let k = ((m.pos / m.dur.max(1.0)).clamp(0.0, 1.0) * (w - 1) as f64).round() as usize;
            let mut ln = Ln::new();
            ln.a(&pos, th().text).a(" ", th().text);
            for i in 0..w {
                match i.cmp(&k) {
                    std::cmp::Ordering::Less => ln.a("━", th().accent),
                    std::cmp::Ordering::Equal => ln.a("●", th().hi),
                    std::cmp::Ordering::Greater => ln.a("─", th().track),
                };
            }
            ln.a(" ", th().dim).a(&dur, th().dim);
            ln.put(buf, block, 0, h - 3);
        }
    }

    if vol_w <= block_w {
        let pad = block_w - vol_ln.width() - extra.chars().count();
        vol_ln.a(&" ".repeat(pad), th().dim).a(&extra, th().dim);
    }
    vol_ln.put(buf, block, 0, h - 2);

    let a = &app.audio;
    let mut src = Ln::new();
    src.a(
        &format!("{} · {} · fft {}", a.source, fmt::khz(a.sample_rate), a.fft),
        th().dim,
    );
    src.put(buf, block, 0, h - 1);

    // Now playing, across the top of the deck: title, artist, album (… if cut off).
    let mut song = Ln::new();
    match &app.media {
        Some(m) => {
            song.bold(&m.title, th().hi);
            for part in [&m.artist, &m.album].into_iter().filter(|s| !s.is_empty()) {
                song.a(" · ", th().dim).a(part, th().dim);
            }
        }
        None => {
            song.a("nothing playing", th().dim);
        }
    }
    song.put(buf, inner, 0, 0);
    if song.width() > inner.width as usize {
        buf[(inner.right() - 1, inner.y)].set_char('…').set_fg(th().dim);
    }
}

/// Per-size record geometry. The rim, groove lines and most of the label never
/// change with rotation, so they're drawn once; each frame only tests the
/// dots that the two sheen wedges or the label notch can switch.
struct Geometry {
    dots: usize,
    fixed: Braille,
    /// (x, y, angle at zero rotation) of groove dots lit only inside a sheen wedge.
    sheen: Vec<(u16, u16, f32)>,
    /// Label dots unlit while inside the notch.
    notch: Vec<(u16, u16, f32)>,
    /// Colour of each cell, row-major.
    colors: Vec<Color>,
}

impl Geometry {
    fn new(dots: usize) -> Self {
        let mut fixed = Braille::new(dots / 2, dots / 4);
        let c = dots as f32 / 2.0;
        let scale = dots as f32 / REF_DOTS;
        let dist = |x: f32, y: f32| (x - c).hypot(y - c) / scale;
        let (mut sheen, mut notch) = (Vec::new(), Vec::new());
        for y in 0..fixed.dots_h() {
            for x in 0..fixed.dots_w() {
                let (xf, yf) = (x as f32, y as f32);
                let r = dist(xf, yf);
                let ang = (yf - c).atan2(xf - c);
                let p = (x as u16, y as u16, ang);
                if r > RADIUS {
                    // outside the record
                } else if r > RADIUS - 1.2 {
                    fixed.set(x, y); // rim
                } else if r > 11.0 {
                    // Grooves stay 3 dots apart at any size; scaling them aliases.
                    if (r * scale).floor() as i32 % 3 == 0 {
                        fixed.set(x, y);
                    } else {
                        sheen.push(p);
                    }
                } else if r > 9.5 {
                    // gap between grooves and label
                } else if r > 4.0 {
                    notch.push(p); // label, except where the notch passes
                } else if r > 1.6 {
                    fixed.set(x, y); // label centre
                }
                // r ≤ 1.6: spindle hole
            }
        }
        let colors = (0..fixed.rows)
            .flat_map(|row| (0..fixed.cols).map(move |col| (col, row)))
            .map(|(col, row)| {
                let d = dist((col * 2 + 1) as f32, (row * 4 + 2) as f32);
                if d < 10.0 {
                    th().record_label
                } else if d > RADIUS - 2.0 {
                    th().record_rim
                } else {
                    th().record_groove
                }
            })
            .collect();
        Self {
            dots,
            fixed,
            sheen,
            notch,
            colors,
        }
    }

    fn render(&self, rot: f32) -> Vec<Ln> {
        let mut g = self.fixed.clone();
        let spun = |a: f32| (a - rot).rem_euclid(TAU);
        for &(x, y, a) in &self.sheen {
            // two highlight wedges, half a turn apart, that make the spin visible
            let a = spun(a);
            if (a - 0.9).abs().min((a - 0.9 - PI).abs()) < 0.18 {
                g.set(x as usize, y as usize);
            }
        }
        for &(x, y, a) in &self.notch {
            if spun(a) >= 0.5 {
                g.set(x as usize, y as usize);
            }
        }
        let cols = g.cols;
        g.rows(|col, row| self.colors[row * cols + col])
    }
}

struct RecordCache {
    geometry: Geometry,
    /// Last rotation drawn and its rows; reused while the record is stopped.
    last: Option<(f32, Vec<Ln>)>,
}

thread_local! {
    static RECORD: RefCell<Option<RecordCache>> = const { RefCell::new(None) };
}

/// Draws the record (`dots`×`dots` braille dots) at (x0, y0) inside `area`.
fn draw_record(buf: &mut Buffer, area: Rect, x0: u16, y0: u16, dots: usize, rot: f32) {
    RECORD.with_borrow_mut(|cache| {
        if cache.as_ref().is_none_or(|c| c.geometry.dots != dots) {
            *cache = Some(RecordCache {
                geometry: Geometry::new(dots),
                last: None,
            });
        }
        let c = cache.as_mut().expect("just set");
        if c.last.as_ref().is_none_or(|(r, _)| *r != rot) {
            c.last = Some((rot, c.geometry.render(rot)));
        }
        if let Some((_, rows)) = &c.last {
            for (r, ln) in rows.iter().enumerate() {
                ln.put(buf, area, x0, y0 + r as u16);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cached renderer must match a direct evaluation of the mock's rules.
    #[test]
    fn cached_record_matches_direct_rules() {
        let dots = 80;
        let rot = 1.234;
        let fast = Geometry::new(dots).render(rot);
        let mut g = Braille::new(dots / 2, dots / 4);
        let c = dots as f32 / 2.0;
        for y in 0..dots {
            for x in 0..dots {
                let (xf, yf) = (x as f32, y as f32);
                let r = (xf - c).hypot(yf - c);
                let ang = ((yf - c).atan2(xf - c) - rot).rem_euclid(TAU);
                let on = if r > RADIUS {
                    false
                } else if r > RADIUS - 1.2 {
                    true
                } else if r > 11.0 {
                    let sheen = (ang - 0.9).abs().min((ang - 0.9 - PI).abs());
                    r.floor() as i32 % 3 == 0 || sheen < 0.18
                } else if r > 9.5 {
                    false
                } else if r > 1.6 {
                    !(ang < 0.5 && r > 4.0)
                } else {
                    false
                };
                if on {
                    g.set(x, y);
                }
            }
        }
        let slow = g.rows(|_, _| Color::Reset);
        let text = |rows: &[Ln]| rows.iter().map(Ln::text).collect::<Vec<_>>();
        assert_eq!(text(&fast), text(&slow));
    }
}
