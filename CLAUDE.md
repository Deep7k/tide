# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

tide is a Windows-only Rust TUI for Windows Terminal: a music visualizer (WASAPI
loopback → FFT spectrum + VU) and system monitor (cpu, mem, gpu, net, proc), with
now-playing info and controls via Windows SMTC. Deeper technical docs are in
`docs/` (start at `docs/README.md`); user docs are in `README.md`.

## Commands

The Rust toolchain is project-local in `.toolchain/` (git-ignored) and is **not**
on PATH. Shell state doesn't persist between tool calls, so activate it in every
PowerShell command:

```powershell
Set-ExecutionPolicy -Scope Process Bypass -Force; . C:\Users\admin\projects\code\winviz\env.ps1; Set-Location C:\Users\admin\projects\code\winviz
```

Then:

```powershell
cargo build                         # debug
cargo build --release               # LTO + 1 codegen unit: several minutes from clean
cargo test -q                       # all unit + snapshot tests
cargo test -q layout                # tests whose path contains "layout"
cargo test -q ui::layout::tests::full_size_matches_mock   # one test
cargo clippy -q --all-targets       # must print nothing
cargo fmt                           # rustfmt.toml: max_width = 120
cargo run -q -- --dump 165x42       # print a demo frame as text (no terminal needed)
cargo run -q -- --demo              # interactive, fake data
```

`target\release\tide.exe` is locked while the user has tide running; if a release
build fails with "Access is denied", verify with the debug build instead.

Diagnostics against the real machine (all print and exit): `--probe-audio`
(5 s of the live audio pipeline, per-band levels), `--probe-stats` (readings +
sampling cost), `--probe-media` (SMTC session, volume), `--bench WxH` (frame build
cost per panel, bytes/frame), `--profile FILE --quit-after N` (live loop phase
timings + process CPU; run it minimised via `Start-Process conhost.exe … -WindowStyle
Minimized`, see `docs/development.md`).

## Snapshot tests

`main::tests` render demo frames at **165×42** and **120×30** to plain text and
compare with `snapshots/demo_<W>x<H>.txt`. The demo uses fixed time steps and the
default config/theme, so output is deterministic. A refactor must not change
them. For an intended visual change, re-save and review the diff:

```powershell
$env:TIDE_BLESS='1'; cargo test -q snapshot; Remove-Item Env:TIDE_BLESS
```

Snapshots are text only: colour changes aren't covered.

## Architecture

- **One render thread owns the terminal and `app::App`.** Sources that can block
  run on their own threads and hand data over; the UI only reads `App`:
  - `audio/` — cpal callback fills a stereo ring (`Mutex`); `Audio::update` runs the
    FFT on the main thread each frame. An `audio-device-watch` thread polls the
    default output device; capture opens the *concrete* default device and
    re-opens when it changes (cpal's virtual default device fails to re-open in a
    running process).
  - `sys/` — `stats` thread samples PDH counters, DXGI/D3DKMT/NVML, IP Helper,
    sysinfo; sends `Snapshot`s over a channel.
  - `media.rs` — `media` thread polls SMTC + master volume, executes `Cmd`s
    (play/pause, seek) from the keys.
  - `demo.rs` — `--demo` replaces all three with a port of the HTML mock's generator.
- **Frame loop** (`main::event_loop`): layout → apply source data → `App::tick` →
  draw → wait for input or the next frame. The layout decides `band_count`, which
  tells the audio pipeline how many bands to produce.
- **Terminal output is the dominant cost**, so keep it lean: the viewport is
  `Viewport::Fixed` and only resized on `Event::Resize` (don't call `term.size()`
  per frame); `FrameWriter` holds ratatui's flush until a gate opens so each frame
  is one console write; frames redraw at `idle_fps` when `App::is_idle()`.
- **Layout** (`ui/layout.rs`) is a hand-computed cell grid with explicit
  degradation rules for small terminals and fill rules for large ones; the full
  design size is 165×42.
- **Drawing**: build rows with `glyphs::Ln` and `put` them. All glyphs must be one
  cell wide (`Ln::put` writes chars straight into cells). Colours only via
  `theme::th()`; number formatting via `ui/fmt.rs`. The deck's braille record uses
  a per-size cached `Geometry` — keep `deck::tests::cached_record_matches_direct_rules`
  passing if the record rules change.
- **Config** (`config.rs`): defaults ← `%APPDATA%\tide\config.toml` or `--config` ←
  CLI flags. Unknown keys warn (via `serde_ignored`), `validate()` reports all bad
  values. To add a setting: field + `Default` + `validate` check + the
  `default_toml()` template and its `format!` args (`printed_defaults_round_trip`
  fails otherwise). To add a theme colour: `Theme` field + default + `ThemeFile`
  `Option<Hex>` + the `set!` list. Checklists are in `docs/configuration.md`.

## Design decisions to preserve

- The visual spec was `mockups/design_handoff_tide_mockup2/` (git-ignored, local
  only), later revised by the user: full-width top panel (`user@host`, battery,
  fps), track info on the deck's first line, GPUs labelled `gpu1`/`gpu2`.
- CPU panel shows **physical** cores (hyperthread siblings averaged).
- `space` toggles media play/pause via SMTC (not "freeze rendering").
- GPU: load/memory/temperature come from Windows sources that don't wake a
  sleeping laptop dGPU; NVML (watts) is only queried while the NVIDIA GPU is busy
  when `gpu_source = "auto"`.
- VU meters stay `▌` (a half-height variant was tried and rejected).
- Hidden dev flags, the demo module, and every config/theme key are intentional,
  not dead code.

## Repository conventions

- Work on `master`; remote `origin` is `git@github.com:Deep7k/tide.git`.
- Commit messages must **not** include a `Co-Authored-By: Claude` trailer.
- Release tags are `vX.Y.Z` matching `version` in `Cargo.toml`; pushing one runs
  `.github/workflows/release.yml` (tag check, tests, build, zip, GitHub Release).
  `ci.yml` runs fmt, clippy `-D warnings` and tests on every push — keep them clean.
  Ask before pushing tags. Details in `docs/cicd.md`.
- `build.rs` embeds `assets/tide.ico` and version info (from `Cargo.toml`) into the
  `.exe` via `winresource` (needs the Windows SDK's `rc.exe`). README media is in
  `assets/`; large originals go in git-ignored `assets/source/`.
- The release profile uses `panic = "abort"` (the panic hook restores the
  terminal); `.cargo/config.toml` links the C runtime statically.
