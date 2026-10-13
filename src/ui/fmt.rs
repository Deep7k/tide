//! Number formatting used across panels.

use crate::config::{NetUnits, TempUnit};

const KIB: f64 = 1024.0;
const MIB: f64 = KIB * 1024.0;
const GIB: f64 = MIB * 1024.0;

/// `0.42` → `" 42%"` (right-aligned in 4 cols, plus a leading space).
pub fn pct4(v: f32) -> String {
    format!(" {:>4}", format!("{}%", (v * 100.0).round() as i32))
}

/// Seconds → `m:ss`.
pub fn mmss(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Network rate. Bytes: above 0.5 MB/s → `4.2 MB/s`, below → `312 KB/s`.
/// Bits use decimal units and Task Manager's steps: `5.7 Mbps`, `840 Kbps`.
pub fn rate(bytes_per_s: f64, units: NetUnits) -> String {
    match units {
        NetUnits::Bytes if bytes_per_s > 0.5 * MIB => format!("{:.1} MB/s", bytes_per_s / MIB),
        NetUnits::Bytes => format!("{:.0} KB/s", bytes_per_s / KIB),
        NetUnits::Bits => {
            let bits = bytes_per_s * 8.0;
            if bits >= 1e9 {
                format!("{:.1} Gbps", bits / 1e9)
            } else if bits >= 1e6 {
                format!("{:.1} Mbps", bits / 1e6)
            } else {
                format!("{:.0} Kbps", bits / 1e3)
            }
        }
    }
}

/// Temperature from °C: `44°C` or `111°F`.
pub fn temp(celsius: f32, unit: TempUnit) -> String {
    match unit {
        TempUnit::C => format!("{celsius:.0}°C"),
        TempUnit::F => format!("{:.0}°F", celsius * 9.0 / 5.0 + 32.0),
    }
}

/// `used/totalG`, e.g. `11.2/32G`.
pub fn used_total_g(used: u64, total: u64) -> String {
    format!("{:.1}/{:.0}G", used as f64 / GIB, total as f64 / GIB)
}

/// Compact size for the proc table: `412M`, `1.9G`.
pub fn short_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b < GIB {
        format!("{:.0}M", b / MIB)
    } else {
        format!("{:.1}G", b / GIB)
    }
}

/// Byte total: `184 MiB`, `2.1 GiB`.
pub fn total(bytes: u64) -> String {
    let b = bytes as f64;
    if b < GIB {
        format!("{:.0} MiB", b / MIB)
    } else {
        format!("{:.1} GiB", b / GIB)
    }
}

/// MiB with one decimal, for small sizes like the ring buffer.
pub fn mib1(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / MIB)
}

/// Frequency axis label: `70`, `440`, `1.1k`, `16k`.
pub fn freq_label(hz: f64) -> String {
    if hz < 1000.0 {
        format!("{}", ((hz / 10.0).round() * 10.0) as u32)
    } else if hz < 10_000.0 {
        format!("{:.1}k", hz / 1000.0)
    } else {
        format!("{:.0}k", hz / 1000.0)
    }
}

/// Sample rate for the header: `48 kHz`, `44.1 kHz`.
pub fn khz(rate: u32) -> String {
    if rate.is_multiple_of(1000) {
        format!("{} kHz", rate / 1000)
    } else {
        format!("{:.1} kHz", rate as f64 / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates() {
        assert_eq!(rate(312.0 * KIB, NetUnits::Bytes), "312 KB/s");
        assert_eq!(rate(4.2 * MIB, NetUnits::Bytes), "4.2 MB/s");
        assert_eq!(rate(712_500.0, NetUnits::Bits), "5.7 Mbps");
        assert_eq!(rate(105_000.0, NetUnits::Bits), "840 Kbps");
        assert_eq!(temp(44.0, TempUnit::C), "44°C");
        assert_eq!(temp(44.0, TempUnit::F), "111°F");
    }

    #[test]
    fn freq_labels_match_mock() {
        let labels: Vec<_> = (0..=35)
            .step_by(5)
            .map(|i| freq_label(30.0 * (16000.0f64 / 30.0).powf(i as f64 / 35.0)))
            .collect();
        assert_eq!(labels, ["30", "70", "180", "440", "1.1k", "2.7k", "6.5k", "16k"]);
    }

    #[test]
    fn sizes() {
        assert_eq!(pct4(0.37), "  37%");
        assert_eq!(mmss(238.0), "3:58");
        assert_eq!(short_size(412 * 1024 * 1024), "412M");
        assert_eq!(short_size((1.9 * GIB) as u64), "1.9G");
        assert_eq!(used_total_g((11.2 * GIB) as u64, 32 * 1024 * 1024 * 1024), "11.2/32G");
        assert_eq!(khz(44_100), "44.1 kHz");
    }
}
