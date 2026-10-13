//! Every hardware GPU (e.g. a laptop's integrated + discrete pair): load and
//! memory from Windows' GPU performance counters, temperature from the
//! graphics kernel (what Task Manager shows), and power from NVIDIA's NVML
//! when allowed. None of the Windows sources wake a sleeping GPU.

use std::collections::HashMap;

use nvml_wrapper::Nvml;
use windows::Wdk::Graphics::Direct3D::{
    D3DKMT_ADAPTER_PERFDATA, D3DKMT_CLOSEADAPTER, D3DKMT_OPENADAPTERFROMLUID, D3DKMT_QUERYADAPTERINFO,
    D3DKMTCloseAdapter, D3DKMTOpenAdapterFromLuid, D3DKMTQueryAdapterInfo, KMTQAITYPE_ADAPTERPERFDATA,
};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1};

use super::pdh::{Counter, Query};
use crate::app::Gpu;
use crate::config::{GpuSelect, GpuSource};

const NVIDIA_VENDOR: u32 = 0x10DE;
/// Adapters with less dedicated memory than this are integrated GPUs that
/// work out of shared system memory.
const INTEGRATED_BELOW: u64 = 512 << 20;
/// In "auto", NVML is only queried while the GPU is this busy, so an idle
/// laptop dGPU can stay powered down.
const AWAKE_UTIL: f32 = 0.01;

/// One adapter's readings, plus its short name for `--probe-stats`.
pub struct GpuSample {
    pub name: String,
    pub stats: Gpu,
}

struct Adapter {
    name: String,
    /// As Windows reports it, e.g. "NVIDIA T1200 Laptop GPU".
    full_name: String,
    /// Matches PDH instance names, e.g. `luid_0x00000000_0x0000d1e2`.
    luid: String,
    shared: bool,
    mem_total: u64,
    nvidia: bool,
    /// Graphics-kernel handle for temperature queries.
    kmt: Option<u32>,
}

pub struct GpuSampler {
    source: GpuSource,
    adapters: Vec<Adapter>,
    query: Query,
    engines: Option<Counter>,
    dedicated: Option<Counter>,
    shared: Option<Counter>,
    /// Loaded on first use; None inside if NVML isn't available.
    nvml: Option<Option<Nvml>>,
}

impl GpuSampler {
    /// None when no hardware GPU was found.
    pub fn new(source: GpuSource, select: &GpuSelect) -> Option<Self> {
        let adapters = choose(adapters(), select);
        if adapters.is_empty() {
            return None;
        }
        let query = Query::new()?;
        let engines = query.add(r"\GPU Engine(*)\Utilization Percentage");
        let dedicated = query.add(r"\GPU Adapter Memory(*)\Dedicated Usage");
        let shared = query.add(r"\GPU Adapter Memory(*)\Shared Usage");
        query.collect();
        Some(Self {
            source,
            adapters,
            query,
            engines,
            dedicated,
            shared,
            nvml: None,
        })
    }

    pub fn sample(&mut self) -> Vec<GpuSample> {
        self.query.collect();
        let engines = self.engines.map(Counter::array).unwrap_or_default();
        let dedicated = self.dedicated.map(Counter::array).unwrap_or_default();
        let shared = self.shared.map(Counter::array).unwrap_or_default();

        let mut out = Vec::with_capacity(self.adapters.len());
        for a in &self.adapters {
            let util = engine_util(&engines, &a.luid);
            let mem_used = sum_for(if a.shared { &shared } else { &dedicated }, &a.luid);
            let temp_c = a.kmt.and_then(kmt_temperature);
            let want_nvml = a.nvidia
                && match self.source {
                    GpuSource::Nvml => true,
                    GpuSource::Auto => util >= AWAKE_UTIL,
                    GpuSource::Counters | GpuSource::Off => false,
                };
            let (nv_temp, watts) = if want_nvml {
                nvml_readings(&mut self.nvml)
            } else {
                (None, None)
            };
            out.push(GpuSample {
                name: a.name.clone(),
                stats: Gpu {
                    util,
                    shared: a.shared,
                    mem_used,
                    mem_total: a.mem_total,
                    temp_c: temp_c.or(nv_temp),
                    watts,
                },
            });
        }
        out
    }
}

/// NVIDIA temperature and watts, loading NVML on first use.
fn nvml_readings(nvml: &mut Option<Option<Nvml>>) -> (Option<f32>, Option<f32>) {
    use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
    let Some(nvml) = nvml.get_or_insert_with(|| Nvml::init().ok()) else {
        return (None, None);
    };
    let Ok(dev) = nvml.device_by_index(0) else {
        return (None, None);
    };
    let temp = dev.temperature(TemperatureSensor::Gpu).ok().map(|t| t as f32);
    let watts = dev.power_usage().ok().map(|mw| mw as f32 / 1000.0);
    (temp, watts)
}

impl Drop for GpuSampler {
    fn drop(&mut self) {
        for h in self.adapters.iter().filter_map(|a| a.kmt) {
            // SAFETY: handle came from D3DKMTOpenAdapterFromLuid and is closed once.
            let _ = unsafe { D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER { hAdapter: h }) };
        }
    }
}

/// Task Manager's figure: sum each engine type across processes, take the busiest type.
fn engine_util(instances: &[(String, f64)], luid: &str) -> f32 {
    let mut by_engine: HashMap<&str, f64> = HashMap::new();
    for (name, v) in instances {
        if name.to_ascii_lowercase().contains(luid)
            && let Some(i) = name.find("engtype_")
        {
            *by_engine.entry(&name[i..]).or_default() += v;
        }
    }
    (by_engine.values().copied().fold(0.0, f64::max) / 100.0).clamp(0.0, 1.0) as f32
}

fn sum_for(instances: &[(String, f64)], luid: &str) -> u64 {
    instances
        .iter()
        .filter(|(name, _)| name.to_ascii_lowercase().contains(luid))
        .map(|(_, v)| *v as u64)
        .sum()
}

/// GPU temperature from the graphics kernel, in °C.
fn kmt_temperature(adapter: u32) -> Option<f32> {
    let mut perf = D3DKMT_ADAPTER_PERFDATA::default();
    let mut q = D3DKMT_QUERYADAPTERINFO {
        hAdapter: adapter,
        Type: KMTQAITYPE_ADAPTERPERFDATA,
        pPrivateDriverData: (&mut perf as *mut D3DKMT_ADAPTER_PERFDATA).cast(),
        PrivateDriverDataSize: size_of::<D3DKMT_ADAPTER_PERFDATA>() as u32,
    };
    // SAFETY: q points at a correctly sized perf-data struct that outlives the call.
    let ok = unsafe { D3DKMTQueryAdapterInfo(&mut q) }.is_ok();
    // Reported in tenths of a degree; 0 means the driver doesn't provide it.
    (ok && perf.Temperature > 0).then(|| perf.Temperature as f32 / 10.0)
}

/// Hardware adapters in Windows' order (Task Manager's GPU 0, GPU 1, …).
fn adapters() -> Vec<Adapter> {
    // SAFETY: plain factory creation; COM interfaces are released on drop.
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return Vec::new();
    };
    let mut out: Vec<Adapter> = Vec::new();
    for i in 0.. {
        // SAFETY: EnumAdapters1 errors past the last adapter.
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(i) }) else {
            break;
        };
        // SAFETY: valid adapter interface.
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
            continue;
        }
        let luid = format!(
            "luid_0x{:08x}_0x{:08x}",
            desc.AdapterLuid.HighPart as u32, desc.AdapterLuid.LowPart
        );
        if out.iter().any(|a| a.luid == luid) {
            continue;
        }
        let full_name = super::from_wide(&desc.Description);
        let dedicated = desc.DedicatedVideoMemory as u64;
        let shared = dedicated < INTEGRATED_BELOW;
        let mut open = D3DKMT_OPENADAPTERFROMLUID {
            AdapterLuid: desc.AdapterLuid,
            hAdapter: 0,
        };
        // SAFETY: out-struct on the stack; the handle is closed in Drop.
        let kmt = unsafe { D3DKMTOpenAdapterFromLuid(&mut open) }
            .is_ok()
            .then_some(open.hAdapter);
        out.push(Adapter {
            name: short_name(&full_name),
            full_name,
            luid,
            shared,
            mem_total: if shared {
                desc.SharedSystemMemory as u64
            } else {
                dedicated
            },
            nvidia: desc.VendorId == NVIDIA_VENDOR,
            kmt,
        });
    }
    out
}

/// Applies the `gpus` setting: filter by kind, or pick and order by name.
fn choose(all: Vec<Adapter>, select: &GpuSelect) -> Vec<Adapter> {
    match select {
        GpuSelect::All => all,
        GpuSelect::Discrete => all.into_iter().filter(|a| !a.shared).collect(),
        GpuSelect::Integrated => all.into_iter().filter(|a| a.shared).collect(),
        GpuSelect::Names(names) => {
            let mut pool: Vec<Option<Adapter>> = all.into_iter().map(Some).collect();
            let mut out = Vec::new();
            for want in names {
                let want = want.to_ascii_lowercase();
                let hit = pool.iter_mut().find(|a| {
                    a.as_ref()
                        .is_some_and(|a| a.name == want || a.full_name.to_ascii_lowercase().contains(&want))
                });
                if let Some(a) = hit.and_then(Option::take) {
                    out.push(a);
                }
            }
            out
        }
    }
}

/// "NVIDIA T1200 Laptop GPU" → "t1200", "NVIDIA GeForce RTX 4070" → "rtx4070".
fn short_name(full: &str) -> String {
    const NOISE: [&str; 8] = [
        "nvidia", "geforce", "amd", "radeon", "intel", "laptop", "gpu", "graphics",
    ];
    let cleaned = full.to_ascii_lowercase().replace("(r)", " ").replace("(tm)", " ");
    let name: String = cleaned.split_whitespace().filter(|w| !NOISE.contains(w)).collect();
    if name.is_empty() {
        "gpu".into()
    } else {
        name.chars().take(8).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_names() {
        assert_eq!(short_name("NVIDIA T1200 Laptop GPU"), "t1200");
        assert_eq!(short_name("NVIDIA GeForce RTX 4070 Laptop GPU"), "rtx4070");
        assert_eq!(short_name("Intel(R) UHD Graphics"), "uhd");
        assert_eq!(short_name("Intel(R) Iris(R) Xe Graphics"), "irisxe");
        assert_eq!(short_name("AMD Radeon RX 7900 XTX"), "rx7900xt");
    }

    #[test]
    fn engine_util_takes_busiest_engine_type_for_the_adapter() {
        let rows = vec![
            (
                "pid_1_luid_0x00000000_0x0000AAAA_phys_0_eng_0_engtype_3D".to_string(),
                15.0,
            ),
            (
                "pid_2_luid_0x00000000_0x0000AAAA_phys_0_eng_0_engtype_3D".to_string(),
                7.0,
            ),
            (
                "pid_2_luid_0x00000000_0x0000AAAA_phys_0_eng_3_engtype_VideoDecode".to_string(),
                12.0,
            ),
            (
                "pid_3_luid_0x00000000_0x0000BBBB_phys_0_eng_0_engtype_3D".to_string(),
                90.0,
            ),
        ];
        let u = engine_util(&rows, "luid_0x00000000_0x0000aaaa");
        assert!((u - 0.22).abs() < 1e-6, "{u}");
    }
}
