//! Spectrum and VU analysis: Hann-windowed FFT mapped into log-spaced bands.

use std::sync::Arc;

use rustfft::{Fft, FftPlanner, num_complex::Complex};

use crate::ui::spectrum::band_hz;

pub const FFT_SIZE: usize = 4096;
/// Levels are normalised from this floor up to 0 dBFS.
pub const FLOOR_DB: f32 = -60.0;
/// Per-frame smoothing at 60 fps (scaled for other frame times).
const ATTACK: f32 = 0.6;
const RELEASE: f32 = 0.15;

pub struct Analyzer {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    buf: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    /// Amplitude per bin, where a full-scale sine reads 1.0.
    mags: Vec<f32>,
}

impl Analyzer {
    pub fn new() -> Self {
        let fft = FftPlanner::new().plan_fft_forward(FFT_SIZE);
        let window = (0..FFT_SIZE)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / FFT_SIZE as f32).cos())
            .collect();
        let scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
        Self {
            fft,
            window,
            buf: vec![Complex::default(); FFT_SIZE],
            scratch,
            mags: vec![0.0; FFT_SIZE / 2],
        }
    }

    /// Band levels in dBFS for `count` log-spaced bands. `mono` must hold FFT_SIZE samples.
    pub fn bands_db(&mut self, mono: &[f32], sample_rate: u32, count: usize) -> Vec<f32> {
        for ((b, &s), &w) in self.buf.iter_mut().zip(mono).zip(&self.window) {
            *b = Complex::new(s * w, 0.0);
        }
        self.fft.process_with_scratch(&mut self.buf, &mut self.scratch);
        // Hann coherent gain is 0.5, and a real sine splits across ±f: 2 / (N·0.5).
        let norm = 4.0 / FFT_SIZE as f32;
        for (m, c) in self.mags.iter_mut().zip(&self.buf) {
            *m = c.norm() * norm;
        }

        let df = sample_rate as f32 / FFT_SIZE as f32;
        (0..count)
            .map(|i| {
                let c = band_hz(i, count) as f32;
                // Band edges: geometric midpoints to the neighbouring centres.
                let ratio = (band_hz(1, count) / band_hz(0, count)).sqrt() as f32;
                let (lo, hi) = (c / ratio, c * ratio);
                let (k0, k1) = (
                    (lo / df).ceil() as usize,
                    ((hi / df).floor() as usize).min(self.mags.len() - 1),
                );
                let amp = if k0 <= k1 {
                    self.mags[k0..=k1].iter().copied().fold(0.0, f32::max)
                } else {
                    // Narrower than one bin (low bass): interpolate at the centre.
                    let k = c / df;
                    let (a, b) = (k.floor() as usize, (k.ceil() as usize).min(self.mags.len() - 1));
                    let t = k - k.floor();
                    self.mags[a] * (1.0 - t) + self.mags[b] * t
                };
                to_db(amp)
            })
            .collect()
    }
}

/// Display boost above 1 kHz so treble isn't dwarfed by mids (music falls
/// off roughly 3 dB/octave there).
const TILT_DB_PER_OCT: f32 = 3.0;
/// dB window shown below the AGC ceiling.
const RANGE_DB: f32 = 40.0;
const HEADROOM_DB: f32 = 3.0;
/// AGC ceiling follows peaks up quickly and drifts down slowly.
const AGC_RISE_S: f32 = 0.3;
const AGC_FALL_S: f32 = 8.0;
/// Keep the ceiling from chasing noise in very quiet passages.
const AGC_MIN_DB: f32 = -60.0;

pub fn tilt_db(hz: f32) -> f32 {
    TILT_DB_PER_OCT * (hz / 1000.0).log2().max(0.0)
}

/// Automatic gain: maps band dB into 0..1 relative to recent peaks, so
/// quiet tracks and low per-app volume still fill the display.
pub struct Agc {
    pub ceiling_db: f32,
}

impl Agc {
    pub fn new() -> Self {
        Self { ceiling_db: -30.0 }
    }

    pub fn levels(&mut self, db: &[f32], dt: f32) -> Vec<f32> {
        let peak = db.iter().copied().fold(f32::MIN, f32::max);
        let tau = if peak > self.ceiling_db { AGC_RISE_S } else { AGC_FALL_S };
        self.ceiling_db += (peak - self.ceiling_db) * (1.0 - (-dt / tau).exp());
        self.ceiling_db = self.ceiling_db.max(AGC_MIN_DB);
        let top = self.ceiling_db + HEADROOM_DB;
        db.iter()
            .map(|d| ((d - (top - RANGE_DB)) / RANGE_DB).clamp(0.0, 1.0))
            .collect()
    }
}

pub fn to_db(amp: f32) -> f32 {
    20.0 * amp.max(1e-6).log10()
}

/// RMS level in dBFS.
pub fn rms_db(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return to_db(0.0);
    }
    let ms = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    to_db(ms.sqrt())
}

/// dB → 0..1 between FLOOR_DB and 0 dBFS.
pub fn normalize(db: f32) -> f32 {
    ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
}

/// Moves `cur` toward `target` with fast attack and slow release.
pub fn smooth(cur: f32, target: f32, dt: f32) -> f32 {
    let k = if target > cur { ATTACK } else { RELEASE };
    let k = 1.0 - (1.0 - k).powf(dt * 60.0);
    cur + (target - cur) * k
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, amp: f32, rate: u32) -> Vec<f32> {
        (0..FFT_SIZE)
            .map(|i| amp * (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin())
            .collect()
    }

    #[test]
    fn full_scale_sine_reads_near_0_db_in_its_band() {
        let mut a = Analyzer::new();
        let hz = band_hz(20, 36) as f32; // ~1.1 kHz
        let db = a.bands_db(&sine(hz, 1.0, 48_000), 48_000, 36);
        let loudest = db.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        assert_eq!(loudest, 20);
        assert!(db[20] > -1.5 && db[20] < 0.5, "got {} dB", db[20]);
        assert!(db[10] < -60.0, "far band leaks: {} dB", db[10]);
    }

    #[test]
    fn low_bass_band_narrower_than_a_bin_still_reads() {
        let mut a = Analyzer::new();
        let db = a.bands_db(&sine(40.0, 0.5, 48_000), 48_000, 36);
        assert!(db[2] > -12.0, "40 Hz band: {} dB", db[2]);
    }

    #[test]
    fn rms_of_full_scale_sine_is_minus_3_db() {
        let db = rms_db(&sine(1000.0, 1.0, 48_000));
        assert!((db + 3.01).abs() < 0.1, "{db}");
    }

    #[test]
    fn agc_tracks_quiet_material() {
        let mut agc = Agc::new();
        let db = [-70.0, -45.0, -60.0];
        let mut lv = Vec::new();
        for _ in 0..3600 {
            lv = agc.levels(&db, 1.0 / 60.0); // 60 s, well past the 8 s fall time
        }
        assert!((agc.ceiling_db + 45.0).abs() < 1.0, "{}", agc.ceiling_db);
        assert!(lv[1] > 0.9 && lv[0] < 0.5, "{lv:?}");
    }

    #[test]
    fn tilt_only_lifts_treble() {
        assert_eq!(tilt_db(100.0), 0.0);
        assert!((tilt_db(8000.0) - 9.0).abs() < 1e-4);
    }

    #[test]
    fn smoothing_attacks_fast_releases_slow() {
        let up = smooth(0.0, 1.0, 1.0 / 60.0);
        let down = smooth(1.0, 0.0, 1.0 / 60.0);
        assert!((up - 0.6).abs() < 1e-5);
        assert!((down - 0.85).abs() < 1e-5);
    }
}
