# tide technical docs

How tide works inside. For installing, running and configuring it, see the
[top-level README](../README.md).

| Doc | Covers |
| --- | --- |
| [architecture.md](architecture.md) | Threads, the frame loop, shared state, live vs demo mode, startup and shutdown |
| [audio.md](audio.md) | Loopback capture, device following, FFT and band mapping, gain, smoothing, VU |
| [system-stats.md](system-stats.md) | CPU, memory, GPU, network, processes and battery: sources, quirks, costs |
| [media.md](media.md) | Now playing, play/pause and seek through SMTC; master volume |
| [rendering.md](rendering.md) | Layout and degradation rules, drawing primitives, panels, the record, terminal output |
| [configuration.md](configuration.md) | Config and theme loading, validation, and how to add a setting or colour |
| [development.md](development.md) | Toolchain, tests and snapshots, diagnostic flags, profiling, releases |
| [cicd.md](cicd.md) | GitHub Actions: CI checks, the release workflow, cutting a release, dry runs |

## Glossary

- **Band**: one bar of the spectrum, covering a log-spaced frequency range.
- **Deck**: the panel with the track info, spinning record, progress and volume.
- **SMTC**: System Media Transport Controls, Windows' API for the "now playing" session
  shown in the volume flyout.
- **PDH**: Performance Data Helper, Windows' API for performance counters.
- **Loopback**: capturing what an output device plays rather than a microphone.
- **Snapshot (tests)**: a demo frame rendered as plain text and saved under `snapshots/`.
