# Media

`src/media.rs`: the track line, progress, volume and record spin in the deck,
and the `space` / `← →` keys. Runs on the `media` thread, which needs COM
(initialised as multithreaded) for both WinRT and the audio endpoint API.

```
main thread                         media thread (every poll_ms = 250 ms)
  key → Cmd ──── channel ────►  run the command on the current session
                                read session: title, artist, album,
                                timeline, play state; master volume
  MediaLink::update ◄─ channel ─ Snapshot { track, volume, muted, notice }
```

## Session

`GlobalSystemMediaTransportControlsSessionManager::RequestAsync()` is created
once; `GetCurrentSession()` returns whichever app Windows treats as current (the
one in the volume flyout). Any error, including "no session", means nothing is
playing: the deck shows `nothing playing`, hides progress, and the record stops.

Each poll reads:

- `TryGetMediaPropertiesAsync()` → title, artist, album. An empty title falls back to
  the app's name.
- `GetPlaybackInfo().PlaybackStatus()` → playing or not.
- `GetTimelineProperties()` → start, end, position, last-updated time.

WinRT async operations are awaited with `.join()` (blocking is fine on this thread).

## Position extrapolation

Apps update the timeline position only now and then (Spotify every few seconds).
While playing, the media thread adds the time since `LastUpdatedTime` (compared
against `GetSystemTimePreciseAsFileTime`, both in 100 ns ticks since 1601), and
the snapshot records when it was taken. `MediaLink::update` adds the time since
then, and `App::tick` keeps advancing the position every frame between polls.
So the progress bar moves smoothly at the frame rate. A duration of 0 hides
the progress bar.

## Commands

| Key | Call | If the app refuses |
| --- | --- | --- |
| `space` | `TryTogglePlayPauseAsync` | `<App> doesn't accept play/pause` |
| `←` / `→` | `TryChangePlaybackPositionAsync(start + clamp(pos ± seek_seconds))` | `<App> doesn't support seeking` |
| any, no session | — | `no media playing` |

Refusals return a notice in the next snapshot, shown in the footer for 3 s. The
app name comes from the session's `SourceAppUserModelId` (`Spotify.exe` →
`Spotify`, `…_8wekyb3d8bbwe!Microsoft.ZuneMusic` → `ZuneMusic`).

In `--demo` mode the same `Cmd`s go to `Demo::apply`, which pauses and seeks the
fake track.

## Master volume

`IMMDeviceEnumerator::GetDefaultAudioEndpoint(eRender, eConsole)` →
`IAudioEndpointVolume` → scalar volume and mute. Connecting takes ~9 ms, so the
endpoint is kept and re-resolved every 2 s (following device changes), or as
soon as a read fails. A cached read takes ~0.15 ms. Until the first read the
volume line is hidden rather than showing 0%.

## Record spin

`App::tick` eases `record_speed` toward `spin_speed` (3.5 rad/s) while playing and
toward 0 otherwise, at a rate that stops it from full speed in `spin_ease_ms`
(400 ms).

## Cost

About 3–4 ms of work per poll on the media thread; nothing on the render thread
beyond draining the channel. `--probe-media` prints the session, track and volume
with timings.
