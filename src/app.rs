//! Display state. Data sources (the demo generator now; audio, system stats
//! and SMTC later) write into it; the UI only reads it.

use std::collections::VecDeque;
use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use crate::config::Config;

/// CPU sparkline history length (samples, one per 500 ms).
pub const CPU_HIST: usize = 60;
/// Net sparkline history length.
pub const NET_HIST: usize = 34;
/// How long footer notices stay up.
const NOTICE_FOR: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct Vu {
    /// Meter fill, 0..1.
    pub level: f32,
    pub db: f32,
}

#[derive(Default)]
pub struct Cpu {
    pub total: f32,
    pub hist: VecDeque<f32>,
    /// Physical cores, 0..1.
    pub cores: Vec<f32>,
    pub ghz: f32,
    pub temp_c: Option<f32>,
    /// tide's own CPU use, 0..1.
    pub self_use: f32,
}

#[derive(Default)]
pub struct Mem {
    pub ram_used: u64,
    pub ram_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub self_bytes: u64,
    pub ring_bytes: u64,
}

#[derive(Default)]
pub struct Gpu {
    pub name: String,
    pub util: f32,
    /// Integrated GPUs report shared memory; discrete ones dedicated VRAM.
    pub shared: bool,
    pub mem_used: u64,
    pub mem_total: u64,
    pub temp_c: Option<f32>,
    pub watts: Option<f32>,
}

#[derive(Default)]
pub struct Net {
    /// Interface kind shown in the title, e.g. `wi-fi`.
    pub kind: String,
    /// Current rates, bytes/s.
    pub down: f64,
    pub up: f64,
    pub down_hist: VecDeque<f64>,
    pub up_hist: VecDeque<f64>,
    /// Rate that fills the sparkline, bytes/s.
    pub down_scale: f64,
    pub up_scale: f64,
    pub session_down: u64,
    pub session_up: u64,
}

pub struct Proc {
    pub pid: u32,
    pub name: String,
    /// Percent of the whole machine, like Task Manager.
    pub cpu: f32,
    pub mem: u64,
}

pub struct Media {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub pos: f64,
    pub dur: f64,
    pub playing: bool,
}

/// User commands for whichever media source is active.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    PlayPause,
    /// Relative seek, seconds.
    Seek(f64),
}

pub struct AudioInfo {
    pub source: String,
    pub sample_rate: u32,
    pub bits: String,
    pub fft: usize,
}

pub struct App {
    pub cfg: Config,
    /// Band count the layout wants; sources produce this many.
    pub band_count: usize,
    pub bands: Vec<f32>,
    pub peaks: Vec<f32>,
    pub vu: [Vu; 2],
    pub record_angle: f32,
    pub record_speed: f32,
    pub cpu: Cpu,
    pub mem: Mem,
    /// Every hardware GPU, in Windows' order.
    pub gpus: Vec<Gpu>,
    /// Shown in the gpu panel instead of stats, e.g. "no gpu found".
    pub gpu_status: Option<String>,
    pub net: Net,
    /// Sorted by CPU, descending.
    pub procs: Vec<Proc>,
    pub media: Option<Media>,
    /// Master volume, 0..1; None until known.
    pub volume: Option<f32>,
    pub muted: bool,
    /// Footer message and when it was posted, e.g. "Spotify doesn't support seeking".
    notice: Option<(String, Instant)>,
    pub audio: AudioInfo,
    pub fps: f32,
}

impl App {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            band_count: 36,
            bands: Vec::new(),
            peaks: Vec::new(),
            vu: Default::default(),
            record_angle: 0.0,
            record_speed: 0.0,
            cpu: Cpu::default(),
            mem: Mem::default(),
            gpus: Vec::new(),
            gpu_status: None,
            net: Net::default(),
            procs: Vec::new(),
            media: None,
            volume: None,
            muted: false,
            notice: None,
            audio: AudioInfo {
                source: String::new(),
                sample_rate: 48_000,
                bits: String::new(),
                fft: 4096,
            },
            fps: 0.0,
        }
    }

    /// Stores new band levels and updates the falling peak caps.
    pub fn set_bands(&mut self, levels: Vec<f32>, dt: f32) {
        if self.peaks.len() != levels.len() {
            self.peaks = levels.clone();
        }
        for (p, &v) in self.peaks.iter_mut().zip(&levels) {
            *p = v.max(*p - self.cfg.spectrum.peak_decay * dt);
        }
        self.bands = levels;
    }

    /// Shows a short message in the footer.
    pub fn notify(&mut self, msg: String) {
        self.notice = Some((msg, Instant::now()));
    }

    /// The footer message, while it's still fresh.
    pub fn notice(&self) -> Option<&str> {
        self.notice
            .as_ref()
            .filter(|(_, at)| at.elapsed() < NOTICE_FOR)
            .map(|(m, _)| m.as_str())
    }

    /// Per-frame animation that doesn't depend on the data source.
    pub fn tick(&mut self, dt: f32) {
        // Position moves on between media updates.
        if let Some(m) = self.media.as_mut()
            && m.playing
        {
            m.pos = (m.pos + dt as f64).min(m.dur.max(0.0));
        }
        let playing = self.media.as_ref().is_some_and(|m| m.playing);
        let speed = self.cfg.deck.spin_speed;
        let ease_s = (self.cfg.deck.spin_ease_ms as f32 / 1000.0).max(0.001);
        let target = if playing { speed } else { 0.0 };
        let step = speed / ease_s * dt;
        self.record_speed += (target - self.record_speed).clamp(-step, step);
        self.record_angle = (self.record_angle + self.record_speed * dt).rem_euclid(TAU);
    }
}

/// Pushes onto a fixed-length history, dropping the oldest sample.
pub fn push_hist<T>(hist: &mut VecDeque<T>, v: T, len: usize) {
    hist.push_back(v);
    while hist.len() > len {
        hist.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_hold_then_fall() {
        let mut app = App::new(Config::default());
        app.set_bands(vec![0.8], 0.0);
        app.set_bands(vec![0.1], 1.0);
        assert!((app.peaks[0] - (0.8 - app.cfg.spectrum.peak_decay)).abs() < 1e-6);
        app.set_bands(vec![0.9], 0.1);
        assert_eq!(app.peaks[0], 0.9);
    }

    #[test]
    fn spin_eases_to_stop_in_400ms() {
        let mut app = App::new(Config::default());
        app.record_speed = app.cfg.deck.spin_speed;
        app.media = None; // not playing
        for _ in 0..24 {
            app.tick(1.0 / 60.0); // 400 ms
        }
        assert!(app.record_speed.abs() < 1e-3);
    }
}
