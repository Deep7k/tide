//! `config.toml`: built-in defaults ← config file ← command-line flags.
//!
//! Every key is optional. Unknown keys are reported as warnings so typos
//! don't silently do nothing.

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default)]
pub struct Config {
    pub fps: u32,
    /// Built-in theme name, a theme name in `%APPDATA%\tide\themes\`, or a path to a theme file.
    pub theme: String,
    pub audio: AudioCfg,
    pub spectrum: SpectrumCfg,
    pub deck: DeckCfg,
    pub stats: StatsCfg,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default)]
pub struct AudioCfg {
    pub fft_size: usize,
    pub freq_min: f64,
    pub freq_max: f64,
    pub agc: bool,
    pub range_db: f32,
    pub treble_tilt_db_per_octave: f32,
    pub attack: f32,
    pub release: f32,
    pub silence_ms: u64,
    pub vu_window_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default)]
pub struct SpectrumCfg {
    pub max_bands: u16,
    pub peak_caps: bool,
    /// Peak cap fall rate, full height per second.
    pub peak_decay: f32,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default)]
pub struct DeckCfg {
    /// rad/s while playing.
    pub spin_speed: f32,
    pub spin_ease_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default)]
pub struct StatsCfg {
    pub refresh_ms: u64,
    pub proc_refresh_ms: u64,
    pub proc_rows: usize,
    pub proc_group_by_name: bool,
    pub cpu_hot: f32,
    pub gpu_source: GpuSource,
    pub net_interface: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuSource {
    Auto,
    Nvml,
    Counters,
    Off,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fps: 60,
            theme: "tide".into(),
            audio: AudioCfg::default(),
            spectrum: SpectrumCfg::default(),
            deck: DeckCfg::default(),
            stats: StatsCfg::default(),
        }
    }
}

impl Default for AudioCfg {
    fn default() -> Self {
        Self {
            fft_size: 4096,
            freq_min: 30.0,
            freq_max: 16_000.0,
            agc: true,
            range_db: 40.0,
            treble_tilt_db_per_octave: 3.0,
            attack: 0.6,
            release: 0.15,
            silence_ms: 150,
            vu_window_ms: 50,
        }
    }
}

impl Default for SpectrumCfg {
    fn default() -> Self {
        // The mock drops 0.012 per frame at ~15 fps.
        Self {
            max_bands: 36,
            peak_caps: true,
            peak_decay: 0.18,
        }
    }
}

impl Default for DeckCfg {
    fn default() -> Self {
        Self {
            spin_speed: 3.5,
            spin_ease_ms: 400,
        }
    }
}

impl Default for StatsCfg {
    fn default() -> Self {
        Self {
            refresh_ms: 500,
            proc_refresh_ms: 1000,
            proc_rows: 6,
            proc_group_by_name: true,
            cpu_hot: 0.82,
            gpu_source: GpuSource::Auto,
            net_interface: "auto".into(),
        }
    }
}

impl GpuSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Nvml => "nvml",
            Self::Counters => "counters",
            Self::Off => "off",
        }
    }
}

impl Config {
    fn validate(&self) -> Result<(), String> {
        let mut errs = Vec::new();
        let mut check = |ok: bool, msg: String| {
            if !ok {
                errs.push(msg);
            }
        };
        let a = &self.audio;
        check(
            (1..=240).contains(&self.fps),
            format!("fps must be 1–240 (got {})", self.fps),
        );
        check(
            a.fft_size.is_power_of_two() && (1024..=16_384).contains(&a.fft_size),
            format!(
                "audio.fft_size must be a power of two from 1024 to 16384 (got {})",
                a.fft_size
            ),
        );
        check(
            10.0 <= a.freq_min && a.freq_min < a.freq_max && a.freq_max <= 24_000.0,
            format!(
                "audio.freq_min/freq_max must satisfy 10 ≤ min < max ≤ 24000 (got {} / {})",
                a.freq_min, a.freq_max
            ),
        );
        check(
            (10.0..=120.0).contains(&a.range_db),
            format!("audio.range_db must be 10–120 (got {})", a.range_db),
        );
        check(
            (0.0..=12.0).contains(&a.treble_tilt_db_per_octave),
            format!(
                "audio.treble_tilt_db_per_octave must be 0–12 (got {})",
                a.treble_tilt_db_per_octave
            ),
        );
        for (name, v) in [("attack", a.attack), ("release", a.release)] {
            check(v > 0.0 && v <= 1.0, format!("audio.{name} must be in (0, 1] (got {v})"));
        }
        check(
            (20..=5000).contains(&a.silence_ms),
            format!("audio.silence_ms must be 20–5000 (got {})", a.silence_ms),
        );
        check(
            (5..=1000).contains(&a.vu_window_ms),
            format!("audio.vu_window_ms must be 5–1000 (got {})", a.vu_window_ms),
        );
        let s = &self.spectrum;
        check(
            (8..=64).contains(&s.max_bands),
            format!("spectrum.max_bands must be 8–64 (got {})", s.max_bands),
        );
        check(
            s.peak_decay > 0.0,
            format!("spectrum.peak_decay must be > 0 (got {})", s.peak_decay),
        );
        check(
            self.deck.spin_speed >= 0.0,
            format!("deck.spin_speed must be ≥ 0 (got {})", self.deck.spin_speed),
        );
        let st = &self.stats;
        check(
            (100..=10_000).contains(&st.refresh_ms),
            format!("stats.refresh_ms must be 100–10000 (got {})", st.refresh_ms),
        );
        check(
            (100..=10_000).contains(&st.proc_refresh_ms),
            format!("stats.proc_refresh_ms must be 100–10000 (got {})", st.proc_refresh_ms),
        );
        check(
            (1..=50).contains(&st.proc_rows),
            format!("stats.proc_rows must be 1–50 (got {})", st.proc_rows),
        );
        check(
            (0.0..=1.0).contains(&st.cpu_hot),
            format!("stats.cpu_hot must be 0–1 (got {})", st.cpu_hot),
        );
        if errs.is_empty() { Ok(()) } else { Err(errs.join("\n")) }
    }
}

/// `%APPDATA%\tide`, where the default config and user themes live.
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("tide"))
}

pub struct Loaded {
    pub config: Config,
    /// The file the config came from, if any.
    pub path: Option<PathBuf>,
    pub warnings: Vec<String>,
}

/// Loads `explicit`, else `%APPDATA%\tide\config.toml` if it exists, else defaults.
pub fn load(explicit: Option<&Path>) -> Result<Loaded, String> {
    let path = match explicit {
        Some(p) => Some(p.to_path_buf()),
        None => config_dir().map(|d| d.join("config.toml")).filter(|p| p.is_file()),
    };
    let Some(path) = path else {
        return Ok(Loaded {
            config: Config::default(),
            path: None,
            warnings: Vec::new(),
        });
    };
    let text = read_text(&path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    let (config, unknown) = parse::<Config>(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    config.validate().map_err(|e| format!("{}:\n{e}", path.display()))?;
    let warnings = unknown
        .into_iter()
        .map(|k| format!("{}: unknown key `{k}` (ignored)", path.display()))
        .collect();
    Ok(Loaded {
        config,
        path: Some(path),
        warnings,
    })
}

/// Reads a text file as UTF-8, or UTF-16 if it starts with a UTF-16 byte-order
/// mark (Windows PowerShell 5.1's `>` redirect writes UTF-16LE).
pub fn read_text(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    let utf16 = |b: &[u8], le: bool| {
        let units: Vec<u16> = b
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| {
                if le {
                    u16::from_le_bytes(c)
                } else {
                    u16::from_be_bytes(c)
                }
            })
            .collect();
        String::from_utf16(&units).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    };
    match bytes.as_slice() {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        _ => String::from_utf8(bytes).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
    }
}

/// Parses TOML into `T`, returning the dotted paths of keys `T` doesn't know.
pub fn parse<T: for<'de> Deserialize<'de>>(text: &str) -> Result<(T, Vec<String>), String> {
    // Some Windows editors save UTF-8 with a byte-order mark.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut unknown = Vec::new();
    let de = toml::Deserializer::parse(text).map_err(|e| e.to_string())?;
    let v = serde_ignored::deserialize(de, |p| unknown.push(p.to_string())).map_err(|e| e.to_string())?;
    unknown.sort();
    Ok((v, unknown))
}

/// `--print-config`: every key with its default value and a comment.
pub fn default_toml() -> String {
    let c = Config::default();
    let (a, s, d, st) = (&c.audio, &c.spectrum, &c.deck, &c.stats);
    format!(
        r#"# tide configuration. Every key is optional; delete what you don't change.
# Default location: %APPDATA%\tide\config.toml (or pass --config PATH).

# Render rate. --fps on the command line overrides this.
fps = {fps}

# "tide" (built in), a name from %APPDATA%\tide\themes\<name>.toml,
# or a path to a theme file (relative paths are relative to this file).
theme = "{theme}"

[audio]
# FFT window: power of two, 1024–16384. Bigger = finer bass, slower response.
fft_size = {fft}
# Spectrum frequency range in Hz (bands are log-spaced across it).
freq_min = {fmin:?}
freq_max = {fmax:?}
# Automatic gain: scale bars to recent peaks so quiet tracks still fill the
# display. Off = fixed scale with 0 dBFS at the top.
agc = {agc}
# dB range shown from the bottom to the top of the bars.
range_db = {range:?}
# Lift above 1 kHz so treble shows up next to the mids.
treble_tilt_db_per_octave = {tilt:?}
# Bar smoothing per frame at 60 fps, 0–1 (higher = snappier).
attack = {attack:?}
release = {release:?}
# Treat the audio as silent after this long without data.
silence_ms = {silence}
# VU meter RMS window.
vu_window_ms = {vu}

[spectrum]
# Band count at full width (each band is 3 columns). Narrow terminals show fewer.
max_bands = {bands}
peak_caps = {caps}
# How fast peak caps fall, in full bar heights per second.
peak_decay = {decay:?}

[deck]
# Record spin speed in rad/s while playing, and how long it takes to stop on pause.
spin_speed = {spin:?}
spin_ease_ms = {ease}

[stats]
# Refresh interval for cpu/mem/gpu/net and their sparklines.
refresh_ms = {refresh}
proc_refresh_ms = {proc_refresh}
proc_rows = {rows}
# One row per program (e.g. all chrome.exe processes summed) instead of per process.
proc_group_by_name = {group}
# Core bars turn hot above this load (0–1).
cpu_hot = {hot:?}
# "auto", "nvml" (NVIDIA driver: adds temperature and watts), "counters"
# (Windows GPU counters, any vendor), or "off".
gpu_source = "{gpu}"
# "auto" follows the default route, or an adapter name like "Wi-Fi".
net_interface = "{net}"
"#,
        fps = c.fps,
        theme = c.theme,
        fft = a.fft_size,
        fmin = a.freq_min,
        fmax = a.freq_max,
        agc = a.agc,
        range = a.range_db,
        tilt = a.treble_tilt_db_per_octave,
        attack = a.attack,
        release = a.release,
        silence = a.silence_ms,
        vu = a.vu_window_ms,
        bands = s.max_bands,
        caps = s.peak_caps,
        decay = s.peak_decay,
        spin = d.spin_speed,
        ease = d.spin_ease_ms,
        refresh = st.refresh_ms,
        proc_refresh = st.proc_refresh_ms,
        rows = st.proc_rows,
        group = st.proc_group_by_name,
        hot = st.cpu_hot,
        gpu = st.gpu_source.as_str(),
        net = st.net_interface,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printed_defaults_round_trip() {
        let (c, unknown) = parse::<Config>(&default_toml()).unwrap();
        assert_eq!(c, Config::default());
        assert!(unknown.is_empty(), "{unknown:?}");
        c.validate().unwrap();
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let (c, _) = parse::<Config>("fps = 30\n[audio]\nagc = false\n").unwrap();
        assert_eq!(c.fps, 30);
        assert!(!c.audio.agc);
        assert_eq!(c.audio.fft_size, 4096);
        assert_eq!(c.stats, StatsCfg::default());
    }

    #[test]
    fn unknown_keys_are_reported() {
        let (_, unknown) = parse::<Config>("fsp = 30\n[audio]\nfft = 1\n").unwrap();
        assert_eq!(unknown, ["audio.fft", "fsp"]);
    }

    #[test]
    fn byte_order_mark_is_ignored() {
        let (c, _) = parse::<Config>("\u{feff}fps = 30\n").unwrap();
        assert_eq!(c.fps, 30);
    }

    #[test]
    fn type_errors_mention_the_line() {
        let err = parse::<Config>("\n\nfps = \"fast\"\n").unwrap_err();
        assert!(err.contains("line 3"), "{err}");
    }

    #[test]
    fn invalid_values_are_rejected() {
        let (c, _) = parse::<Config>("[audio]\nfft_size = 3000\n").unwrap();
        assert!(c.validate().unwrap_err().contains("fft_size"));
    }
}
