//! `--profile FILE`: times each phase of the live render loop and the
//! process's total CPU, and writes a report when tide exits.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

#[derive(Clone, Copy)]
pub enum Phase {
    /// Terminal size query and layout.
    Layout,
    /// Audio, stats and media updates plus animation tick.
    Data,
    /// Frame build, diff, encode and the write to the console.
    Draw,
    /// Waiting for input or the next frame (not CPU).
    Wait,
}

const NAMES: [&str; 4] = ["size + layout", "data updates", "draw + write", "wait / input"];

pub struct Profile {
    start: Instant,
    cpu_start: Duration,
    frames: u64,
    phases: [Duration; 4],
    mark: Instant,
}

impl Profile {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            start: now,
            cpu_start: process_cpu(),
            frames: 0,
            phases: [Duration::ZERO; 4],
            mark: now,
        }
    }

    /// Ends the current phase.
    pub fn lap(&mut self, phase: Phase) {
        let now = Instant::now();
        self.phases[phase as usize] += now - self.mark;
        self.mark = now;
        if matches!(phase, Phase::Wait) {
            self.frames += 1;
        }
    }

    pub fn report(&self) -> String {
        let wall = self.start.elapsed();
        let cpu = process_cpu().saturating_sub(self.cpu_start);
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;
        let per_frame = |d: Duration| d.as_secs_f64() * 1e6 / self.frames.max(1) as f64;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{} frames in {:.1} s ({:.1} fps)",
            self.frames,
            wall.as_secs_f64(),
            self.frames as f64 / wall.as_secs_f64()
        );
        let _ = writeln!(
            out,
            "process cpu {:.2} s = {:.1}% of one core = {:.2}% of the machine",
            cpu.as_secs_f64(),
            cpu.as_secs_f64() / wall.as_secs_f64() * 100.0,
            cpu.as_secs_f64() / wall.as_secs_f64() / cores * 100.0
        );
        let busy: Duration = self.phases[..3].iter().sum();
        let _ = writeln!(
            out,
            "main thread busy {:.1}% of one core; other threads ≈ {:.1}%",
            busy.as_secs_f64() / wall.as_secs_f64() * 100.0,
            cpu.saturating_sub(busy).as_secs_f64() / wall.as_secs_f64() * 100.0
        );
        for (name, d) in NAMES.iter().zip(self.phases) {
            let _ = writeln!(out, "  {name:<16} {:>8.1} µs/frame", per_frame(d));
        }
        out
    }
}

/// User + kernel CPU time of this process so far.
fn process_cpu() -> Duration {
    let [mut c, mut e, mut k, mut u] = [FILETIME::default(); 4];
    // SAFETY: out-pointers to locals; the pseudo-handle needs no closing.
    if unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) }.is_err() {
        return Duration::ZERO;
    }
    let ticks = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
    Duration::from_nanos((ticks(k) + ticks(u)) * 100)
}
