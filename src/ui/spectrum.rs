//! ¹ spectrum: log-spaced bars with peak caps, frequency axis, stereo VU.

use ratatui::{buffer::Buffer, layout::Rect};

use super::{
    fmt,
    glyphs::{E, Ln},
};
use crate::app::App;
use crate::theme::{grad, th};

/// Centre frequency of band `i` of `n`, log-spaced from `fmin` to `fmax`.
pub fn band_hz(i: usize, n: usize, fmin: f64, fmax: f64) -> f64 {
    fmin * (fmax / fmin).powf(i as f64 / (n.max(2) - 1) as f64)
}

pub fn draw(buf: &mut Buffer, inner: Rect, app: &App, rows: u16) {
    let n = app.bands.len();
    let (fmin, fmax) = (app.cfg.audio.freq_min, app.cfg.audio.freq_max);
    let rows_n = rows as usize;

    // bars, coloured by row from the bottom up
    for r in 0..rows_n {
        let fb = rows_n - 1 - r;
        let col = grad(&th().viz, fb as f32 / (rows_n.max(2) - 1) as f32);
        let mut ln = Ln::new();
        for i in 0..n {
            let lv = (app.bands[i] * rows as f32 * 8.0).round() as i32 - fb as i32 * 8;
            let peak = app.peaks.get(i).copied().unwrap_or(0.0);
            if lv >= 8 {
                ln.a("██", col);
            } else if lv > 0 {
                let g = E[lv as usize];
                ln.a(g, col).a(g, col);
            } else if app.cfg.spectrum.peak_caps && (peak * rows as f32).floor() as usize == fb {
                ln.a("▔▔", th().hi);
            } else {
                ln.a("  ", col);
            }
            ln.a(" ", col);
        }
        ln.put(buf, inner, 0, r as u16);
    }

    // frequency axis: label every 5th band
    let w = n * 3;
    let mut axis = vec![' '; w];
    for i in (0..n).step_by(5) {
        let label = fmt::freq_label(band_hz(i, n, fmin, fmax));
        let start = (i * 3).min(w.saturating_sub(1 + label.len()));
        for (k, ch) in label.chars().enumerate() {
            if let Some(cell) = axis.get_mut(start + k) {
                *cell = ch;
            }
        }
    }
    Ln::with(&axis.into_iter().collect::<String>(), th().dim).put(buf, inner, 0, rows);

    // VU meters after one blank row
    let vu_w = (inner.width as usize).saturating_sub(12);
    for (k, vu) in app.vu.iter().enumerate() {
        let mut ln = Ln::new();
        ln.a(if k == 0 { "L " } else { "R " }, th().dim);
        for i in 0..vu_w {
            let on = (i as f32 / vu_w as f32) < vu.level;
            ln.a(
                "▌",
                if on {
                    grad(&th().viz, i as f32 / (vu_w.max(2) - 1) as f32)
                } else {
                    th().track
                },
            );
        }
        ln.a(&format!("{:>7.1} dB", vu.db.max(-99.9)), th().dim);
        ln.put(buf, inner, 0, rows + 2 + k as u16);
    }
}
