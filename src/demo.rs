//! `--demo`: fake animated data, ported from the mock's `build()` so tide can
//! be compared side by side with the HTML reference.

use crate::app::{App, CPU_HIST, Cmd, Media, NET_HIST, Proc, push_hist};

const MIB: f64 = 1024.0 * 1024.0;
const GIB: f64 = 1024.0 * MIB;
const BPM: f64 = 118.0;
/// Demo net values are 0..1 in the mock; 1.0 = 8 MB/s.
const NET_FULL: f64 = 8.0 * MIB;
/// Stats and sparklines tick every 500 ms.
const SAMPLE_S: f64 = 0.5;

const PROCS: [(u32, &str, f32, f64); 7] = [
    (18244, "Spotify.exe", 6.2, 412.0 * MIB),
    (21108, "tide.exe", 3.1, 84.0 * MIB),
    (9932, "chrome.exe", 4.5, 1.9 * GIB),
    (14420, "Code.exe", 2.2, 880.0 * MIB),
    (7716, "Discord.exe", 1.4, 510.0 * MIB),
    (4120, "explorer.exe", 0.6, 190.0 * MIB),
    (30112, "WindowsTerminal.exe", 1.1, 140.0 * MIB),
];

fn cl(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

pub struct Demo {
    t: f64,
    since_sample: f64,
}

impl Demo {
    pub fn new(app: &mut App) -> Self {
        app.audio.source = "demo".into();
        app.audio.bits = "24-bit".into();
        app.media = Some(Media {
            title: "Lowlight Arcade".into(),
            artist: "Halcyon Drift".into(),
            album: "Signal Bloom".into(),
            pos: 102.0,
            dur: 238.0,
            playing: true,
        });
        app.volume = 0.7;

        // Same pre-filled history as the mock.
        app.cpu.hist = (0..CPU_HIST)
            .map(|i| (0.3 + 0.1 * (i as f64 / 4.0).sin()) as f32)
            .collect();
        app.net.down_hist = (0..NET_HIST)
            .map(|i| (0.3 + 0.2 * (i as f64 / 3.0).sin()) * NET_FULL)
            .collect();
        app.net.up_hist = (0..NET_HIST)
            .map(|i| (0.1 + 0.08 * (i as f64 / 2.0).sin()) * NET_FULL)
            .collect();
        app.net.down_scale = NET_FULL;
        app.net.up_scale = NET_FULL;
        app.net.kind = "wi-fi".into();
        app.net.session_down = (2.1 * GIB) as u64;
        app.net.session_up = (184.0 * MIB) as u64;

        app.mem.ram_total = (32.0 * GIB) as u64;
        app.mem.swap_total = (4.0 * GIB) as u64;
        app.mem.swap_used = (0.8 * GIB) as u64;
        app.mem.self_bytes = (84.0 * MIB) as u64;
        app.mem.ring_bytes = (2.1 * MIB) as u64;

        app.gpu.name = "rtx".into();
        app.gpu.vram_used = (3.1 * GIB) as u64;
        app.gpu.vram_total = (8.0 * GIB) as u64;
        app.gpu.temp_c = Some(54.0);
        app.gpu.watts = Some(31.0);

        app.cpu.ghz = 4.31;
        app.cpu.self_use = 0.031;

        Self {
            t: 0.0,
            since_sample: 0.0,
        }
    }

    pub fn update(&mut self, app: &mut App, dt: f64) {
        self.t += dt;
        let t = self.t;
        let ph = t * BPM / 60.0;
        let beat = (1.0 - ph.fract()).powi(3);
        let hat = (1.0 - (ph + 0.5).fract()).powi(6);
        let m = 0.45 + 0.15 * (t * 0.7).sin();

        // spectrum
        let n = app.band_count.max(2);
        let band = |x: f64, i: f64| {
            (0.08
                + 0.62 * (1.0 - x).powf(1.6) * (0.45 + 0.55 * beat)
                + 0.32 * (-(x - m).powi(2) / 0.012).exp() * (0.6 + 0.4 * (t * 3.0 + i).sin())
                + 0.12 * (0.5 + 0.5 * (t * 2.3 + i * 0.55).sin())
                + 0.06 * (t * 9.0 + i * 2.7).sin()
                + if x > 0.62 {
                    0.28 * hat * (0.6 + 0.4 * (i * 3.1).sin())
                } else {
                    0.0
                })
            .clamp(0.02, 0.97)
        };
        let levels = (0..n)
            .map(|i| band(i as f64 / (n - 1) as f64, i as f64) as f32)
            .collect();
        app.set_bands(levels, dt as f32);

        // VU
        let lv = [
            cl(0.55 + 0.35 * beat + 0.05 * (t * 7.0).sin()),
            cl(0.52 + 0.36 * beat + 0.05 * (t * 6.0 + 1.0).sin()),
        ];
        for (vu, v) in app.vu.iter_mut().zip(lv) {
            vu.level = v as f32;
            vu.db = (20.0 * v.log10()) as f32;
        }

        // cpu
        let cpu = cl(0.3 + 0.12 * (t * 0.4).sin() + 0.08 * beat + 0.04 * (t * 3.0).sin());
        app.cpu.total = cpu as f32;
        app.cpu.cores = (0..8)
            .map(|i| {
                let i = i as f64;
                (0.2 + 0.22 * (t * (0.3 + i * 0.07) + i).sin()
                    + 0.12 * (t * 1.7 + i * 2.0).sin()
                    + if i == 2.0 { 0.4 } else { 0.0 }
                    + 0.06 * beat)
                    .clamp(0.02, 0.98) as f32
            })
            .collect();
        app.cpu.temp_c = Some((58.0 + 6.0 * cpu) as f32);

        // mem, gpu
        app.mem.ram_used = ((0.35 + 0.01 * (t * 0.2).sin()) * 32.0 * GIB) as u64;
        app.gpu.util = cl(0.24 + 0.05 * (t * 0.9).sin() + 0.04 * beat) as f32;

        // net
        let down = cl(0.35 + 0.25 * (t * 0.5).sin() + 0.1 * (t * 2.1).sin());
        let up = cl(0.12 + 0.08 * (t * 0.8 + 1.0).sin());
        app.net.down = down * NET_FULL;
        app.net.up = up * NET_FULL;

        // proc
        let mut procs: Vec<Proc> = PROCS
            .iter()
            .enumerate()
            .map(|(i, &(pid, name, cpu, mem))| {
                let i = i as f64;
                let wobble = 1.2 * (t * (0.6 + i * 0.13) + i).sin() + if i == 0.0 { 1.5 * beat } else { 0.0 };
                Proc {
                    pid,
                    name: name.into(),
                    cpu: (cpu as f64 + wobble).clamp(0.1, 99.0) as f32,
                    mem: mem as u64,
                }
            })
            .collect();
        procs.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
        app.procs = procs;

        // media
        if let Some(m) = app.media.as_mut()
            && m.playing
        {
            m.pos = (m.pos + dt) % m.dur;
        }

        // histories
        self.since_sample += dt;
        if self.since_sample >= SAMPLE_S {
            self.since_sample -= SAMPLE_S;
            push_hist(&mut app.cpu.hist, app.cpu.total, CPU_HIST);
            push_hist(&mut app.net.down_hist, app.net.down, NET_HIST);
            push_hist(&mut app.net.up_hist, app.net.up, NET_HIST);
        }
    }

    pub fn apply(&mut self, app: &mut App, cmd: Cmd) {
        let Some(m) = app.media.as_mut() else { return };
        match cmd {
            Cmd::PlayPause => m.playing = !m.playing,
            Cmd::Seek(s) => m.pos = (m.pos + s).clamp(0.0, m.dur),
        }
    }
}
