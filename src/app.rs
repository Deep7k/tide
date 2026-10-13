//! Display state. Data sources (the demo generator now; audio, system stats
//! and SMTC later) write into it; the UI only reads it.

use std::collections::VecDeque;
use std::f32::consts::TAU;

/// Peak caps fall 0.012 per frame at the mock's ~15 fps.
const PEAK_DECAY_PER_S: f32 = 0.012 * 15.0;
/// Record spin speed while playing, rad/s.
pub const SPIN_SPEED: f32 = 3.5;
/// Time for the record to ease to a stop on pause.
const SPIN_EASE_S: f32 = 0.4;
/// CPU sparkline history length (samples, one per 500 ms).
pub const CPU_HIST: usize = 60;
/// Net sparkline history length.
pub const NET_HIST: usize = 34;

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
    pub vram_used: u64,
    pub vram_total: u64,
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
    /// Band count the layout wants; sources produce this many.
    pub band_count: usize,
    pub bands: Vec<f32>,
    pub peaks: Vec<f32>,
    pub vu: [Vu; 2],
    pub record_angle: f32,
    pub record_speed: f32,
    pub cpu: Cpu,
    pub mem: Mem,
    pub gpu: Gpu,
    pub net: Net,
    /// Sorted by CPU, descending.
    pub procs: Vec<Proc>,
    pub media: Option<Media>,
    /// Master volume, 0..1; None until known.
    pub volume: Option<f32>,
    pub audio: AudioInfo,
    pub fps: f32,
}

impl App {
    pub fn new() -> Self {
        Self {
            band_count: 36,
            bands: Vec::new(),
            peaks: Vec::new(),
            vu: Default::default(),
            record_angle: 0.0,
            record_speed: 0.0,
            cpu: Cpu::default(),
            mem: Mem::default(),
            gpu: Gpu::default(),
            net: Net::default(),
            procs: Vec::new(),
            media: None,
            volume: None,
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
            *p = v.max(*p - PEAK_DECAY_PER_S * dt);
        }
        self.bands = levels;
    }

    /// Per-frame animation that doesn't depend on the data source.
    pub fn tick(&mut self, dt: f32) {
        let playing = self.media.as_ref().is_some_and(|m| m.playing);
        let target = if playing { SPIN_SPEED } else { 0.0 };
        let step = SPIN_SPEED / SPIN_EASE_S * dt;
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
        let mut app = App::new();
        app.set_bands(vec![0.8], 0.0);
        app.set_bands(vec![0.1], 1.0);
        assert!((app.peaks[0] - (0.8 - PEAK_DECAY_PER_S)).abs() < 1e-6);
        app.set_bands(vec![0.9], 0.1);
        assert_eq!(app.peaks[0], 0.9);
    }

    #[test]
    fn spin_eases_to_stop_in_400ms() {
        let mut app = App::new();
        app.record_speed = SPIN_SPEED;
        app.media = None; // not playing
        for _ in 0..24 {
            app.tick(1.0 / 60.0); // 400 ms
        }
        assert!(app.record_speed.abs() < 1e-3);
    }
}
