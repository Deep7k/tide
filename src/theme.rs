//! Colours. The built-in "tide" theme holds the mockup2 design tokens; a
//! theme file overrides any subset of them. The theme is chosen once at
//! startup (`theme = ...` in the config) and read through `th()`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ratatui::style::Color;
use serde::Deserialize;

use crate::config;

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub bg: Color,
    pub border: Color,
    /// Empty bar / meter cells.
    pub track: Color,
    /// Labels, axis, hints.
    pub dim: Color,
    pub text: Color,
    /// Emphasis: track title, peaks, playhead, rates.
    pub hi: Color,
    /// Spectrum/deck titles, ▶, progress, volume, tide's own process.
    pub accent: Color,
    /// Spectrum and VU gradient, bottom/left to top/right (2+ stops).
    pub viz: Vec<Color>,
    pub record_label: Color,
    pub record_rim: Color,
    pub record_groove: Color,
    pub cpu: Color,
    pub cpu_hot: Color,
    pub mem: Color,
    pub gpu: Color,
    pub net_down: Color,
    pub net_up: Color,
    /// Battery meter in the top panel (turns `cpu_hot` when low).
    pub battery: Color,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: rgb(0x070B10),
            border: rgb(0x1C2733),
            track: rgb(0x16212C),
            dim: rgb(0x5A6B7D),
            text: rgb(0xCFDBE6),
            hi: rgb(0xF2FBFF),
            accent: rgb(0x22E0FF),
            viz: vec![rgb(0x1F5BFF), rgb(0x2A9BFF), rgb(0x22E0FF), rgb(0xA8FBFF)],
            record_label: rgb(0x22E0FF),
            record_rim: rgb(0x2A9BFF),
            record_groove: rgb(0x1F5BFF),
            cpu: rgb(0xFFB547),
            cpu_hot: rgb(0xFF5EA8),
            mem: rgb(0xB56CFF),
            gpu: rgb(0xFF7A59),
            net_down: rgb(0x22E0FF),
            net_up: rgb(0xFF5EA8),
            battery: rgb(0x7CF2C4),
        }
    }
}

static THEME: OnceLock<Theme> = OnceLock::new();

/// The active theme (the built-in one until `install` is called).
pub fn th() -> &'static Theme {
    THEME.get_or_init(Theme::default)
}

/// Sets the theme for the rest of the run. Must happen before the first draw.
pub fn install(theme: Theme) {
    let _ = THEME.set(theme);
}

/// Linear gradient through `stops`; `f` is quantised to 1/20 steps like the mock.
pub fn grad(stops: &[Color], f: f32) -> Color {
    if stops.len() < 2 {
        return stops.first().copied().unwrap_or(Color::Reset);
    }
    let f = (f.clamp(0.0, 1.0) * 20.0).round() / 20.0;
    let n = stops.len() - 1;
    let i = ((f * n as f32).floor() as usize).min(n - 1);
    mix(stops[i], stops[i + 1], f * n as f32 - i as f32)
}

fn mix(a: Color, b: Color, f: f32) -> Color {
    let (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) = (a, b) else {
        return if f < 0.5 { a } else { b };
    };
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f).round() as u8;
    Color::Rgb(m(ar, br), m(ag, bg), m(ab, bb))
}

/// A theme file: any subset of the theme's colours as "#RRGGBB".
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ThemeFile {
    bg: Option<Hex>,
    border: Option<Hex>,
    track: Option<Hex>,
    dim: Option<Hex>,
    text: Option<Hex>,
    hi: Option<Hex>,
    accent: Option<Hex>,
    viz: Option<Vec<Hex>>,
    record_label: Option<Hex>,
    record_rim: Option<Hex>,
    record_groove: Option<Hex>,
    cpu: Option<Hex>,
    cpu_hot: Option<Hex>,
    mem: Option<Hex>,
    gpu: Option<Hex>,
    net_down: Option<Hex>,
    net_up: Option<Hex>,
    battery: Option<Hex>,
}

#[derive(Clone, Copy, Debug)]
struct Hex(Color);

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        parse_hex(&s)
            .map(Hex)
            .ok_or_else(|| serde::de::Error::custom(format!("expected \"#RRGGBB\", got {s:?}")))
    }
}

fn parse_hex(s: &str) -> Option<Color> {
    let h = s.strip_prefix('#').filter(|h| h.len() == 6)?;
    u32::from_str_radix(h, 16).ok().map(rgb)
}

impl ThemeFile {
    fn apply(self, t: &mut Theme) -> Result<(), String> {
        macro_rules! set {
            ($($f:ident),*) => { $( if let Some(Hex(c)) = self.$f { t.$f = c; } )* };
        }
        set!(
            bg,
            border,
            track,
            dim,
            text,
            hi,
            accent,
            record_label,
            record_rim,
            record_groove
        );
        set!(cpu, cpu_hot, mem, gpu, net_down, net_up, battery);
        if let Some(viz) = self.viz {
            if viz.len() < 2 {
                return Err("viz needs at least 2 colours".into());
            }
            t.viz = viz.into_iter().map(|Hex(c)| c).collect();
        }
        Ok(())
    }
}

/// Resolves the config's `theme` value: "tide" (built in), a path to a
/// `.toml` file (relative to `base_dir`), or a name in `%APPDATA%\tide\themes\`.
/// Returns the theme and any unknown-key warnings.
pub fn load(name: &str, base_dir: Option<&Path>) -> Result<(Theme, Vec<String>), String> {
    if name == "tide" {
        return Ok((Theme::default(), Vec::new()));
    }
    let path = if name.ends_with(".toml") || name.contains(['/', '\\']) {
        let p = PathBuf::from(name);
        match base_dir {
            Some(dir) if p.is_relative() => dir.join(p),
            _ => p,
        }
    } else {
        let dir = config::config_dir()
            .ok_or("theme: %APPDATA% is not set")?
            .join("themes");
        dir.join(format!("{name}.toml"))
    };
    let text = config::read_text(&path).map_err(|e| format!("theme {name:?}: can't read {}: {e}", path.display()))?;
    let (file, unknown) = config::parse::<ThemeFile>(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut theme = Theme::default();
    file.apply(&mut theme).map_err(|e| format!("{}: {e}", path.display()))?;
    let warnings = unknown
        .into_iter()
        .map(|k| format!("{}: unknown theme key `{k}` (ignored)", path.display()))
        .collect();
    Ok((theme, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grad_hits_endpoints_and_stops() {
        let viz = Theme::default().viz;
        assert_eq!(grad(&viz, 0.0), rgb(0x1F5BFF));
        assert_eq!(grad(&viz, 1.0), rgb(0xA8FBFF));
        // 0.5 sits halfway between the 2nd and 3rd stops.
        assert_eq!(grad(&viz, 0.5), mix(rgb(0x2A9BFF), rgb(0x22E0FF), 0.5));
    }

    #[test]
    fn partial_theme_file_overrides_only_its_keys() {
        let (file, unknown) =
            config::parse::<ThemeFile>("accent = \"#FF0000\"\nviz = [\"#000000\", \"#FFFFFF\"]\nbogus = 1\n").unwrap();
        let mut t = Theme::default();
        file.apply(&mut t).unwrap();
        assert_eq!(t.accent, rgb(0xFF0000));
        assert_eq!(t.viz, vec![rgb(0), rgb(0xFFFFFF)]);
        assert_eq!(t.bg, Theme::default().bg);
        assert_eq!(unknown, ["bogus"]);
    }

    #[test]
    fn bad_colour_is_an_error() {
        assert!(
            config::parse::<ThemeFile>("bg = \"teal\"\n")
                .unwrap_err()
                .contains("#RRGGBB")
        );
    }
}
