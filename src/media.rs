//! Now playing, play/pause and seek through Windows' media session API
//! (SMTC, the same source as the volume flyout's media controls), plus the
//! system master volume. Runs on its own thread; the render loop sends
//! commands and receives snapshots.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows::core::HSTRING;

use crate::app::{App, Cmd, Media};
use crate::config::MediaCfg;

/// WinRT times are in 100 ns ticks.
const TICKS_PER_S: f64 = 10_000_000.0;

struct Track {
    title: String,
    artist: String,
    album: String,
    pos: f64,
    dur: f64,
    playing: bool,
    /// When `pos` was true; the UI extrapolates from here.
    at: Instant,
}

struct Snapshot {
    track: Option<Track>,
    volume: Option<f32>,
    muted: bool,
    notice: Option<String>,
}

pub struct MediaLink {
    cmds: Sender<Cmd>,
    snaps: Receiver<Snapshot>,
}

impl MediaLink {
    pub fn spawn(cfg: &MediaCfg) -> Self {
        let (cmds, cmd_rx) = mpsc::channel();
        let (snap_tx, snaps) = mpsc::channel();
        let poll = Duration::from_millis(cfg.poll_ms);
        thread::Builder::new()
            .name("media".into())
            .spawn(move || run(poll, cmd_rx, snap_tx))
            .expect("spawn media thread");
        Self { cmds, snaps }
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.cmds.send(cmd);
    }

    /// Applies any snapshots that arrived since the last frame.
    pub fn update(&self, app: &mut App) {
        while let Ok(s) = self.snaps.try_recv() {
            app.media = s.track.map(|t| {
                let pos = if t.playing {
                    t.pos + t.at.elapsed().as_secs_f64()
                } else {
                    t.pos
                };
                Media {
                    title: t.title,
                    artist: t.artist,
                    album: t.album,
                    pos: pos.min(t.dur.max(0.0)),
                    dur: t.dur,
                    playing: t.playing,
                }
            });
            app.volume = s.volume;
            app.muted = s.muted;
            if let Some(n) = s.notice {
                app.notify(n);
            }
        }
    }
}

fn run(poll: Duration, cmds: Receiver<Cmd>, snaps: Sender<Snapshot>) {
    // SAFETY: initialises COM for this thread only (WinRT and the endpoint API need it).
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let mut src = Source::default();
    let mut volume = Volume::default();
    loop {
        let notice = match cmds.recv_timeout(poll) {
            Ok(cmd) => src.run(cmd),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let (volume, muted) = volume.read().map_or((None, false), |(v, m)| (Some(v), m));
        let snap = Snapshot {
            track: src.track(),
            volume,
            muted,
            notice,
        };
        if snaps.send(snap).is_err() {
            return; // UI is gone
        }
    }
}

#[derive(Default)]
struct Source {
    manager: Option<Manager>,
}

impl Source {
    fn session(&mut self) -> Option<Session> {
        if self.manager.is_none() {
            self.manager = Manager::RequestAsync().and_then(|op| op.join()).ok();
        }
        // Errors (including "no session") just mean nothing is playing.
        self.manager.as_ref()?.GetCurrentSession().ok()
    }

    fn track(&mut self) -> Option<Track> {
        let s = self.session()?;
        let props = s.TryGetMediaPropertiesAsync().and_then(|op| op.join()).ok()?;
        let title = text(props.Title());
        let playing = is_playing(&s);
        let (pos, dur) = timeline(&s, playing).unwrap_or((0.0, 0.0));
        Some(Track {
            title: if title.is_empty() { app_name(&s) } else { title },
            artist: text(props.Artist()),
            album: text(props.AlbumTitle()),
            pos,
            dur,
            playing,
            at: Instant::now(),
        })
    }

    /// Runs a command; returns a notice for the footer if it couldn't be done.
    fn run(&mut self, cmd: Cmd) -> Option<String> {
        let Some(s) = self.session() else {
            return Some("no media playing".into());
        };
        let app = app_name(&s);
        match cmd {
            Cmd::PlayPause => {
                let ok = s.TryTogglePlayPauseAsync().and_then(|op| op.join()).unwrap_or(false);
                (!ok).then(|| format!("{app} doesn't accept play/pause"))
            }
            Cmd::Seek(delta) => {
                let Some((pos, dur)) = timeline(&s, is_playing(&s)).filter(|&(_, d)| d > 0.0) else {
                    return Some(format!("{app} doesn't report a position"));
                };
                let start = s
                    .GetTimelineProperties()
                    .and_then(|t| t.StartTime())
                    .map_or(0, |t| t.Duration);
                let target = start + ((pos + delta).clamp(0.0, dur) * TICKS_PER_S) as i64;
                let ok = s
                    .TryChangePlaybackPositionAsync(target)
                    .and_then(|op| op.join())
                    .unwrap_or(false);
                (!ok).then(|| format!("{app} doesn't support seeking"))
            }
        }
    }
}

fn is_playing(s: &Session) -> bool {
    s.GetPlaybackInfo()
        .and_then(|p| p.PlaybackStatus())
        .is_ok_and(|st| st == Status::Playing)
}

/// (position, duration) in seconds. Apps only update the position now and
/// then, so while playing it's extrapolated from the last update time.
fn timeline(s: &Session, playing: bool) -> Option<(f64, f64)> {
    let t = s.GetTimelineProperties().ok()?;
    let start = t.StartTime().ok()?.Duration;
    let end = t.EndTime().ok()?.Duration;
    let mut pos = (t.Position().ok()?.Duration - start) as f64 / TICKS_PER_S;
    let dur = (end - start) as f64 / TICKS_PER_S;
    if playing && let Ok(updated) = t.LastUpdatedTime() {
        let since = (now_ticks() - updated.UniversalTime) as f64 / TICKS_PER_S;
        if updated.UniversalTime > 0 && (0.0..dur.max(0.0)).contains(&since) {
            pos += since;
        }
    }
    Some((pos.clamp(0.0, dur.max(0.0)), dur))
}

/// Current time as a WinRT DateTime (100 ns ticks since 1601).
fn now_ticks() -> i64 {
    // SAFETY: plain system call.
    let ft = unsafe { GetSystemTimePreciseAsFileTime() };
    ((ft.dwHighDateTime as i64) << 32) | ft.dwLowDateTime as i64
}

/// Friendly name of the app behind a session: "Spotify.exe" → "Spotify",
/// "Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic" → "ZuneMusic".
fn app_name(s: &Session) -> String {
    app_name_from_id(&text(s.SourceAppUserModelId()))
}

/// A WinRT string property, or "" if it couldn't be read.
fn text(h: windows::core::Result<HSTRING>) -> String {
    h.map(|h| h.to_string()).unwrap_or_default()
}

fn app_name_from_id(id: &str) -> String {
    let id = id.rsplit('!').next().unwrap_or(id);
    let id = id
        .strip_suffix(".exe")
        .or_else(|| id.strip_suffix(".EXE"))
        .unwrap_or(id);
    let id = id.split('_').next().unwrap_or(id);
    let name = id.rsplit(['\\', '.']).next().unwrap_or(id);
    if name.is_empty() {
        "the player".into()
    } else {
        name.into()
    }
}

/// Re-resolve the default output device this often, to follow device changes.
const VOLUME_DEVICE_REFRESH: Duration = Duration::from_secs(2);

/// Master volume of the default output device. Connecting to the device is
/// the slow part, so the connection is kept and refreshed periodically.
#[derive(Default)]
struct Volume {
    endpoint: Option<(IAudioEndpointVolume, Instant)>,
}

impl Volume {
    /// Volume (0..1) and mute.
    fn read(&mut self) -> Option<(f32, bool)> {
        if self
            .endpoint
            .as_ref()
            .is_none_or(|(_, at)| at.elapsed() > VOLUME_DEVICE_REFRESH)
        {
            self.endpoint = default_endpoint().map(|e| (e, Instant::now()));
        }
        let (ep, _) = self.endpoint.as_ref()?;
        // SAFETY: valid endpoint interface on a COM-initialised thread.
        let read = unsafe {
            ep.GetMasterVolumeLevelScalar()
                .and_then(|v| Ok((v, ep.GetMute()?.as_bool())))
        };
        if read.is_err() {
            self.endpoint = None; // device went away; reconnect next time
        }
        read.ok()
    }
}

fn default_endpoint() -> Option<IAudioEndpointVolume> {
    // SAFETY: COM is initialised on this thread; interfaces release on drop.
    unsafe {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = devices.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        device.Activate(CLSCTX_ALL, None).ok()
    }
}

/// `--probe-media`: prints what tide sees right now and how long it took.
pub fn probe() -> String {
    use std::fmt::Write;
    // SAFETY: COM for this thread.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let mut src = Source::default();
    let mut out = String::new();
    let t = Instant::now();
    let session = src.session();
    let _ = writeln!(
        out,
        "session: {} ({:?})",
        session.as_ref().map_or("none".into(), app_name),
        t.elapsed()
    );
    let t = Instant::now();
    match src.track() {
        Some(tr) => {
            let _ = writeln!(
                out,
                "track: {:?} · {:?} · {:?} · {:.1}/{:.1} s · playing {} ({:?})",
                tr.title,
                tr.artist,
                tr.album,
                tr.pos,
                tr.dur,
                tr.playing,
                t.elapsed()
            );
        }
        None => {
            let _ = writeln!(out, "track: none");
        }
    }
    let mut vol = Volume::default();
    let t = Instant::now();
    let first = vol.read();
    let connect = t.elapsed();
    let t = Instant::now();
    let again = vol.read();
    let _ = writeln!(
        out,
        "volume: {first:?} (connect + read {connect:?}) · {again:?} (cached read {:?})",
        t.elapsed()
    );
    out
}

#[cfg(test)]
mod tests {
    use super::app_name_from_id;

    #[test]
    fn app_names() {
        assert_eq!(app_name_from_id("Spotify.exe"), "Spotify");
        assert_eq!(
            app_name_from_id("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"),
            "ZuneMusic"
        );
        assert_eq!(app_name_from_id("MSEdge"), "MSEdge");
        assert_eq!(app_name_from_id(""), "the player");
    }
}
