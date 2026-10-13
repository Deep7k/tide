//! Live system stats. A background thread samples cpu/mem/gpu/net every
//! `refresh_ms` (processes every `proc_refresh_ms`) and sends snapshots to
//! the render loop, so slow system calls never stall a frame.

mod cpu;
mod gpu;
mod net;
mod pdh;

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use sysinfo::{CpuRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

use crate::app::{App, CPU_HIST, Gpu, NET_HIST, Proc, push_hist};
use crate::config::{GpuSelect, GpuSource, ProcSort, StatsCfg};
use cpu::{CpuSample, CpuSampler};
use gpu::{GpuSample, GpuSampler};
use net::{NetSample, NetSampler};

/// Sparklines never scale below this, so idle traffic stays near the floor.
const NET_MIN_SCALE: f64 = 64.0 * 1024.0;
/// Upper bound on processes kept per snapshot.
const MAX_PROCS: usize = 50;

struct Snapshot {
    cpu: Option<CpuSample>,
    self_use: f32,
    ram_used: u64,
    ram_total: u64,
    swap_used: u64,
    swap_total: u64,
    self_bytes: u64,
    gpu: Result<Vec<GpuSample>, &'static str>,
    net: NetSample,
    procs: Option<Vec<Proc>>,
}

pub struct Stats {
    rx: Receiver<Snapshot>,
}

impl Stats {
    pub fn spawn(cfg: &StatsCfg) -> Self {
        let (tx, rx) = mpsc::channel();
        let cfg = cfg.clone();
        thread::Builder::new()
            .name("stats".into())
            .spawn(move || run(cfg, tx))
            .expect("spawn stats thread");
        Self { rx }
    }

    /// Applies any snapshots that arrived since the last frame.
    pub fn update(&self, app: &mut App) {
        while let Ok(s) = self.rx.try_recv() {
            apply(app, s);
        }
    }
}

fn run(cfg: StatsCfg, tx: Sender<Snapshot>) {
    let mut sampler = Sampler::new(&cfg);
    let period = Duration::from_millis(cfg.refresh_ms);
    let proc_period = Duration::from_millis(cfg.proc_refresh_ms);
    let mut next = Instant::now() + period;
    let mut next_proc = Instant::now();
    loop {
        let now = Instant::now();
        let want_procs = now >= next_proc;
        if want_procs {
            next_proc = now + proc_period;
        }
        if tx.send(sampler.sample(want_procs)).is_err() {
            return; // UI is gone
        }
        thread::sleep(next.saturating_duration_since(Instant::now()));
        next += period;
        if next < Instant::now() {
            next = Instant::now() + period;
        }
    }
}

struct Sampler {
    sys: System,
    me: Pid,
    logical: f32,
    cfg: StatsCfg,
    cpu: Option<CpuSampler>,
    gpu: Result<GpuSampler, &'static str>,
    net: NetSampler,
    /// tide's own cpu (0..1) and memory, updated with the process list.
    me_stats: (f32, u64),
}

impl Sampler {
    fn new(cfg: &StatsCfg) -> Self {
        // sysinfo needs its CPU list to compute per-process usage.
        let sys = System::new_with_specifics(RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing()));
        let logical = sys.cpus().len().max(1) as f32;
        let gpu = match cfg.gpu_source {
            GpuSource::Off => Err("off in config"),
            source => GpuSampler::new(source, &cfg.gpus).ok_or(match cfg.gpus {
                GpuSelect::All => "no gpu found",
                _ => "no gpu matches stats.gpus",
            }),
        };
        Self {
            sys,
            me: Pid::from_u32(std::process::id()),
            logical,
            cfg: cfg.clone(),
            cpu: CpuSampler::new(cfg.cpu_counter),
            gpu,
            net: NetSampler::new(&cfg.net_interface),
            me_stats: (0.0, 0),
        }
    }

    fn sample(&mut self, all_procs: bool) -> Snapshot {
        let cpu = self.cpu.as_mut().map(CpuSampler::sample);

        self.sys.refresh_memory();
        // Process refresh enumerates every process even for one pid, so tide's
        // own numbers update with the full list.
        if all_procs {
            let kind = ProcessRefreshKind::nothing().with_cpu().with_memory();
            self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
            if let Some(p) = self.sys.process(self.me) {
                self.me_stats = (p.cpu_usage() / 100.0 / self.logical, p.memory());
            }
        }
        let (self_use, self_bytes) = self.me_stats;

        Snapshot {
            cpu,
            self_use,
            ram_used: self.sys.used_memory(),
            ram_total: self.sys.total_memory(),
            swap_used: self.sys.used_swap(),
            swap_total: self.sys.total_swap(),
            self_bytes,
            gpu: self.gpu.as_mut().map(GpuSampler::sample).map_err(|e| *e),
            net: self.net.sample(),
            procs: all_procs.then(|| self.procs()),
        }
    }

    /// Processes by CPU (percent of the whole machine, like Task Manager),
    /// optionally summed per program name.
    fn procs(&self) -> Vec<Proc> {
        let mut out: Vec<Proc> = Vec::new();
        // name → (index in `out`, cpu of the pid shown)
        let mut by_name: HashMap<String, (usize, f32)> = HashMap::new();
        for (pid, p) in self.sys.processes() {
            if pid.as_u32() == 0 {
                continue; // System Idle Process
            }
            let name = p.name().to_string_lossy().into_owned();
            if self.cfg.hides(&name) {
                continue;
            }
            let cpu = p.cpu_usage() / self.logical;
            let mem = p.memory();
            if self.cfg.proc_group_by_name
                && let Some((i, top)) = by_name.get_mut(&name)
            {
                let g = &mut out[*i];
                // Show the busiest member's pid.
                if cpu > *top {
                    g.pid = pid.as_u32();
                    *top = cpu;
                }
                g.cpu += cpu;
                g.mem += mem;
                continue;
            }
            by_name.insert(name.clone(), (out.len(), cpu));
            out.push(Proc {
                pid: pid.as_u32(),
                name,
                cpu,
                mem,
            });
        }
        match self.cfg.proc_sort {
            ProcSort::Cpu => out.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.mem.cmp(&a.mem))),
            ProcSort::Mem => out.sort_by(|a, b| b.mem.cmp(&a.mem).then(b.cpu.total_cmp(&a.cpu))),
        }
        out.truncate(MAX_PROCS);
        out
    }
}

fn apply(app: &mut App, s: Snapshot) {
    if let Some(c) = s.cpu {
        app.cpu.total = c.total;
        app.cpu.cores = c.cores;
        app.cpu.ghz = c.ghz;
    }
    app.cpu.temp_c = None; // not readable on Windows without a driver
    app.cpu.self_use = s.self_use;
    push_hist(&mut app.cpu.hist, app.cpu.total, CPU_HIST);

    app.mem.ram_used = s.ram_used;
    app.mem.ram_total = s.ram_total;
    app.mem.swap_used = s.swap_used;
    app.mem.swap_total = s.swap_total;
    app.mem.self_bytes = s.self_bytes;

    match s.gpu {
        Ok(gpus) => {
            app.gpu_status = None;
            app.gpus = gpus
                .into_iter()
                .map(|g| Gpu {
                    name: g.name,
                    util: g.util,
                    shared: g.shared,
                    mem_used: g.mem_used,
                    mem_total: g.mem_total,
                    temp_c: g.temp_c,
                    watts: g.watts,
                })
                .collect();
        }
        Err(why) => app.gpu_status = Some(why.into()),
    }

    let n = &mut app.net;
    n.kind = s.net.kind;
    n.down = s.net.down;
    n.up = s.net.up;
    n.session_down = s.net.session_down;
    n.session_up = s.net.session_up;
    push_hist(&mut n.down_hist, n.down, NET_HIST);
    push_hist(&mut n.up_hist, n.up, NET_HIST);
    // The UI scales each graph to its visible peak, but never below this.
    n.down_scale = NET_MIN_SCALE;
    n.up_scale = NET_MIN_SCALE;

    if let Some(procs) = s.procs {
        app.procs = procs;
    }
}

/// `--probe-stats`: samples a few times and prints the last readings plus
/// how long sampling takes.
pub fn probe(cfg: &StatsCfg) -> String {
    use std::fmt::Write;
    let t0 = Instant::now();
    let mut sampler = Sampler::new(cfg);
    let init = t0.elapsed();
    let mut times = Vec::new();
    let mut snap = None;
    for _ in 0..4 {
        thread::sleep(Duration::from_millis(cfg.refresh_ms));
        let t = Instant::now();
        snap = Some(sampler.sample(true));
        times.push(t.elapsed());
    }
    let t = Instant::now();
    let _ = sampler.sample(false);
    let light = t.elapsed();
    let t = Instant::now();
    let _ = sampler.gpu.as_mut().map(GpuSampler::sample);
    let gpu_t = t.elapsed();
    let t = Instant::now();
    let _ = sampler.cpu.as_mut().map(CpuSampler::sample);
    let cpu_t = t.elapsed();
    let s = snap.expect("sampled");
    let gib = |b: u64| b as f64 / (1u64 << 30) as f64;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "init {init:?} · sample with procs {times:?} · without {light:?} (gpu {gpu_t:?}, cpu {cpu_t:?})"
    );
    match &s.cpu {
        Some(c) => {
            let cores: Vec<String> = c.cores.iter().map(|v| format!("{:.0}", v * 100.0)).collect();
            let _ = writeln!(
                out,
                "cpu  total {:.1}% · {:.2} GHz · cores [{}]",
                c.total * 100.0,
                c.ghz,
                cores.join(" ")
            );
        }
        None => {
            let _ = writeln!(out, "cpu  counters unavailable");
        }
    }
    let _ = writeln!(
        out,
        "tide cpu {:.2}% · {:.0} MiB",
        s.self_use * 100.0,
        s.self_bytes as f64 / 1048576.0
    );
    let _ = writeln!(
        out,
        "mem  ram {:.1}/{:.1} GiB · swap {:.1}/{:.1} GiB",
        gib(s.ram_used),
        gib(s.ram_total),
        gib(s.swap_used),
        gib(s.swap_total)
    );
    match &s.gpu {
        Ok(gpus) => {
            for g in gpus {
                let _ = writeln!(
                    out,
                    "gpu  {} · {:.1}% · {} {:.2}/{:.1} GiB · temp {:?} · watts {:?}",
                    g.name,
                    g.util * 100.0,
                    if g.shared { "shared" } else { "vram" },
                    gib(g.mem_used),
                    gib(g.mem_total),
                    g.temp_c,
                    g.watts
                );
            }
        }
        Err(e) => {
            let _ = writeln!(out, "gpu  {e}");
        }
    }
    let _ = writeln!(
        out,
        "net  {} · ↓ {:.0} B/s ↑ {:.0} B/s · session ↓ {} ↑ {} bytes",
        s.net.kind, s.net.down, s.net.up, s.net.session_down, s.net.session_up
    );
    for p in s.procs.iter().flatten().take(8) {
        let _ = writeln!(
            out,
            "proc {:>6} {:<28} {:>5.1}% {:>6.0} MiB",
            p.pid,
            p.name,
            p.cpu,
            p.mem as f64 / 1048576.0
        );
    }
    out
}
