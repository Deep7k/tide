# Audio

`src/audio/`: capture what the PC is playing, turn it into spectrum bands and VU
levels, and write them into `App` once per frame.

```
default output device
   │ WASAPI loopback (cpal, shared mode, 32-bit float)
   ▼
cpal callback ──► stereo ring (Mutex<VecDeque> ×2)        audio/capture.rs
                       │ latest fft_size frames, once per frame
                       ▼
                 mono = (L + R) / 2
                       │ Hann window → FFT (rustfft)         audio/dsp.rs
                       ▼
                 amplitude per bin ──► log-spaced bands (dBFS)
                       │ + treble tilt
                       ▼
                 AGC: dB → 0..1 over range_db
                       │ attack / release smoothing        audio/mod.rs
                       ▼
                 App::set_bands (peaks fall separately)
```

## Capture (`capture.rs`)

- **Loopback.** cpal opens an *input* stream on an *output* device, which turns on
  WASAPI loopback. Shared mode always mixes in 32-bit float; any other format is
  reported as an error.
- **Ring.** The callback appends left/right samples to two `VecDeque<f32>`s behind
  one `Mutex`, trimmed to `max(16384, 2 × fft_size)` frames. The render loop copies
  the newest `fft_size` frames out with `Capture::latest`.
- **Silence.** WASAPI delivers nothing at all while nothing plays. If the newest
  data is older than `silence_ms` (150 ms), `latest` returns zeros and `false`, and
  the bars decay smoothly instead of freezing.
- **Format.** The device's mix format decides the sample rate (often 44.1 or
  48 kHz); the header line and band mapping use the actual rate.

### Following the default device

The stream is opened on the **concrete** device that is currently the default
(`host.default_output_device()` → `id()` → `host.device_by_id()`), not on cpal's
virtual "follow the default" device. Re-opening that virtual device after a
switch failed inside a running process until restart; the concrete path doesn't.

Device changes are caught two ways:

1. **Watcher thread** (`audio-device-watch`) reads the default device every second
   (a 3 ms call, so it stays off the render thread). `maintain()` compares it with
   the open device and re-opens immediately if it moved. Readings taken before the
   stream was opened are ignored, so a stale reading can't bounce tide off the
   device it just opened.
2. **cpal errors.** `StreamInvalidated` / `DeviceNotAvailable` mark the stream
   invalid; it re-opens after 500 ms.

A failed open retries every 2 s, and the error shows once in the footer as
`audio capture: …`; the header source reads `no audio device`.

## Analysis (`dsp.rs`)

### FFT and band mapping

- Hann window over `fft_size` samples (4096 by default; 10.7 Hz bins at 44.1 kHz).
- Bin amplitude is `|X| × 4 / N`, so a full-scale sine reads 1.0 (0 dBFS): the Hann
  window's coherent gain is 0.5 and a real sine splits across ±f.
- Band `i` of `n` is centred at `fmin × (fmax/fmin)^(i/(n−1))` (30 Hz–16 kHz by
  default; see `ui::spectrum::band_hz`). Its edges are the geometric midpoints to its
  neighbours.
- A band takes the **maximum** amplitude of the bins inside it. Low bass bands can be
  narrower than one bin; those interpolate the amplitude at the band centre.

### Levels: tilt, AGC, smoothing

Raw band levels from real music sit far below the design's −60…0 dB range: a
5-second capture of Spotify had most bands at −50 to −70 dBFS and the treble
about 20 dB below the mids. Loopback also captures *after* the app's own volume
slider. Three steps make the bars usable:

| Step | Default | What it does |
| --- | --- | --- |
| Treble tilt | `treble_tilt_db_per_octave = 3` | Adds 3 dB per octave above 1 kHz (bass untouched) |
| AGC | `agc = true`, `range_db = 40` | A ceiling follows the loudest band (rises with τ = 0.3 s, falls with τ = 8 s, never below −60 dBFS); bars show `range_db` below ceiling + 3 dB headroom |
| Smoothing | `attack = 0.6`, `release = 0.15` | Per-frame factors at 60 fps, rescaled for the real frame time: `k = 1 − (1 − f)^(dt·60)` |

With `agc = false` the ceiling is fixed so the top of the bars is 0 dBFS.

`--probe-audio` runs this exact pipeline for 5 s and prints p10/p50/p90 of each
band's final 0..1 level, which is how the defaults were tuned.

### Peaks

`App::set_bands` keeps a peak per band: `peak = max(level, peak − peak_decay × dt)`
(0.18 full heights per second, the mock's 0.012 per frame at 15 fps).

### VU meters

Per channel: RMS over the last `vu_window_ms` (50 ms) → dBFS, smoothed with the
same attack/release, then mapped −60…0 dB → 0..1 (`dsp::normalize`). The meters use
an absolute scale (no AGC), so the dB readout is the real level.

## Cost

About 20–30 µs per frame for the FFT and band mapping, plus a few µs for the copy
out of the ring. The callback itself only appends samples.
