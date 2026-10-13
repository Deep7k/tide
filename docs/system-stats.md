# System stats

`src/sys/`: everything in the cpu, mem, gpu, net and proc panels plus the
battery in the top panel. One background thread (`stats`) owns all samplers.
Every `refresh_ms` (500 ms) it sends a `Snapshot` over a channel; every
`proc_refresh_ms` (1 s) the snapshot also carries the process list.
`Stats::update` drains the channel each frame, and `apply` copies the newest
values into `App` and pushes one sample onto each sparkline history.

## Sources at a glance

| Panel | Source | Module |
| --- | --- | --- |
| cpu | Performance counters (PDH) + CPU topology API | `cpu.rs`, `pdh.rs` |
| mem | sysinfo (`GlobalMemoryStatusEx`, `K32GetPerformanceInfo`) | `mod.rs` |
| gpu | DXGI (adapters), PDH GPU counters, graphics kernel (`D3DKMT`), NVIDIA NVML | `gpu.rs` |
| net | IP Helper (`GetBestInterface`, `GetIfEntry2`, `GetIfTable2`) | `net.rs` |
| proc | sysinfo process list | `mod.rs` |
| battery | `GetSystemPowerStatus` | `mod.rs` |

## CPU (`cpu.rs`)

- **Per-core load:** `\Processor Information(*)\% Processor Utility`, the counter
  Task Manager uses (frequency-aware, capped at 100%). `cpu_counter = "time"`
  switches to `% Processor Time` (plain busy time). If Utility isn't available
  (older Windows), Time is used.
- **Physical cores:** `GetLogicalProcessorInformationEx(RelationProcessorCore)`
  gives each core's logical processors as group masks. Instance names like `"0,5"`
  (group, index) map to logical index `group × 64 + index`; hyperthread siblings are
  averaged into one core. If topology can't be read, every logical processor
  counts as a core.
- **Clock:** `% Processor Performance × Processor Frequency` (base MHz) gives the
  effective clock, which drops well below base when idle.
- **Total:** `\Processor Information(_Total)\…` with the same counter choice.
- **Temperature:** not available on Windows without a kernel driver, so it's
  always `None` in live mode.

Rate counters need two collections before they have a value; the first sample
after start reads 0.

## Memory

- **RAM:** used = total − available.
- **Swap:** commit charge beyond physical RAM (`CommitTotal − PhysicalTotal`, out of
  `CommitLimit − PhysicalTotal`), as sysinfo computes it — effectively page-file use.
- **tide:** the process's working set, plus the audio ring size.

## GPU (`gpu.rs`)

**Adapters.** DXGI enumerates hardware adapters (software renderers skipped,
duplicates by LUID removed) in Windows' order, so `gpu1`/`gpu2` match Task
Manager's GPU 0/1. Adapters with under 512 MiB dedicated memory are treated as
integrated and report *shared* memory. `stats.gpus` filters by kind or picks
adapters by name (short name like `t1200` or a substring of the full name), in
the listed order.

| Reading | Source | Notes |
| --- | --- | --- |
| Load | `\GPU Engine(*)\Utilization Percentage` | Instances are per process and engine (`pid_…_luid_…_engtype_3D`). Sum per engine type for the adapter's LUID, take the busiest type — Task Manager's method |
| Memory | `\GPU Adapter Memory(*)\Dedicated Usage` or `Shared Usage` | Totals come from DXGI (`DedicatedVideoMemory` / `SharedSystemMemory`) |
| Temperature | `D3DKMTQueryAdapterInfo(KMTQAITYPE_ADAPTERPERFDATA)` | Tenths of °C; 0 means the driver doesn't report it (typical for Intel iGPUs) |
| Watts | NVIDIA NVML (`nvml.dll`, loaded on first use) | NVIDIA only |

**Battery-friendly NVML.** Querying NVML can wake a sleeping laptop dGPU (and
`nvidia-smi` does). None of the Windows sources do. So with `gpu_source = "auto"`
NVML is only called while the counters already show the NVIDIA GPU at ≥ 1% load;
`"nvml"` always calls it, `"counters"` never does, `"off"` disables GPU stats.

## Network (`net.rs`)

- **Interface:** `"auto"` asks Windows which interface routes to 8.8.8.8
  (`GetBestInterface`, no traffic is sent); otherwise the adapter whose alias
  matches `net_interface` (e.g. `"Wi-Fi"`) from `GetIfTable2`.
- **Rates:** `GetIfEntry2` byte counters, differenced between samples. A change of
  interface starts a new baseline instead of producing a spike.
- **Session totals:** sums of those deltas since tide started, across interfaces.
- **Label:** `wi-fi` / `ethernet` from the interface type, otherwise its alias;
  `offline` when there's no route.
- **Graphs:** 256 samples of history. The UI draws as many as fit and scales them
  to the visible peak, never below 64 KiB/s so idle traffic stays low.

## Processes

- sysinfo, refreshed with CPU and memory only (`ProcessRefreshKind`) every
  `proc_refresh_ms`.
- **CPU %:** sysinfo reports per-process usage scaled by the core count (up to
  n × 100%); tide divides by the logical CPU count for Task Manager's
  whole-machine percentage. sysinfo only computes this if its CPU list was
  loaded at creation, hence `System::new_with_specifics(… with_cpu …)`.
- **Memory:** working set.
- **Grouping:** with `proc_group_by_name`, processes with the same name are summed
  and the row shows the busiest member's pid.
- **Filtering and order:** pid 0 (System Idle Process) and `proc_hide` names are
  skipped; sorted by `proc_sort`, top 50 kept.
- **tide's own CPU and memory** come from the same list. Refreshing a single pid
  still enumerates every process in sysinfo, so it's only done with the full list.

Process CPU (busy time) and the total (Utility) are different measures, so the
rows can briefly add up to more than the total — Task Manager has the same quirk.

## Battery

`GetSystemPowerStatus`. No battery (flag 128), unknown status (255) or a
percentage over 100 → `None`, and the top panel hides the battery.

## Cost

A sample without the process list takes about 2–3 ms (GPU counters ~1–2 ms);
with it, 25–40 ms, once per second. That's roughly 0.2% of a 16-thread CPU, all on
the stats thread. `--probe-stats` prints the readings and these timings.
