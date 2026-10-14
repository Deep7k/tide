# Development

## Toolchain

- **Rust 1.99.0**, pinned in `rust-toolchain.toml`; MSVC host
  (`x86_64-pc-windows-msvc`), edition 2024.
- **MSVC Build Tools** (C++ workload with the Windows SDK) for the linker.
- **Project-local option:** this repo can keep Rust in `.toolchain/` (git-ignored)
  instead of a global install: `RUSTUP_HOME=.toolchain\rustup`,
  `CARGO_HOME=.toolchain\cargo`, installed with `--no-modify-path`. Each shell
  activates it with `. .\env.ps1`; `.vscode/settings.json` points VS Code
  terminals and rust-analyzer at it.

## Everyday commands

```powershell
. .\env.ps1                    # project-local toolchain only
cargo run -- --demo            # fake data, no capture
cargo run --release            # live
cargo test                     # unit + snapshot tests
cargo clippy --all-targets     # must print nothing
cargo fmt                      # rustfmt.toml: max_width = 120
```

Release builds use LTO with one codegen unit, so they take a few minutes from
clean; debug builds and tests don't.

## Tests

Unit tests sit next to the code they test (`#[cfg(test)] mod tests`). Notable ones:

| Area | Covers |
| --- | --- |
| `audio::dsp` | a full-scale sine reads ~0 dBFS in the right band; sub-bin bass bands; RMS; AGC tracking; tilt; smoothing factors |
| `config` | `--print-config` round-trips to the defaults; partial files; unknown keys; line numbers in errors; BOM; validation; `gpus` forms |
| `theme` | gradient endpoints; partial theme files; bad colours |
| `ui::layout` | full-size rectangles; 120×30 and 90×24 plans; tall terminals; no overflow at any size |
| `ui::deck` | the cached record matches the direct formula |
| `ui::stats` | two-GPU panel; net rate alignment when stretched |
| `ui::glyphs`, `ui::fmt` | bars, sparklines, braille bits; number formats |
| `main` (snapshots) | demo frames at 165×42 and 120×30 match `snapshots/` |

### Snapshots

`main::tests::check` renders a demo frame (3 s in, fixed 1/60 s steps, default
config and theme) to plain text and compares it with
`snapshots/demo_<W>x<H>.txt`. A layout or drawing change that alters output fails
the test with the new frame printed. After an intended change:

```powershell
$env:TIDE_BLESS = 1; cargo test snapshot; Remove-Item Env:TIDE_BLESS
```

Review the diff of `snapshots/` before committing. Snapshots are text only, so
colour changes don't show up there.

## Diagnostic flags

These are kept on purpose; most are left out of `--help`.

| Flag | Use it for |
| --- | --- |
| `--demo` | Comparing against the HTML mock, screenshots, working without audio |
| `--dump WxH` | Checking a layout at any size without a terminal of that size |
| `--probe-audio` | Tuning gain/tilt: 5 s of the live pipeline, p10/p50/p90 per band |
| `--probe-stats` | Checking readings against Task Manager; sampling cost per part |
| `--probe-media` | What SMTC reports right now, and how long it takes |
| `--bench WxH` | Frame build time per panel and bytes per frame, off-screen |
| `--profile FILE --quit-after N` | Real loop cost: per-phase µs/frame and process CPU |

To profile without disturbing your own terminal, run it minimised in a separate
console:

```powershell
$exe = (Resolve-Path .\target\release\tide.exe).Path
$p = Start-Process conhost.exe -ArgumentList "`"$exe`" --profile `"$env:TEMP\tide-prof.txt`" --quit-after 15" -WindowStyle Minimized -PassThru
$p.WaitForExit(); Get-Content $env:TEMP\tide-prof.txt
```

A freshly built `.exe` is scanned by Windows Defender on first launch, so the
first timing after a build is slow; measure again.

## CI and releases

GitHub Actions runs fmt, clippy and the tests on every push (`ci.yml`) and builds
and publishes a GitHub Release when a `vX.Y.Z` tag is pushed (`release.yml`). See
[cicd.md](cicd.md) for both workflows, how to cut a release, dry runs and
troubleshooting. `tide --version` prints the `Cargo.toml` version compiled in.

Release build details:

- The release profile strips symbols and uses `panic = "abort"`;
  `.cargo/config.toml` links the C runtime statically, so `tide.exe` (~2.1 MB)
  needs no Visual C++ redistributable.

## Repository layout

| Path | |
| --- | --- |
| `src/` | the app (see [architecture.md](architecture.md#module-map)) |
| `snapshots/` | saved demo frames for the snapshot tests |
| `assets/` | `tide.ico` (embedded into the `.exe` by `build.rs` with version info from `Cargo.toml`), the README icon and demo GIF. Originals live in git-ignored `assets/source/` |
| `docs/` | these docs |
| `mockups/` | the original design handoff (git-ignored, local only) |
| `.toolchain/`, `target/` | local toolchain and build output (git-ignored) |
