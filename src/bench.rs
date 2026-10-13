//! `--bench WxH`: renders demo frames off-screen and reports where the
//! per-frame time goes and how many bytes each frame sends to the terminal.

use std::cell::Cell;
use std::fmt::Write as _;
use std::io::{self, Write};
use std::rc::Rc;
use std::time::{Duration, Instant};

use ratatui::{
    Terminal, TerminalOptions, Viewport,
    backend::{CrosstermBackend, TestBackend},
    buffer::Buffer,
    layout::Rect,
};

use crate::app::App;
use crate::audio::dsp::Analyzer;
use crate::config::Config;
use crate::demo::Demo;
use crate::ui;
use crate::ui::chrome::content;

const FRAMES: usize = 600;

/// Counts bytes instead of writing them.
struct Sink {
    bytes: Rc<Cell<usize>>,
}

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes.set(self.bytes.get() + buf.len());
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Stat(Vec<Duration>);

impl Stat {
    fn line(&mut self, name: &str) -> String {
        self.0.sort();
        let n = self.0.len().max(1);
        let avg = self.0.iter().sum::<Duration>() / n as u32;
        let p99 = self.0.get(n * 99 / 100).copied().unwrap_or_default();
        format!("{name:<28} avg {:>7.1} µs   p99 {:>7.1} µs\n", us(avg), us(p99))
    }
}

fn us(d: Duration) -> f64 {
    d.as_secs_f64() * 1e6
}

pub fn run(cfg: &Config, w: u16, h: u16) -> String {
    let area = Rect::new(0, 0, w, h);
    let layout = ui::layout::compute(area, cfg.spectrum.max_bands, cfg.spectrum.rows_cap());
    let mut app = App::new(cfg.clone());
    let mut demo = Demo::new(&mut app);
    app.band_count = layout.bands as usize;

    let fixed = || TerminalOptions {
        viewport: Viewport::Fixed(area),
    };
    let bytes = Rc::new(Cell::new(0));
    let sink = Sink {
        bytes: Rc::clone(&bytes),
    };
    let mut real = Terminal::with_options(CrosstermBackend::new(sink), fixed()).expect("terminal");
    let mut test = Terminal::with_options(TestBackend::new(w, h), fixed()).expect("terminal");

    let (mut data, mut build, mut full) = (Stat(vec![]), Stat(vec![]), Stat(vec![]));
    let dt = 1.0 / 60.0;
    for _ in 0..FRAMES {
        let t = Instant::now();
        demo.update(&mut app, dt);
        app.tick(dt as f32);
        data.0.push(t.elapsed());

        // Frame building alone (TestBackend's diff is cheap and writes nothing).
        let t = Instant::now();
        test.draw(|f| ui::draw(f, &app, &layout)).expect("draw");
        build.0.push(t.elapsed());

        // Building plus ratatui's diff and escape-code encoding, as in the real loop.
        let t = Instant::now();
        real.draw(|f| ui::draw(f, &app, &layout)).expect("draw");
        full.0.push(t.elapsed());
    }
    let bytes = bytes.get();

    // Per-panel build cost, drawn straight into a buffer.
    let mut buf = Buffer::empty(area);
    let mut panels: Vec<(&str, Stat)> = Vec::new();
    let mut time = |name: &'static str, f: &mut dyn FnMut(&mut Buffer)| {
        let mut s = Stat(vec![]);
        for _ in 0..FRAMES / 4 {
            let t = Instant::now();
            f(&mut buf);
            s.0.push(t.elapsed());
        }
        panels.push((name, s));
    };
    time("  spectrum + vu", &mut |b| {
        ui::spectrum::draw(b, content(layout.spectrum), &app, layout.spec_rows)
    });
    if let Some(d) = layout.deck {
        time("  deck (record)", &mut |b| ui::deck::draw(b, content(d), &app));
    }
    if let Some(r) = layout.cpu {
        time("  cpu", &mut |b| ui::stats::cpu(b, content(r), &app));
    }
    if let Some(r) = layout.net {
        time("  net", &mut |b| ui::stats::net(b, content(r), &app));
    }
    if let Some(r) = layout.procs {
        time("  proc", &mut |b| ui::stats::procs(b, content(r), &app));
    }

    let mut fft = Stat(vec![]);
    let mut analyzer = Analyzer::new(cfg.audio.fft_size);
    let noise: Vec<f32> = (0..cfg.audio.fft_size)
        .map(|i| ((i * 7919 % 1000) as f32 / 500.0) - 1.0)
        .collect();
    for _ in 0..FRAMES {
        let t = Instant::now();
        let _ = analyzer.bands_db(&noise, 48_000, 36, cfg.audio.freq_min, cfg.audio.freq_max);
        fft.0.push(t.elapsed());
    }

    let mut out = format!("{FRAMES} demo frames at {w}x{h}\n");
    out += &data.line("demo data + tick");
    out += &build.line("build frame");
    for (name, s) in &mut panels {
        out += &s.line(name);
    }
    out += &full.line("build + diff + encode");
    out += &fft.line("fft + bands (live audio)");
    let _ = writeln!(
        out,
        "{:<28} avg {:>7.0} bytes/frame",
        "terminal output",
        bytes as f64 / FRAMES as f64
    );
    out
}
