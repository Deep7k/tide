mod app;
mod audio;
mod bench;
mod config;
mod demo;
mod media;
mod profile;
mod sys;
mod theme;
mod ui;

use std::cell::Cell;
use std::io::{self, BufWriter, Stdout, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ratatui::{
    Terminal, TerminalOptions, Viewport,
    backend::{CrosstermBackend, TestBackend},
    crossterm::{
        cursor,
        event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
        execute, queue,
        terminal::{self, BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen, LeaveAlternateScreen},
    },
    layout::Rect,
};

use app::{App, Cmd};
use audio::Audio;
use config::Config;
use demo::Demo;
use media::MediaLink;
use profile::{Phase, Profile};
use sys::Stats;

const USAGE: &str = "usage: tide [--config PATH] [--demo] [--fps N]
       --config PATH    settings file (default: %APPDATA%\\tide\\config.toml if present)
       --print-config   print every setting with its default, as a starting config
       --demo           fake data everywhere (no audio capture)
       --dump WxH       print one demo frame as plain text
       --probe-audio    capture 5 s of system audio and print level stats
       --probe-stats    sample system stats for 2 s and print them
       --probe-media    print the current media session and master volume
       --bench WxH      time off-screen rendering of demo frames";

type Term = Terminal<CrosstermBackend<FrameWriter>>;

/// Collects a whole frame and sends it to the console in one write. Console
/// writes are synchronous (the console parses them before returning), so
/// ratatui's own flush is held back until the gate opens after the
/// end-of-update marker.
struct FrameWriter {
    out: BufWriter<Stdout>,
    gate: Rc<Cell<bool>>,
}

impl Write for FrameWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.out.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.gate.get() { self.out.flush() } else { Ok(()) }
    }
}

struct Args {
    demo: bool,
    /// Overrides `fps` from the config.
    fps: Option<u32>,
    config: Option<PathBuf>,
    print_config: bool,
    dump: Option<(u16, u16)>,
    /// Hidden dev options: write a loop profile on exit; quit after N seconds.
    profile: Option<PathBuf>,
    quit_after: Option<f64>,
    probe_audio: bool,
    probe_stats: bool,
    probe_media: bool,
    bench: Option<(u16, u16)>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        demo: false,
        fps: None,
        config: None,
        print_config: false,
        dump: None,
        profile: None,
        quit_after: None,
        probe_audio: false,
        probe_stats: false,
        probe_media: false,
        bench: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--demo" => args.demo = true,
            "--profile" => args.profile = Some(it.next().ok_or("--profile needs a file")?.into()),
            "--quit-after" => {
                let v = it.next().ok_or("--quit-after needs seconds")?;
                args.quit_after = Some(v.parse().map_err(|_| format!("bad --quit-after: {v}"))?);
            }
            "--probe-audio" => args.probe_audio = true,
            "--probe-stats" => args.probe_stats = true,
            "--probe-media" => args.probe_media = true,
            "--print-config" => args.print_config = true,
            "--config" => args.config = Some(it.next().ok_or("--config needs a path")?.into()),
            "--fps" => {
                let v = it.next().ok_or("--fps needs a value")?;
                let fps = v.parse::<u32>().ok().filter(|f| (1..=240).contains(f));
                args.fps = Some(fps.ok_or(format!("bad --fps: {v} (1–240)"))?);
            }
            "--bench" => args.bench = Some(parse_size(it.next().ok_or("--bench needs WxH")?)?),
            "--dump" => args.dump = Some(parse_size(it.next().ok_or("--dump needs WxH")?)?),
            "-h" | "--help" => return Err(USAGE.into()),
            other => return Err(format!("unknown argument: {other}\n{USAGE}")),
        }
    }
    Ok(args)
}

/// "165x42" → (165, 42).
fn parse_size(v: String) -> Result<(u16, u16), String> {
    let (w, h) = v.split_once('x').ok_or(format!("bad size {v:?}, expected WxH"))?;
    let num = |s: &str| {
        s.parse::<u16>()
            .ok()
            .filter(|&n| n > 0)
            .ok_or(format!("bad size {v:?}"))
    };
    Ok((num(w)?, num(h)?))
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    if args.print_config {
        print!("{}", config::default_toml());
        return ExitCode::SUCCESS;
    }
    let cfg = match load_settings(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("tide: {e}");
            return ExitCode::from(2);
        }
    };
    if let Some((w, h)) = args.dump {
        print!("{}", dump(w, h, &cfg));
        return ExitCode::SUCCESS;
    }
    if args.probe_audio {
        print!("{}", Audio::probe(&cfg.audio, 5.0));
        return ExitCode::SUCCESS;
    }
    if let Some((w, h)) = args.bench {
        print!("{}", bench::run(&cfg, w, h));
        return ExitCode::SUCCESS;
    }
    if args.probe_media {
        print!("{}", media::probe());
        return ExitCode::SUCCESS;
    }
    if args.probe_stats {
        print!("{}", sys::probe(&cfg.stats));
        return ExitCode::SUCCESS;
    }
    let opts = RunOpts {
        demo_only: args.demo,
        profile: args.profile,
        quit_after: args.quit_after.map(Duration::from_secs_f64),
    };
    match run(cfg, opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tide: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Config file, then command-line overrides, then the theme it names.
/// Warnings go to stderr, where they're readable after tide exits.
fn load_settings(args: &Args) -> Result<Config, String> {
    let loaded = config::load(args.config.as_deref())?;
    let mut cfg = loaded.config;
    if let Some(fps) = args.fps {
        cfg.fps = fps;
    }
    let base_dir = loaded.path.as_deref().and_then(|p| p.parent());
    let (theme, theme_warnings) = theme::load(&cfg.theme, base_dir)?;
    theme::install(theme);
    for w in loaded.warnings.iter().chain(&theme_warnings) {
        eprintln!("tide: warning: {w}");
    }
    Ok(cfg)
}

struct RunOpts {
    demo_only: bool,
    profile: Option<PathBuf>,
    quit_after: Option<Duration>,
}

fn run(cfg: Config, opts: RunOpts) -> io::Result<()> {
    // Windows' default ~15.6 ms timer tick can't pace 60 fps.
    // SAFETY: plain Win32 call, paired with timeEndPeriod below.
    unsafe { windows::Win32::Media::timeBeginPeriod(1) };
    let (mut term, gate) = setup()?;
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        prev_hook(info);
    }));

    let mut prof = opts.profile.is_some().then(Profile::new);
    let result = event_loop(&mut term, &gate, cfg, &opts, &mut prof);

    restore();
    unsafe { windows::Win32::Media::timeEndPeriod(1) };
    if let (Some(p), Some(path)) = (prof, &opts.profile) {
        std::fs::write(path, p.report())?;
    }
    result
}

fn setup() -> io::Result<(Term, Rc<Cell<bool>>)> {
    terminal::enable_raw_mode()?;
    let mut out = BufWriter::with_capacity(1 << 16, io::stdout());
    execute!(out, EnterAlternateScreen, cursor::Hide)?;
    // A fixed viewport stops ratatui asking the console for its size every
    // frame (a round trip to the console host); resizes arrive as events.
    let (w, h) = terminal::size()?;
    let gate = Rc::new(Cell::new(false));
    let writer = FrameWriter {
        out,
        gate: Rc::clone(&gate),
    };
    let viewport = Viewport::Fixed(Rect::new(0, 0, w, h));
    let term = Terminal::with_options(CrosstermBackend::new(writer), TerminalOptions { viewport })?;
    Ok((term, gate))
}

fn restore() {
    let _ = terminal::disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, cursor::Show);
}

fn event_loop(
    term: &mut Term,
    gate: &Cell<bool>,
    cfg: Config,
    opts: &RunOpts,
    prof: &mut Option<Profile>,
) -> io::Result<()> {
    let demo_only = opts.demo_only;
    let started = Instant::now();
    let lap = |prof: &mut Option<Profile>, phase: Phase| {
        if let Some(p) = prof.as_mut() {
            p.lap(phase);
        }
    };
    let fps = cfg.fps;
    let idle_fps = cfg.idle_fps;
    let max_bands = cfg.spectrum.max_bands;
    let max_rows = (cfg.spectrum.max_rows > 0).then_some(cfg.spectrum.max_rows);
    // Live mode reads audio and system stats; --demo fakes everything.
    let mut audio = (!demo_only).then(|| Audio::new(&cfg.audio));
    let stats = (!demo_only).then(|| Stats::spawn(&cfg.stats));
    let media = (!demo_only).then(|| MediaLink::spawn(&cfg.media));
    let seek = cfg.media.seek_seconds;
    let mut app = App::new(cfg);
    let mut demo = demo_only.then(|| Demo::new(&mut app));
    let frame = Duration::from_secs_f64(1.0 / fps as f64);
    let idle_frame = Duration::from_secs_f64(1.0 / idle_fps.min(fps) as f64);
    let mut area = term.get_frame().area();
    let mut last = Instant::now();
    let mut next = last + frame;

    loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.25);
        last = now;
        if dt > 0.0 {
            let inst = 1.0 / dt;
            app.fps = if app.fps == 0.0 {
                inst
            } else {
                app.fps + (inst - app.fps) * 0.05
            };
        }

        let layout = ui::layout::compute(area, max_bands, max_rows);
        lap(prof, Phase::Layout);
        app.band_count = layout.bands as usize;
        if let Some(d) = demo.as_mut() {
            d.update(&mut app, dt as f64);
        }
        if let Some(a) = audio.as_mut() {
            a.update(&mut app, dt);
        }
        if let Some(s) = &stats {
            s.update(&mut app);
        }
        if let Some(m) = &media {
            m.update(&mut app);
        }
        app.tick(dt);
        lap(prof, Phase::Data);

        queue!(term.backend_mut(), BeginSynchronizedUpdate)?;
        term.draw(|f| ui::draw(f, &app, &layout))?;
        queue!(term.backend_mut(), EndSynchronizedUpdate)?;
        gate.set(true);
        let flushed = term.backend_mut().flush();
        gate.set(false);
        flushed?;
        lap(prof, Phase::Draw);

        // While nothing moves, redraw at the idle rate instead.
        if app.is_idle() {
            next = next.max(now + idle_frame);
        }

        // Handle input until the next frame is due; anything that changes the
        // screen ends the wait early so it shows at once.
        loop {
            let timeout = next.saturating_duration_since(Instant::now());
            if !event::poll(timeout)? {
                break;
            }
            match event::read()? {
                Event::Resize(w, h) => {
                    area = Rect::new(0, 0, w, h);
                    term.resize(area)?;
                    next = Instant::now();
                }
                Event::Key(key) => match key_action(key, seek) {
                    Some(Action::Quit) => return Ok(()),
                    Some(Action::Cmd(cmd)) => {
                        if let Some(m) = &media {
                            m.send(cmd);
                        } else if let Some(d) = demo.as_mut() {
                            d.apply(&mut app, cmd);
                        }
                        next = Instant::now();
                    }
                    None => {}
                },
                _ => {}
            }
        }
        lap(prof, Phase::Wait);
        if opts.quit_after.is_some_and(|d| started.elapsed() >= d) {
            return Ok(());
        }
        next += frame;
        let now = Instant::now();
        if next < now {
            next = now + frame; // fell behind; don't try to catch up
        }
    }
}

enum Action {
    Quit,
    Cmd(Cmd),
}

fn key_action(key: KeyEvent, seek: f64) -> Option<Action> {
    // Windows reports both press and release.
    if key.kind != KeyEventKind::Press {
        return None;
    }
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        KeyCode::Char(' ') => Some(Action::Cmd(Cmd::PlayPause)),
        KeyCode::Left => Some(Action::Cmd(Cmd::Seek(-seek))),
        KeyCode::Right => Some(Action::Cmd(Cmd::Seek(seek))),
        _ => None,
    }
}

/// Renders the demo 3 s in (so peaks and history look settled) and returns
/// the frame as plain text, one line per row.
fn dump(w: u16, h: u16, cfg: &Config) -> String {
    let mut app = App::new(cfg.clone());
    let mut demo = Demo::new(&mut app);
    let layout = ui::layout::compute(
        Rect::new(0, 0, w, h),
        cfg.spectrum.max_bands,
        (cfg.spectrum.max_rows > 0).then_some(cfg.spectrum.max_rows),
    );
    app.band_count = layout.bands as usize;
    for _ in 0..180 {
        demo.update(&mut app, 1.0 / 60.0);
        app.tick(1.0 / 60.0);
    }
    app.fps = 60.0;

    let mut term = Terminal::new(TestBackend::new(w, h)).expect("test backend");
    term.draw(|f| ui::draw(f, &app, &layout)).expect("draw");
    let buf = term.backend().buffer();
    let mut out = String::new();
    for y in 0..h {
        for x in 0..w {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Config, dump};

    /// Compares a demo frame with the saved snapshot. Run with
    /// `TIDE_BLESS=1` to rewrite snapshots after an intended change.
    fn check(w: u16, h: u16) {
        let path = format!("{}/snapshots/demo_{w}x{h}.txt", env!("CARGO_MANIFEST_DIR"));
        let got = dump(w, h, &Config::default());
        if std::env::var_os("TIDE_BLESS").is_some() {
            std::fs::write(&path, &got).unwrap();
            return;
        }
        let want = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
        assert!(
            got == want,
            "frame {w}x{h} differs from {path}; rerun with TIDE_BLESS=1 if intended\n{got}"
        );
    }

    #[test]
    fn snapshot_full_size() {
        check(165, 42);
    }

    #[test]
    fn snapshot_default_terminal() {
        check(120, 30);
    }
}
