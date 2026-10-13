//! ² deck: a spinning braille vinyl record, progress and volume.

use std::f32::consts::{PI, TAU};

use ratatui::{buffer::Buffer, layout::Rect};

use super::{
    fmt,
    glyphs::{Braille, Ln},
};
use crate::app::App;
use crate::theme::{CYAN, DIM, HI, TEXT, TRACK, VIZ_GROOVE, VIZ_RIM};

/// The mock's record is 80×80 dots with outer radius 31; every radius below
/// is in those units and scales with the record.
const REF_DOTS: f32 = 80.0;
const RADIUS: f32 = 31.0;
const MAX_PROGRESS_W: usize = 32;

pub fn draw(buf: &mut Buffer, inner: Rect, app: &App) {
    if inner.height < 3 || inner.width < 4 {
        return;
    }
    let avail_h = (inner.height - 2) as usize;
    let dots = ((inner.width as usize * 2).min(avail_h * 4)) / 4 * 4;
    if dots >= 8 {
        let rows = record(dots, app.record_angle);
        let x0 = (inner.width as usize - dots / 2) / 2;
        let y0 = (avail_h - dots / 4) / 2;
        for (r, ln) in rows.iter().enumerate() {
            ln.put(buf, inner, x0 as u16, (y0 + r) as u16);
        }
    }

    if let Some(m) = &app.media {
        let (pos, dur) = (fmt::mmss(m.pos), fmt::mmss(m.dur));
        let w = MAX_PROGRESS_W.min((inner.width as usize).saturating_sub(pos.len() + dur.len() + 2));
        if w >= 2 {
            let k = ((m.pos / m.dur.max(1.0)).clamp(0.0, 1.0) * (w - 1) as f64).round() as usize;
            let mut ln = Ln::new();
            ln.a(&pos, TEXT).a(" ", TEXT);
            for i in 0..w {
                match i.cmp(&k) {
                    std::cmp::Ordering::Less => ln.a("━", CYAN),
                    std::cmp::Ordering::Equal => ln.a("●", HI),
                    std::cmp::Ordering::Greater => ln.a("─", TRACK),
                };
            }
            ln.a(" ", DIM).a(&dur, DIM);
            ln.put(buf, inner, 0, inner.height - 2);
        }
    }

    let mut ln = Ln::new();
    if let Some(vol) = app.volume {
        let lit = (vol.clamp(0.0, 1.0) * 10.0).round() as usize;
        ln.a("vol ", DIM)
            .a(&"▮".repeat(lit), CYAN)
            .a(&"▯".repeat(10 - lit), TRACK);
        ln.a(&format!(" {}%   ", (vol * 100.0).round() as i32), TEXT);
    }
    let extra = format!("33⅓ rpm · {}", app.audio.bits);
    if ln.width() + extra.chars().count() <= inner.width as usize {
        ln.a(&extra, DIM);
    }
    ln.put(buf, inner, 0, inner.height - 1);
}

/// Renders the record as `dots`×`dots` braille dots (dots/2 cols × dots/4 rows).
fn record(dots: usize, rot: f32) -> Vec<Ln> {
    let mut g = Braille::new(dots / 2, dots / 4);
    let c = dots as f32 / 2.0;
    let scale = dots as f32 / REF_DOTS;
    let dist = |x: f32, y: f32| (x - c).hypot(y - c) / scale;

    for y in 0..g.dots_h() {
        for x in 0..g.dots_w() {
            let (xf, yf) = (x as f32, y as f32);
            let r = dist(xf, yf);
            // Grooves stay 3 dots apart at any size; scaling them aliases.
            let r_dots = r * scale;
            if r > RADIUS {
                continue;
            }
            let ang = ((yf - c).atan2(xf - c) - rot).rem_euclid(TAU);
            let on = if r > RADIUS - 1.2 {
                true // rim
            } else if r > 11.0 {
                // grooves, plus two rotating sheen wedges that show the spin
                let sheen = (ang - 0.9).abs().min((ang - 0.9 - PI).abs());
                r_dots.floor() as i32 % 3 == 0 || sheen < 0.18
            } else if r > 9.5 {
                false // gap between grooves and label
            } else if r > 1.6 {
                !(ang < 0.5 && r > 4.0) // label with a rotating notch
            } else {
                false // spindle hole
            };
            if on {
                g.set(x, y);
            }
        }
    }

    g.rows(|col, row| {
        let d = dist((col * 2 + 1) as f32, (row * 4 + 2) as f32);
        if d < 10.0 {
            CYAN
        } else if d > RADIUS - 2.0 {
            VIZ_RIM
        } else {
            VIZ_GROOVE
        }
    })
}
