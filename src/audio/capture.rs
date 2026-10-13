//! WASAPI loopback capture of the default output device via cpal.
//!
//! The capture callback appends to a small stereo ring; the render loop
//! copies the latest frames out once per frame.
//!
//! The stream is opened on the concrete device that is currently the
//! default, and every second we check whether Windows' default has moved
//! (earphones plugged in, Bluetooth connected, …) and re-open if so.
//! Re-opening through cpal's "follow the default" device after a switch
//! fails inside a running process, so we don't rely on it.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, SampleFormat};

const RETRY_FAST: Duration = Duration::from_millis(500);
const RETRY_SLOW: Duration = Duration::from_secs(2);
/// How often to check whether the default output device changed.
const DEVICE_CHECK: Duration = Duration::from_secs(1);

/// Latest default output device reading and when it was taken (None until the first).
type DefaultDevice = Arc<Mutex<Option<(Instant, Option<cpal::DeviceId>)>>>;

pub struct Format {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: SampleFormat,
}

struct Ring {
    left: VecDeque<f32>,
    right: VecDeque<f32>,
    last_write: Option<Instant>,
}

pub struct Capture {
    /// Frames kept per channel.
    ring_frames: usize,
    /// WASAPI loopback delivers nothing while nothing plays; older data counts as silence.
    stale_after: Duration,
    ring: Arc<Mutex<Ring>>,
    invalidated: Arc<AtomicBool>,
    host: cpal::Host,
    stream: Option<cpal::Stream>,
    /// The device the stream is open on, and when it was opened.
    device_id: Option<cpal::DeviceId>,
    opened_at: Instant,
    /// Latest default output device and when it was read, from the watcher thread.
    /// Reading it takes a few ms, so it's kept off the render thread.
    default_device: DefaultDevice,
    retry_at: Instant,
    pub format: Option<Format>,
    pub last_error: Option<String>,
}

impl Capture {
    pub fn new(ring_frames: usize, stale_after: Duration) -> Self {
        let ring = Ring {
            left: VecDeque::with_capacity(ring_frames),
            right: VecDeque::with_capacity(ring_frames),
            last_write: None,
        };
        let mut c = Self {
            ring_frames,
            stale_after,
            ring: Arc::new(Mutex::new(ring)),
            invalidated: Arc::new(AtomicBool::new(false)),
            host: cpal::default_host(),
            stream: None,
            device_id: None,
            opened_at: Instant::now(),
            default_device: Arc::new(Mutex::new(None)),
            retry_at: Instant::now(),
            format: None,
            last_error: None,
        };
        watch_default_device(Arc::clone(&c.default_device));
        c.maintain();
        c
    }

    /// Ring memory in bytes (both channels).
    pub fn ring_bytes(&self) -> u64 {
        (self.ring_frames * 2 * size_of::<f32>()) as u64
    }

    /// Time one default-device check (for `--probe-audio`).
    pub fn time_device_check(&self) -> Duration {
        let t = Instant::now();
        let _ = self.host.default_output_device().and_then(|d| d.id().ok());
        t.elapsed()
    }

    /// Call once per frame: re-opens the stream after device changes or errors.
    pub fn maintain(&mut self) {
        let now = Instant::now();
        if self.invalidated.swap(false, Ordering::Relaxed) {
            self.close();
            self.retry_at = now + RETRY_FAST;
        }
        // Only trust readings taken after the stream was opened, so a stale one
        // can't bounce us off the device we just opened.
        let moved = self.default_device.lock().is_ok_and(|d| {
            d.as_ref()
                .is_some_and(|(at, id)| *at > self.opened_at && *id != self.device_id)
        });
        if self.stream.is_some() && moved {
            self.close();
            self.retry_at = now; // re-open on the new default right away
        }
        if self.stream.is_none() && now >= self.retry_at {
            match self.open() {
                Ok(()) => self.last_error = None,
                Err(e) => {
                    self.last_error = Some(e);
                    self.retry_at = Instant::now() + RETRY_SLOW;
                }
            }
        }
    }

    fn close(&mut self) {
        self.stream = None;
        self.format = None;
        self.device_id = None;
    }

    fn open(&mut self) -> Result<(), String> {
        let default = self.host.default_output_device().ok_or("no default output device")?;
        let id = default.id().map_err(|e| e.to_string())?;
        // Open the concrete device rather than the "follow the default" one.
        let device = self.host.device_by_id(&id).unwrap_or(default);
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        // Shared-mode WASAPI always mixes in 32-bit float.
        if supported.sample_format() != SampleFormat::F32 {
            return Err(format!("unsupported sample format {}", supported.sample_format()));
        }
        let config = supported.config();
        let channels = config.channels as usize;

        let ring = Arc::clone(&self.ring);
        let ring_frames = self.ring_frames;
        let on_data = move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let Ok(mut r) = ring.lock() else { return };
            for frame in data.chunks_exact(channels) {
                let (l, rt) = (frame[0], *frame.get(1).unwrap_or(&frame[0]));
                r.left.push_back(l);
                r.right.push_back(rt);
            }
            let excess = r.left.len().saturating_sub(ring_frames);
            r.left.drain(..excess);
            r.right.drain(..excess);
            r.last_write = Some(Instant::now());
        };
        let invalidated = Arc::clone(&self.invalidated);
        let on_error = move |e: cpal::Error| {
            if matches!(e.kind(), ErrorKind::StreamInvalidated | ErrorKind::DeviceNotAvailable) {
                invalidated.store(true, Ordering::Relaxed);
            }
        };

        let stream = device
            .build_input_stream::<f32, _, _>(config, on_data, on_error, None)
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        self.format = Some(Format {
            sample_rate: supported.sample_rate(),
            channels: supported.channels(),
            sample_format: supported.sample_format(),
        });
        self.stream = Some(stream);
        self.device_id = Some(id);
        self.opened_at = Instant::now();
        Ok(())
    }

    /// Copies the newest `n` frames into `left`/`right` (zero-padded at the
    /// front if fewer are buffered). Returns false if the data is stale,
    /// i.e. nothing has been playing.
    pub fn latest(&self, n: usize, left: &mut Vec<f32>, right: &mut Vec<f32>) -> bool {
        left.clear();
        right.clear();
        let Ok(r) = self.ring.lock() else { return false };
        let fresh = r.last_write.is_some_and(|t| t.elapsed() < self.stale_after);
        if !fresh {
            left.resize(n, 0.0);
            right.resize(n, 0.0);
            return false;
        }
        let have = r.left.len().min(n);
        left.resize(n - have, 0.0);
        right.resize(n - have, 0.0);
        let skip = r.left.len() - have;
        left.extend(r.left.iter().skip(skip));
        right.extend(r.right.iter().skip(skip));
        true
    }
}

/// Reads the default output device every `DEVICE_CHECK` on its own thread.
/// Stops when the capture that owns `shared` is dropped.
fn watch_default_device(shared: DefaultDevice) {
    let spawned = std::thread::Builder::new()
        .name("audio-device-watch".into())
        .spawn(move || {
            let host = cpal::default_host();
            while Arc::strong_count(&shared) > 1 {
                std::thread::sleep(DEVICE_CHECK);
                let id = host.default_output_device().and_then(|d| d.id().ok());
                if let Ok(mut d) = shared.lock() {
                    *d = Some((Instant::now(), id));
                }
            }
        });
    // Without the watcher, device switches still recover via cpal's errors.
    let _ = spawned;
}
