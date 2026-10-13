//! ³ cpu · ⁴ mem · ⁵ gpu · ⁶ net · ⁷ proc

use ratatui::{buffer::Buffer, layout::Rect};

use super::{
    fmt,
    glyphs::{Ln, bar, spark, spark_rows},
};
use crate::app::App;
use crate::theme::th;

pub fn cpu(buf: &mut Buffer, inner: Rect, app: &App) {
    let c = &app.cpu;
    let w = inner.width as usize;

    let hist: Vec<f32> = c.hist.iter().copied().collect();
    let mut ln = Ln::new();
    ln.a("total ", th().dim);
    spark(&mut ln, &hist, w.saturating_sub(11), th().cpu).a(&fmt::pct4(c.total), th().hi);
    ln.put(buf, inner, 0, 0);

    let label_w = format!("c{} ", c.cores.len().saturating_sub(1)).len();
    for (i, &v) in c.cores.iter().enumerate() {
        let mut ln = Ln::new();
        ln.a(&format!("{:<label_w$}", format!("c{i}")), th().dim);
        bar(
            &mut ln,
            v,
            w.saturating_sub(label_w + 5),
            if v > app.cfg.stats.cpu_hot {
                th().cpu_hot
            } else {
                th().cpu
            },
        )
        .a(&fmt::pct4(v), th().text);
        ln.put(buf, inner, 0, 1 + i as u16);
    }

    let mut parts = vec![format!("{:.2} GHz", c.ghz)];
    parts.extend(c.temp_c.map(|t| fmt::temp(t, app.cfg.stats.temp_unit)));
    parts.push(format!("tide {:.1}%", c.self_use * 100.0));
    let mut ln = Ln::new();
    ln.a(&parts.join(" · "), th().dim);
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
        ln.a(label, th().dim);
        bar(&mut ln, ratio(used, total), w.saturating_sub(14), th().mem)
            .a(&format!(" {:>8}", fmt::used_total_g(used, total)), th().text);
        ln.put(buf, inner, 0, row as u16);
    }
    let mut ln = Ln::new();
    ln.a("tide ", th().dim).a(
        &format!("{} · ring buffer {}", fmt::total(m.self_bytes), fmt::mib1(m.ring_bytes)),
        th().text,
    );
    ln.put(buf, inner, 0, 2);
}

pub fn gpu(buf: &mut Buffer, inner: Rect, app: &App) {
    let w = inner.width as usize;
    let status = app
        .gpu_status
        .as_deref()
        .or(app.gpus.is_empty().then_some("no gpu found"));
    if let Some(status) = status {
        let mut ln = Ln::new();
        ln.a(status, th().dim);
        ln.put(buf, inner, 0, 0);
        return;
    }
    // Two rows per GPU (gpu1, gpu2, … in Windows' order), as many as fit.
    let shown = (inner.height as usize / 2).clamp(1, app.gpus.len());
    let label_w = format!("gpu{shown} ").len();
    for (i, g) in app.gpus[..shown].iter().enumerate() {
        let y = i as u16 * 2;
        let mut ln = Ln::new();
        ln.a(&format!("{:<label_w$}", format!("gpu{}", i + 1)), th().dim);
        bar(&mut ln, g.util, w.saturating_sub(label_w + 5), th().gpu).a(&fmt::pct4(g.util), th().text);
        ln.put(buf, inner, 0, y);

        let kind = if g.shared { "shared" } else { "vram" };
        let mut parts = vec![format!("{kind} {}", fmt::used_total_g(g.mem_used, g.mem_total))];
        parts.extend(g.temp_c.map(|t| fmt::temp(t, app.cfg.stats.temp_unit)));
        parts.extend(g.watts.map(|p| format!("{p:.0}W")));
        let mut ln = Ln::new();
        ln.a(&parts.join(" · "), th().dim);
        ln.put(buf, inner, 0, y + 1);
    }
}

pub fn net(buf: &mut Buffer, inner: Rect, app: &App) {
    let n = &app.net;
    let w = inner.width as usize;
    // Only the samples that fit are drawn; scale them to their own peak
    // (never below the floor, so idle traffic stays low).
    let norm = |hist: &std::collections::VecDeque<f64>, floor: f64| -> Vec<f32> {
        let visible: Vec<f64> = hist.iter().skip(hist.len().saturating_sub(w)).copied().collect();
        let scale = visible.iter().copied().fold(floor, f64::max);
        visible
            .iter()
            .map(|&v| if scale > 0.0 { (v / scale) as f32 } else { 0.0 })
            .collect()
    };
    let rows = [
        (
            "↓ ",
            "download",
            th().net_down,
            n.down,
            norm(&n.down_hist, n.down_scale),
        ),
        ("↑ ", "upload", th().net_up, n.up, norm(&n.up_hist, n.up_scale)),
    ];
    // Graphs grow to fill the box: header + graph for each direction, then the session line.
    let graph_rows = (inner.height.saturating_sub(3) / 2).max(1);
    for (k, (arrow, label, color, rate, hist)) in rows.into_iter().enumerate() {
        let y = k as u16 * (graph_rows + 1);
        // Rate right-aligned one column in from the edge (as in the design),
        // so it follows the panel when it stretches.
        let rate = fmt::rate(rate, app.cfg.stats.net_units);
        let pad = w.saturating_sub(2 + label.len() + rate.len() + 1).max(1);
        let mut ln = Ln::new();
        ln.a(arrow, color)
            .a(label, th().dim)
            .a(&" ".repeat(pad), th().dim)
            .a(&rate, th().hi);
        ln.put(buf, inner, 0, y);
        for (r, ln) in spark_rows(&hist, w, graph_rows as usize, color).iter().enumerate() {
            ln.put(buf, inner, 0, y + 1 + r as u16);
        }
    }
    let mut ln = Ln::new();
    ln.a(
        &format!(
            "session ↓ {}  ↑ {}",
            fmt::total(n.session_down),
            fmt::total(n.session_up)
        ),
        th().dim,
    );
    ln.put(buf, inner, 0, 2 * (graph_rows + 1));
}

pub fn procs(buf: &mut Buffer, inner: Rect, app: &App) {
    // pid, cpu and mem are fixed; the name column takes the rest of the box.
    let name_w = (inner.width as usize).saturating_sub(20);
    let mut ln = Ln::new();
    ln.a(
        &format!("{:<7}{:<name_w$}{:>6}{:>7}", "pid", "name", "cpu", "mem"),
        th().dim,
    );
    ln.put(buf, inner, 0, 0);

    let fit = (inner.height as usize).saturating_sub(1);
    let rows = match app.cfg.stats.proc_rows {
        0 => fit,
        n => n.min(fit),
    };
    for (i, p) in app.procs.iter().take(rows).enumerate() {
        let me = p.name.eq_ignore_ascii_case("tide.exe");
        let name: String = p.name.chars().take(name_w.saturating_sub(1)).collect();
        let mut ln = Ln::new();
        ln.a(&format!("{:<7}", p.pid), th().dim)
            .a(&format!("{name:<name_w$}"), if me { th().accent } else { th().text })
            .a(&format!("{:>6.1}", p.cpu), th().cpu)
            .a(&format!("{:>7}", fmt::short_size(p.mem)), th().text);
        ln.put(buf, inner, 0, 1 + i as u16);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{buffer::Buffer, layout::Rect};

    use crate::app::{App, Gpu};
    use crate::config::Config;

    fn rows(buf: &Buffer) -> Vec<String> {
        (0..buf.area.height)
            .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect()
    }

    #[test]
    fn net_rate_follows_a_stretched_panel() {
        let mut app = App::new(Config::default());
        app.net.down = 312.0 * 1024.0;
        for w in [34u16, 60] {
            let area = Rect::new(0, 0, w, 10);
            let mut buf = Buffer::empty(area);
            super::net(&mut buf, area, &app);
            let row = &rows(&buf)[0];
            // One column of breathing room on the right, as in the design.
            assert!(row.ends_with("312 KB/s "), "{w}: {row:?}");
        }
    }

    #[test]
    fn gpu_panel_shows_integrated_and_discrete() {
        const GIB: u64 = 1 << 30;
        let mut app = App::new(Config::default());
        app.gpus = vec![
            Gpu {
                util: 0.22,
                shared: true,
                mem_used: GIB,
                mem_total: 16 * GIB,
                ..Default::default()
            },
            Gpu {
                mem_total: 4 * GIB,
                temp_c: Some(44.0),
                ..Default::default()
            },
        ];
        let area = Rect::new(0, 0, 34, 4);
        let mut buf = Buffer::empty(area);
        super::gpu(&mut buf, area, &app);
        let r = rows(&buf);
        assert!(
            r[0].starts_with("gpu1 █████▎░") && r[0].ends_with("  22%"),
            "{:?}",
            r[0]
        );
        assert_eq!(r[1].trim_end(), "shared 1.0/16G");
        assert!(r[2].starts_with("gpu2 ░") && r[2].ends_with("   0%"), "{:?}", r[2]);
        assert_eq!(r[3].trim_end(), "vram 0.0/4G · 44°C");
    }
}
