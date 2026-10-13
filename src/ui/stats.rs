//! ³ cpu · ⁴ mem · ⁵ gpu · ⁶ net · ⁷ proc

use ratatui::{buffer::Buffer, layout::Rect};

use super::{
    fmt,
    glyphs::{Ln, bar, spark, spark2},
};
use crate::app::App;
use crate::theme::{AMBER, CORAL, CYAN, DIM, HI, ROSE, TEXT, VIOLET};

/// A core bar turns hot above this load.
const HOT: f32 = 0.82;
const PROC_ROWS: usize = 6;
const PROC_NAME_W: usize = 22;

pub fn cpu(buf: &mut Buffer, inner: Rect, app: &App) {
    let c = &app.cpu;
    let w = inner.width as usize;

    let hist: Vec<f32> = c.hist.iter().copied().collect();
    let mut ln = Ln::new();
    ln.a("total ", DIM);
    spark(&mut ln, &hist, w.saturating_sub(11), AMBER).a(&fmt::pct4(c.total), HI);
    ln.put(buf, inner, 0, 0);

    let label_w = format!("c{} ", c.cores.len().saturating_sub(1)).len();
    for (i, &v) in c.cores.iter().enumerate() {
        let mut ln = Ln::new();
        ln.a(&format!("{:<label_w$}", format!("c{i}")), DIM);
        bar(
            &mut ln,
            v,
            w.saturating_sub(label_w + 5),
            if v > HOT { ROSE } else { AMBER },
        )
        .a(&fmt::pct4(v), TEXT);
        ln.put(buf, inner, 0, 1 + i as u16);
    }

    let mut parts = vec![format!("{:.2} GHz", c.ghz)];
    parts.extend(c.temp_c.map(|t| format!("{t:.0}°C")));
    parts.push(format!("tide {:.1}%", c.self_use * 100.0));
    let mut ln = Ln::new();
    ln.a(&parts.join(" · "), DIM);
    ln.put(buf, inner, 0, 1 + c.cores.len() as u16);
}

pub fn mem(buf: &mut Buffer, inner: Rect, app: &App) {
    let m = &app.mem;
    let w = inner.width as usize;
    let ratio = |used: u64, total: u64| if total == 0 { 0.0 } else { used as f32 / total as f32 };
    for (row, (label, used, total)) in [("ram  ", m.ram_used, m.ram_total), ("swap ", m.swap_used, m.swap_total)]
        .into_iter()
        .enumerate()
    {
        let mut ln = Ln::new();
        ln.a(label, DIM);
        bar(&mut ln, ratio(used, total), w.saturating_sub(14), VIOLET)
            .a(&format!(" {:>8}", fmt::used_total_g(used, total)), TEXT);
        ln.put(buf, inner, 0, row as u16);
    }
    let mut ln = Ln::new();
    ln.a("tide ", DIM).a(
        &format!("{} · ring buffer {}", fmt::total(m.self_bytes), fmt::mib1(m.ring_bytes)),
        TEXT,
    );
    ln.put(buf, inner, 0, 2);
}

pub fn gpu(buf: &mut Buffer, inner: Rect, app: &App) {
    let g = &app.gpu;
    let w = inner.width as usize;
    let label = if g.name.chars().count() < 5 {
        format!("{:<5}", g.name)
    } else {
        format!("{} ", g.name)
    };
    let mut ln = Ln::new();
    ln.a(&label, DIM);
    bar(&mut ln, g.util, w.saturating_sub(label.chars().count() + 5), CORAL).a(&fmt::pct4(g.util), TEXT);
    ln.put(buf, inner, 0, 0);

    let mut parts = vec![format!("vram {}", fmt::used_total_g(g.vram_used, g.vram_total))];
    parts.extend(g.temp_c.map(|t| format!("{t:.0}°C")));
    parts.extend(g.watts.map(|p| format!("{p:.0}W")));
    let mut ln = Ln::new();
    ln.a(&parts.join(" · "), DIM);
    ln.put(buf, inner, 0, 1);
}

pub fn net(buf: &mut Buffer, inner: Rect, app: &App) {
    let n = &app.net;
    let w = inner.width as usize;
    let norm = |hist: &std::collections::VecDeque<f64>, scale: f64| -> Vec<f32> {
        hist.iter()
            .map(|&v| if scale > 0.0 { (v / scale) as f32 } else { 0.0 })
            .collect()
    };
    let rows = [
        ("↓ ", "download", CYAN, n.down, norm(&n.down_hist, n.down_scale)),
        ("↑ ", "upload", ROSE, n.up, norm(&n.up_hist, n.up_scale)),
    ];
    for (k, (arrow, label, color, rate, hist)) in rows.into_iter().enumerate() {
        let y = k as u16 * 3;
        let mut ln = Ln::new();
        ln.a(arrow, color)
            .a(&format!("{label:<20}"), DIM)
            .a(&format!("{:>11}", fmt::rate(rate)), HI);
        ln.put(buf, inner, 0, y);
        let (top, bot) = spark2(&hist, w, color);
        top.put(buf, inner, 0, y + 1);
        bot.put(buf, inner, 0, y + 2);
    }
    let mut ln = Ln::new();
    ln.a(
        &format!(
            "session ↓ {}  ↑ {}",
            fmt::total(n.session_down),
            fmt::total(n.session_up)
        ),
        DIM,
    );
    ln.put(buf, inner, 0, 6);
}

pub fn procs(buf: &mut Buffer, inner: Rect, app: &App) {
    let name_w = PROC_NAME_W.min((inner.width as usize).saturating_sub(20));
    let mut ln = Ln::new();
    ln.a(&format!("{:<7}{:<name_w$}{:>6}{:>7}", "pid", "name", "cpu", "mem"), DIM);
    ln.put(buf, inner, 0, 0);

    let rows = PROC_ROWS.min((inner.height as usize).saturating_sub(1));
    for (i, p) in app.procs.iter().take(rows).enumerate() {
        let me = p.name.eq_ignore_ascii_case("tide.exe");
        let name: String = p.name.chars().take(name_w.saturating_sub(1)).collect();
        let mut ln = Ln::new();
        ln.a(&format!("{:<7}", p.pid), DIM)
            .a(&format!("{name:<name_w$}"), if me { CYAN } else { TEXT })
            .a(&format!("{:>6.1}", p.cpu), AMBER)
            .a(&format!("{:>7}", fmt::short_size(p.mem)), TEXT);
        ln.put(buf, inner, 0, 1 + i as u16);
    }
}
