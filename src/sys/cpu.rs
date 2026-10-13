//! CPU load per physical core and effective clock, from performance counters.

use windows::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, RelationProcessorCore, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};

use super::pdh::{Counter, Query};
use crate::config::CpuCounter;

pub struct CpuSample {
    pub total: f32,
    /// Per physical core, 0..1.
    pub cores: Vec<f32>,
    pub ghz: f32,
}

pub struct CpuSampler {
    query: Query,
    per_cpu: Option<Counter>,
    total: Option<Counter>,
    perf: Option<Counter>,
    base_mhz: Option<Counter>,
    /// Logical processor indices (group·64 + bit) belonging to each physical core.
    cores: Vec<Vec<usize>>,
    logical: usize,
}

impl CpuSampler {
    pub fn new(counter: CpuCounter) -> Option<Self> {
        let query = Query::new()?;
        // "% Processor Utility" is what Task Manager shows; older Windows only has "% Processor Time".
        let pick = |name: &str| {
            let time = || query.add(&format!(r"\Processor Information({name})\% Processor Time"));
            match counter {
                CpuCounter::Utility => query
                    .add(&format!(r"\Processor Information({name})\% Processor Utility"))
                    .or_else(time),
                CpuCounter::Time => time(),
            }
        };
        let per_cpu = pick("*");
        let total = pick("_Total");
        let perf = query.add(r"\Processor Information(_Total)\% Processor Performance");
        let base_mhz = query.add(r"\Processor Information(_Total)\Processor Frequency");
        query.collect();
        let cores = physical_cores();
        let logical = cores.iter().map(Vec::len).sum::<usize>().max(1);
        Some(Self {
            query,
            per_cpu,
            total,
            perf,
            base_mhz,
            cores,
            logical,
        })
    }

    pub fn sample(&mut self) -> CpuSample {
        self.query.collect();
        let pct = |v: f64| (v / 100.0).clamp(0.0, 1.0) as f32;

        // Instances are "group,index" plus "_Total" rows; map to logical index.
        let mut logical = vec![0.0f32; self.logical];
        if let Some(c) = self.per_cpu {
            for (name, v) in c.array() {
                if let Some((g, i)) = name.split_once(',')
                    && let (Ok(g), Ok(i)) = (g.parse::<usize>(), i.parse::<usize>())
                    && let Some(slot) = logical.get_mut(g * 64 + i)
                {
                    *slot = pct(v);
                }
            }
        }
        let cores = self
            .cores
            .iter()
            .map(|lps| lps.iter().filter_map(|&i| logical.get(i)).sum::<f32>() / lps.len().max(1) as f32)
            .collect();

        let total = self.total.and_then(Counter::value).map_or(0.0, pct);
        let ghz = match (
            self.perf.and_then(Counter::value),
            self.base_mhz.and_then(Counter::value),
        ) {
            (Some(perf), Some(mhz)) => (perf / 100.0 * mhz / 1000.0) as f32,
            _ => 0.0,
        };
        CpuSample { total, cores, ghz }
    }
}

/// Logical processors grouped by physical core (hyperthread siblings together).
fn physical_cores() -> Vec<Vec<usize>> {
    let mut len = 0u32;
    // SAFETY: first call only reports the needed buffer length.
    let _ = unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut len) };
    if len == 0 {
        return fallback_cores();
    }
    // u64 storage keeps the variable-length records suitably aligned.
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    let base = buf.as_mut_ptr() as *mut u8;
    // SAFETY: buffer holds `len` bytes.
    if unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, Some(base.cast()), &mut len) }.is_err() {
        return fallback_cores();
    }
    let mut cores = Vec::new();
    let mut off = 0usize;
    while off < len as usize {
        // SAFETY: each record starts at `off` and reports its own Size.
        let info = unsafe { &*(base.add(off) as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX) };
        if info.Size == 0 {
            break;
        }
        if info.Relationship == RelationProcessorCore {
            // SAFETY: RelationProcessorCore records hold a PROCESSOR_RELATIONSHIP.
            let p = unsafe { &info.Anonymous.Processor };
            let masks = unsafe { std::slice::from_raw_parts(p.GroupMask.as_ptr(), p.GroupCount.max(1) as usize) };
            let lps: Vec<usize> = masks
                .iter()
                .flat_map(|m| {
                    (0..64)
                        .filter(move |b| m.Mask >> b & 1 == 1)
                        .map(move |b| m.Group as usize * 64 + b)
                })
                .collect();
            if !lps.is_empty() {
                cores.push(lps);
            }
        }
        off += info.Size as usize;
    }
    if cores.is_empty() { fallback_cores() } else { cores }
}

/// One "core" per logical processor if the topology can't be read.
fn fallback_cores() -> Vec<Vec<usize>> {
    let n = std::thread::available_parallelism().map_or(1, |n| n.get());
    (0..n).map(|i| vec![i]).collect()
}
