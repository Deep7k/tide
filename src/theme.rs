//! Design tokens from the mockup2 handoff.

use ratatui::style::Color;

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const BG: Color = rgb(0x070B10);
pub const BORDER: Color = rgb(0x1C2733);
pub const TRACK: Color = rgb(0x16212C);
pub const DIM: Color = rgb(0x5A6B7D);
pub const TEXT: Color = rgb(0xCFDBE6);
pub const HI: Color = rgb(0xF2FBFF);

pub const CYAN: Color = rgb(0x22E0FF);
pub const AMBER: Color = rgb(0xFFB547);
pub const ROSE: Color = rgb(0xFF5EA8);
pub const VIOLET: Color = rgb(0xB56CFF);
pub const CORAL: Color = rgb(0xFF7A59);

/// Spectrum / VU / record gradient, dark blue → pale cyan.
pub const VIZ: [u32; 4] = [0x1F5BFF, 0x2A9BFF, 0x22E0FF, 0xA8FBFF];
pub const VIZ_RIM: Color = rgb(0x2A9BFF);
pub const VIZ_GROOVE: Color = rgb(0x1F5BFF);

/// Same as the mock's `grad()`: `f` is quantised to 1/20 steps, then
/// linearly interpolated between neighbouring stops.
pub fn grad(stops: &[u32], f: f32) -> Color {
    let f = (f.clamp(0.0, 1.0) * 20.0).round() / 20.0;
    let n = stops.len() - 1;
    let i = ((f * n as f32).floor() as usize).min(n - 1);
    mix(stops[i], stops[i + 1], f * n as f32 - i as f32)
}

fn mix(a: u32, b: u32, f: f32) -> Color {
    let ch = |hex: u32, shift: u32| ((hex >> shift) & 0xFF) as f32;
    let c = |shift| {
        let (x, y) = (ch(a, shift), ch(b, shift));
        (x + (y - x) * f).round() as u8
    };
    Color::Rgb(c(16), c(8), c(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grad_hits_endpoints_and_stops() {
        assert_eq!(grad(&VIZ, 0.0), rgb(0x1F5BFF));
        assert_eq!(grad(&VIZ, 1.0), rgb(0xA8FBFF));
        // 0.5 sits halfway between the 2nd and 3rd stops.
        assert_eq!(grad(&VIZ, 0.5), mix(0x2A9BFF, 0x22E0FF, 0.5));
    }
}
