//! Live audio: loopback capture → FFT bands and VU levels in `App`.

pub mod capture;
pub mod dsp;

use std::time::Duration;

use capture::Capture;
use dsp::{Agc, Analyzer};

use crate::app::App;
use crate::config::AudioCfg;
use crate::ui::spectrum::band_hz;

/// The ring holds at least this many frames, and always two FFT windows.
const MIN_RING_FRAMES: usize = 16_384;

pub struct Audio {
    cfg: AudioCfg,
    capture: Capture,
    analyzer: Analyzer,
    agc: Agc,
    left: Vec<f32>,
    right: Vec<f32>,
    mono: Vec<f32>,
    /// Smoothed band levels, 0..1.
    levels: Vec<f32>,
    /// Smoothed VU dB per channel.
    vu_db: [f32; 2],
}

impl Audio {
    pub fn new(cfg: &AudioCfg) -> Self {
        let ring = MIN_RING_FRAMES.max(cfg.fft_size * 2);
        Self {
            cfg: cfg.clone(),
            capture: Capture::new(ring, Duration::from_millis(cfg.silence_ms)),
            analyzer: Analyzer::new(cfg.fft_size),
            agc: Agc::new(cfg.agc, cfg.range_db),
            left: Vec::new(),
            right: Vec::new(),
            mono: Vec::new(),
            levels: Vec::new(),
            vu_db: [dsp::FLOOR_DB; 2],
        }
    }

    fn rate(&self) -> u32 {
        self.capture.format.as_ref().map_or(48_000, |f| f.sample_rate)
    }

    /// Pulls the latest samples and returns target band levels (0..1, before
    /// smoothing), or None while nothing is playing.
    fn analyze(&mut self, n: usize, dt: f32) -> Option<Vec<f32>> {
        self.capture.maintain();
        let size = self.analyzer.size();
        if !self.capture.latest(size, &mut self.left, &mut self.right) {
            return None;
        }
        self.mono.clear();
        self.mono
            .extend(self.left.iter().zip(&self.right).map(|(l, r)| 0.5 * (l + r)));
        let (fmin, fmax) = (self.cfg.freq_min, self.cfg.freq_max);
        let mut db = self.analyzer.bands_db(&self.mono, self.rate(), n, fmin, fmax);
        for (i, d) in db.iter_mut().enumerate() {
            *d += dsp::tilt_db(band_hz(i, n, fmin, fmax) as f32, self.cfg.treble_tilt_db_per_octave);
        }
        Some(self.agc.levels(&db, dt))
    }

    fn vu_target(&self, ch: usize) -> f32 {
        let samples = if ch == 0 { &self.left } else { &self.right };
        let window = self.rate() as u64 * self.cfg.vu_window_ms / 1000;
        let n = (window as usize).min(samples.len());
        dsp::rms_db(&samples[samples.len() - n..]).max(dsp::FLOOR_DB)
    }

    pub fn update(&mut self, app: &mut App, dt: f32) {
        let (attack, release) = (self.cfg.attack, self.cfg.release);
        let n = app.band_count.max(2);
        if self.levels.len() != n {
            self.levels = vec![0.0; n];
        }
        let targets = self.analyze(n, dt);
        for (i, lv) in self.levels.iter_mut().enumerate() {
            let t = targets.as_ref().map_or(0.0, |t| t[i]);
            *lv = dsp::smooth(*lv, t, dt, attack, release);
        }
        app.set_bands(self.levels.clone(), dt);

        for ch in 0..2 {
            let target = if targets.is_some() {
                self.vu_target(ch)
            } else {
                dsp::FLOOR_DB
            };
            self.vu_db[ch] = dsp::smooth(self.vu_db[ch], target, dt, attack, release);
            app.vu[ch].db = self.vu_db[ch];
            app.vu[ch].level = dsp::normalize(self.vu_db[ch]);
        }

        app.audio.source = match (&self.capture.format, &self.capture.last_error) {
            (Some(_), _) => "wasapi loopback".into(),
            (None, Some(_)) => "no audio device".into(),
            (None, None) => "opening audio".into(),
        };
        if let Some(f) = &self.capture.format {
            app.audio.sample_rate = f.sample_rate;
            app.audio.bits = format!("{}-bit float", f.sample_format.sample_size() * 8);
        }
        app.audio.fft = self.analyzer.size();
        app.mem.ring_bytes = self.capture.ring_bytes();
    }

    /// `--probe-audio`: runs the live pipeline for `secs` and prints stats on
    /// the VU level and the final 0..1 bar levels.
    pub fn probe(cfg: &AudioCfg, secs: f32) -> String {
        use std::fmt::Write;
        const N: usize = 36;
        const DT: f32 = 0.05;
        let mut audio = Self::new(cfg);
        let mut bars: Vec<Vec<f32>> = vec![Vec::new(); N];
        let mut vu = Vec::new();
        let frames = (secs / DT) as usize;
        for _ in 0..frames {
            std::thread::sleep(Duration::from_secs_f32(DT));
            if let Some(levels) = audio.analyze(N, DT) {
                for (b, l) in bars.iter_mut().zip(levels) {
                    b.push(l);
                }
                vu.push(audio.vu_target(0));
            }
        }

        let pct = |v: &[f32], p: usize| {
            let mut s = v.to_vec();
            s.sort_by(f32::total_cmp);
            s.get(s.len() * p / 100).copied().unwrap_or(f32::NAN)
        };
        let mut out = String::new();
        let f = audio.capture.format.as_ref();
        let _ = writeln!(
            out,
            "{} Hz · {} ch · error {:?} · fresh {}/{frames} · agc ceiling {:.1} dB",
            f.map_or(0, |f| f.sample_rate),
            f.map_or(0, |f| f.channels),
            audio.capture.last_error,
            vu.len(),
            audio.agc.ceiling_db
        );
        let _ = writeln!(
            out,
            "VU rms dB  p10 {:.1}  p50 {:.1}  p90 {:.1}",
            pct(&vu, 10),
            pct(&vu, 50),
            pct(&vu, 90)
        );
        let _ = writeln!(out, "bar level (0..1)   p10   p50   p90");
        for (i, b) in bars.iter().enumerate() {
            let _ = writeln!(
                out,
                "{i:>3} {:>6.0} Hz    {:.2}  {:.2}  {:.2}",
                band_hz(i, N, cfg.freq_min, cfg.freq_max),
                pct(b, 10),
                pct(b, 50),
                pct(b, 90)
            );
        }
        out
    }
}
