mod app;
mod audio;
mod demo;
mod theme;
mod ui;

use std::io::{self, BufWriter, Stdout};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use ratatui::{
    Terminal,
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
use demo::Demo;

const USAGE: &str = "usage: tide [--demo] [--fps N]
       --demo           fake data everywhere (no audio capture)
       --dump WxH       print one demo frame as plain text
       --probe-audio    capture 5 s of system audio and print level stats";

type Term = Terminal<CrosstermBackend<BufWriter<Stdout>>>;

struct Args {
    demo: bool,
    fps: u32,
    dump: Option<(u16, u16)>,
    probe_audio: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        demo: false,
        fps: 60,
        dump: None,
        probe_audio: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--demo" => args.demo = true,
            "--probe-audio" => args.probe_audio = true,
            "--fps" => {
                let v = it.next().ok_or("--fps needs a value")?;
                args.fps = v
                    .parse()
                    .ok()
                    .filter(|f| (1..=240).contains(f))
                    .ok_or(format!("bad --fps: {v}"))?;
            }
            "--dump" => {
                let v = it.next().ok_or("--dump needs WxH")?;
                let (w, h) = v.split_once('x').ok_or(format!("bad --dump: {v}"))?;
                args.dump = Some((
                    w.parse().map_err(|_| "bad width")?,
                    h.parse().map_err(|_| "bad height")?,
                ));
            }
            "-h" | "--help" => return Err(USAGE.into()),
            other => return Err(format!("unknown argument: {other}\n{USAGE}")),
        }
    }
    Ok(args)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    if let Some((w, h)) = args.dump {
        print!("{}", dump(w, h));
        return ExitCode::SUCCESS;
    }
    if args.probe_audio {
        print!("{}", Audio::probe(5.0));
        return ExitCode::SUCCESS;
    }
    match run(args.fps, args.demo) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tide: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(fps: u32, demo_only: bool) -> io::Result<()> {
    // Windows' default ~15.6 ms timer tick can't pace 60 fps.
    // SAFETY: plain Win32 call, paired with timeEndPeriod below.
    unsafe { windows::Win32::Media::timeBeginPeriod(1) };
    let mut term = setup()?;
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        prev_hook(info);
    }));

    let result = event_loop(&mut term, fps, demo_only);

    restore();
    unsafe { windows::Win32::Media::timeEndPeriod(1) };
    result
}

fn setup() -> io::Result<Term> {
    terminal::enable_raw_mode()?;
    let mut out = BufWriter::with_capacity(1 << 16, io::stdout());
    execute!(out, EnterAlternateScreen, cursor::Hide)?;
    Terminal::new(CrosstermBackend::new(out))
}

fn restore() {
    let _ = terminal::disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, cursor::Show);
}

fn event_loop(term: &mut Term, fps: u32, demo_only: bool) -> io::Result<()> {
    let mut app = App::new();
    // Live mode: real audio; the demo still feeds stats and media until those land.
    let mut demo = Demo::new(&mut app, !demo_only);
    let mut audio = (!demo_only).then(Audio::new);
    let frame = Duration::from_secs_f64(1.0 / fps as f64);
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

        let size = term.size()?;
        let layout = ui::layout::compute(Rect::new(0, 0, size.width, size.height));
        app.band_count = layout.bands as usize;
        demo.update(&mut app, dt as f64);
        if let Some(a) = audio.as_mut() {
            a.update(&mut app, dt);
        }
        app.tick(dt);

        queue!(term.backend_mut(), BeginSynchronizedUpdate)?;
        term.draw(|f| ui::draw(f, &app, &layout))?;
        execute!(term.backend_mut(), EndSynchronizedUpdate)?;

        // Handle input until the next frame is due.
        loop {
            let timeout = next.saturating_duration_since(Instant::now());
            if !event::poll(timeout)? {
                break;
            }
            if let Event::Key(key) = event::read()? {
                match key_action(key) {
                    Some(Action::Quit) => return Ok(()),
                    Some(Action::Cmd(cmd)) => demo.apply(&mut app, cmd),
                    None => {}
                }
            }
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

fn key_action(key: KeyEvent) -> Option<Action> {
    // Windows reports both press and release.
    if key.kind != KeyEventKind::Press {
        return None;
    }
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        KeyCode::Char(' ') => Some(Action::Cmd(Cmd::PlayPause)),
        KeyCode::Left => Some(Action::Cmd(Cmd::Seek(-5.0))),
        KeyCode::Right => Some(Action::Cmd(Cmd::Seek(5.0))),
        _ => None,
    }
}

/// Renders the demo 3 s in (so peaks and history look settled) and returns
/// the frame as plain text, one line per row.
fn dump(w: u16, h: u16) -> String {
    let mut app = App::new();
    let mut demo = Demo::new(&mut app, false);
    let layout = ui::layout::compute(Rect::new(0, 0, w, h));
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
    use super::dump;

    /// Compares a demo frame with the saved snapshot. Run with
    /// `TIDE_BLESS=1` to rewrite snapshots after an intended change.
    fn check(w: u16, h: u16) {
        let path = format!("{}/snapshots/demo_{w}x{h}.txt", env!("CARGO_MANIFEST_DIR"));
        let got = dump(w, h);
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
        check(165, 41);
    }

    #[test]
    fn snapshot_default_terminal() {
        check(120, 30);
    }
}
