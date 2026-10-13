# Architecture

tide is a single binary. One render thread owns the terminal and all display
state; data sources that can block run on their own threads and hand results
over through channels or a mutex-protected ring.

## Threads

```
                         ┌──────────────────────────── main thread ──────────────────────────────┐
 cpal audio callback ──► │ ring (Mutex) ──► Audio::update ─┐                                     │
 (WASAPI loopback,       │                                 │                                     │
  ~every 10 ms)          │ stats channel ─► Stats::update ─┼─► App ──► ui::draw ──► one console  │
 audio-device-watch ───► │ (DefaultDevice)                 │   (state)  (frame)      write/frame │
  (default device, 1 s)  │ media channel ─► MediaLink::update                                    │
 stats (500 ms / 1 s) ─► │                                 │                                     │
 media (250 ms) ◄──────► │ ◄── Cmd (space, ← →) ── keys ◄──┘                                     │
                         └────────────────────────────────────────────────────────────────────────┘
```

| Thread | Module | What it does | Talks to the main thread via |
| --- | --- | --- | --- |
| main | `main.rs` | Frame loop: layout, apply data, draw, input | — |
| cpal callback | `audio/capture.rs` | Appends captured samples to a stereo ring | `Arc<Mutex<Ring>>` |
| `audio-device-watch` | `audio/capture.rs` | Reads the default output device every second | `Arc<Mutex<Option<(Instant, DeviceId)>>>` |
| `stats` | `sys/mod.rs` | Samples CPU, memory, GPU, network, battery every `refresh_ms`; processes every `proc_refresh_ms` | `mpsc` channel of `Snapshot` |
| `media` | `media.rs` | Polls SMTC and master volume every `poll_ms`; runs play/pause and seek | two `mpsc` channels (`Cmd` in, `Snapshot` out) |

Background threads exit when the main thread drops their receiver/owner
(`send` fails, or the `Arc` strong count drops to 1).

## Shared state: `App`

`app::App` is the only thing the UI reads. Data sources write into it on the
main thread:

- `Audio::update` → `bands`, `peaks` (via `App::set_bands`), `vu`, `audio` info, `mem.ring_bytes`
- `Stats::update` → `cpu`, `mem`, `gpus`, `gpu_status`, `net`, `procs`, `battery`
- `MediaLink::update` → `media`, `volume`, `muted`, footer notices
- `App::tick` → per-frame animation that doesn't depend on a source: media position
  advances while playing, record spin eases toward its target speed

`App` also carries the loaded `Config` (`app.cfg`), so drawing code reads
settings like `proc_rows` or `temp_unit` from there.

## Modes

| Mode | Sources | Use |
| --- | --- | --- |
| live (default) | `Audio`, `Stats`, `MediaLink` | normal use |
| `--demo` | `demo::Demo` only | mock-faithful fake data; comparing against the HTML design; screenshots |

`--dump`, `--bench` and the snapshot tests always use the demo with a fixed
time step, so their output is deterministic.

## The frame loop

`main::event_loop`, once per frame:

1. **Timing.** `dt` since the last frame (capped at 250 ms); `app.fps` is an
   exponential moving average of `1/dt`.
2. **Layout.** `ui::layout::compute(area, max_bands, rows_cap)`. `area` is cached
   and only changes on `Event::Resize` (see [rendering.md](rendering.md#terminal-output)).
   The layout decides the band count, and `app.band_count` tells the sources how
   many bands to produce.
3. **Data.** Demo or audio/stats/media updates, then `app.tick(dt)`.
4. **Draw.** `term.draw(ui::draw)` inside synchronized-update markers; the whole
   frame goes to the console in a single write.
5. **Wait / input.** `event::poll` until the next frame is due. Resizes and handled
   keys end the wait early so the change shows immediately. When `app.is_idle()`
   (no sound, bars and meters settled, record stopped), the next frame is pushed
   out to the `idle_fps` interval.
6. If the loop falls behind, it resynchronises instead of trying to catch up.

`run` raises the Windows timer resolution to 1 ms (`timeBeginPeriod`) for the
process lifetime; the default ~15.6 ms tick can't pace 60 fps.

## Startup

1. `parse_args`. `--print-config` exits here.
2. `load_settings`: config file → command-line overrides → theme (installed into a
   global `OnceLock`). Warnings print to stderr before the alternate screen opens,
   so they're visible after exit.
3. Diagnostic modes (`--dump`, `--bench`, `--probe-*`) print and exit.
4. `run`: timer resolution, terminal setup (raw mode, alternate screen, hidden
   cursor, fixed viewport), panic hook, then the frame loop.

## Shutdown and panics

`q`, `Esc` or `Ctrl+C` return from the loop; `run` restores the terminal and
the timer resolution, and writes the `--profile` report if requested.

Release builds use `panic = "abort"`. The panic hook restores the terminal
(leaves the alternate screen, shows the cursor) before the default hook prints
the message, then the process exits. This also covers panics on background
threads, which with unwinding would have restored the terminal while the main
thread kept drawing.

## Module map

```
src/
├─ main.rs        CLI, terminal setup/restore, frame loop, key mapping, --dump, snapshot tests
├─ app.rs         App state shared between sources and UI
├─ config.rs      Config structs, defaults, loading, validation, --print-config text
├─ theme.rs       Theme colours, theme files, gradient helper
├─ demo.rs        --demo data generator (port of the HTML mock's build())
├─ media.rs       SMTC now playing + commands, master volume (media thread)
├─ bench.rs       --bench (off-screen render timing)
├─ profile.rs     --profile (live loop phase timing + process CPU)
├─ audio/
│  ├─ mod.rs      Audio: capture → analysis → App; --probe-audio
│  ├─ capture.rs  WASAPI loopback via cpal, stereo ring, device following
│  └─ dsp.rs      FFT, band mapping, AGC, tilt, smoothing, RMS
├─ sys/
│  ├─ mod.rs      stats thread, Snapshot, process list, battery, --probe-stats
│  ├─ pdh.rs      performance-counter wrapper
│  ├─ cpu.rs      per-core load, physical-core topology, clock
│  ├─ gpu.rs      adapters, load, memory, temperature, NVML watts
│  └─ net.rs      default-route interface byte counters
└─ ui/
   ├─ mod.rs      draw(): background, panels in order
   ├─ layout.rs   cell-grid layout and degradation rules
   ├─ chrome.rs   panel boxes, top panel, footer
   ├─ spectrum.rs bars, peak caps, frequency axis, VU
   ├─ deck.rs     track line, braille record (cached geometry), progress, volume
   ├─ stats.rs    cpu, mem, gpu, net, proc panels
   ├─ glyphs.rs   Ln line builder, bar, sparklines, pips, braille canvas
   └─ fmt.rs      number formatting
```
