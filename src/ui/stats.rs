//! ³ cpu · ⁴ mem · ⁵ gpu · ⁶ net · ⁷ proc

use ratatui::{buffer::Buffer, layout::Rect};

use super::{
    fmt,
    glyphs::{Ln, bar, spark, spark2},
};
use crate::app::App;
use crate::theme::th;

const PROC_NAME_W: usize = 22;

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
    // Two rows per GPU, as many as fit; labels share a width so the bars line up.
    let shown = (inner.height as usize / 2).clamp(1, app.gpus.len());
    let label_w = app.gpus[..shown]
        .iter()
        .map(|g| g.name.chars().count() + 1)
        .max()
        .unwrap_or(0)
        .max(5);
    for (i, g) in app.gpus[..shown].iter().enumerate() {
        let y = i as u16 * 2;
        let mut ln = Ln::new();
        ln.a(&format!("{:<label_w$}", g.name), th().dim);
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
    let norm = |hist: &std::collections::VecDeque<f64>, scale: f64| -> Vec<f32> {
        hist.iter()
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
    for (k, (arrow, label, color, rate, hist)) in rows.into_iter().enumerate() {
        let y = k as u16 * 3;
        let mut ln = Ln::new();
        ln.a(arrow, color)
            .a(&format!("{label:<20}"), th().dim)
            .a(&format!("{:>11}", fmt::rate(rate, app.cfg.stats.net_units)), th().hi);
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
        th().dim,
    );
    ln.put(buf, inner, 0, 6);
}

pub fn procs(buf: &mut Buffer, inner: Rect, app: &App) {
    let name_w = PROC_NAME_W.min((inner.width as usize).saturating_sub(20));
    let mut ln = Ln::new();
    ln.a(
        &format!("{:<7}{:<name_w$}{:>6}{:>7}", "pid", "name", "cpu", "mem"),
        th().dim,
    );
    ln.put(buf, inner, 0, 0);

    let rows = app.cfg.stats.proc_rows.min((inner.height as usize).saturating_sub(1));
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
    fn gpu_panel_shows_integrated_and_discrete() {
        const GIB: u64 = 1 << 30;
        let mut app = App::new(Config::default());
        app.gpus = vec![
            Gpu {
                name: "uhd".into(),
                util: 0.22,
                shared: true,
                mem_used: GIB,
                mem_total: 16 * GIB,
                ..Default::default()
            },
            Gpu {
                name: "t1200".into(),
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
            r[0].starts_with("uhd   █████░") && r[0].ends_with("  22%"),
            "{:?}",
            r[0]
        );
        assert_eq!(r[1].trim_end(), "shared 1.0/16G");
        assert!(r[2].starts_with("t1200 ░") && r[2].ends_with("   0%"), "{:?}", r[2]);
        assert_eq!(r[3].trim_end(), "vram 0.0/4G · 44°C");
    }
}
