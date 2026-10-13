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
use crate::theme::{AMBER, BG, CORAL, CYAN, TEXT, VIOLET};
use layout::Layout;

pub fn draw(frame: &mut Frame, app: &App, l: &Layout) {
    let buf = frame.buffer_mut();
    let area = *buf.area();
    buf.set_style(area, Style::new().bg(BG).fg(TEXT));

    chrome::header(buf, l.header, app);

    let inner = chrome::panel(
        buf,
        l.spectrum,
        "¹",
        "spectrum",
        CYAN,
        &format!(" · {} bands · log", l.bands),
    );
    spectrum::draw(buf, inner, app, l.spec_rows);

    if let Some(r) = l.deck {
        let inner = chrome::panel(buf, r, "²", "deck", CYAN, " · now spinning");
        deck::draw(buf, inner, app);
    }
    if let Some(r) = l.cpu {
        let inner = chrome::panel(buf, r, "³", "cpu", AMBER, "");
        stats::cpu(buf, inner, app);
    }
    if let Some(r) = l.mem {
        let inner = chrome::panel(buf, r, "⁴", "mem", VIOLET, "");
        stats::mem(buf, inner, app);
    }
    if let Some(r) = l.gpu {
        let inner = chrome::panel(buf, r, "⁵", "gpu", CORAL, "");
        stats::gpu(buf, inner, app);
    }
    if let Some(r) = l.net {
        let inner = chrome::panel(buf, r, "⁶", "net", CYAN, &format!(" · {}", app.net.kind));
        stats::net(buf, inner, app);
    }
    if let Some(r) = l.procs {
        let inner = chrome::panel(buf, r, "⁷", "proc", TEXT, " · by cpu");
        stats::procs(buf, inner, app);
    }
    if let Some(r) = l.footer {
        chrome::footer(buf, r);
    }
}
