//! Renders one frame of mockup2 from `App` state.

pub mod chrome;
pub mod deck;
pub mod fmt;
pub mod glyphs;
pub mod layout;
pub mod spectrum;
pub mod stats;

use ratatui::{Frame, style::Style};

use crate::app::App;
use crate::theme::th;
use layout::Layout;

pub fn draw(frame: &mut Frame, app: &App, l: &Layout) {
    let buf = frame.buffer_mut();
    let area = *buf.area();
    buf.set_style(area, Style::new().bg(th().bg).fg(th().text));

    chrome::header(buf, l.header, app);

    let inner = chrome::panel(
        buf,
        l.spectrum,
        "¹",
        "spectrum",
        th().accent,
        &format!(" · {} bands · log", l.bands),
    );
    spectrum::draw(buf, inner, app, l.spec_rows);

    if let Some(r) = l.deck {
        let inner = chrome::panel(buf, r, "²", "deck", th().accent, " · now spinning");
        deck::draw(buf, inner, app);
    }
    if let Some(r) = l.cpu {
        let inner = chrome::panel(buf, r, "³", "cpu", th().cpu, "");
        stats::cpu(buf, inner, app);
    }
    if let Some(r) = l.mem {
        let inner = chrome::panel(buf, r, "⁴", "mem", th().mem, "");
        stats::mem(buf, inner, app);
    }
    if let Some(r) = l.gpu {
        let inner = chrome::panel(buf, r, "⁵", "gpu", th().gpu, "");
        stats::gpu(buf, inner, app);
    }
    if let Some(r) = l.net {
        let inner = chrome::panel(buf, r, "⁶", "net", th().net_down, &format!(" · {}", app.net.kind));
        stats::net(buf, inner, app);
    }
    if let Some(r) = l.procs {
        let inner = chrome::panel(buf, r, "⁷", "proc", th().text, " · by cpu");
        stats::procs(buf, inner, app);
    }
    if let Some(r) = l.footer {
        chrome::footer(buf, r);
    }
}
