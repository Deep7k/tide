//! ² deck: a spinning braille vinyl record, progress and volume.

use std::f32::consts::{PI, TAU};

use ratatui::{buffer::Buffer, layout::Rect};

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
            ln.put(buf, block, 0, inner.height - 2);
        }
    }

    if vol_w <= block_w {
        let pad = block_w - vol_ln.width() - extra.chars().count();
        vol_ln.a(&" ".repeat(pad), th().dim).a(&extra, th().dim);
    }
    vol_ln.put(buf, block, 0, inner.height - 1);
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
            th().record_label
        } else if d > RADIUS - 2.0 {
            th().record_rim
        } else {
            th().record_groove
        }
    })
}
