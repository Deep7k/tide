//! Cell-grid layout for mockup2, with graceful degradation on small
//! terminals. Full design size is 165×41.
//!
//! Width:  the deck shrinks first (its record scales down), then the
//!         spectrum drops bands, then the deck is hidden. Bottom row panels
//!         drop from the right: proc shrinks its name column, then hides.
//! Height: spectrum bars shrink from 18 rows to 8, then the spacer rows go,
//!         then the bottom row is hidden.

use ratatui::layout::Rect;

pub const BAND_W: u16 = 3;
pub const MAX_BANDS: u16 = 36;
const MIN_BANDS_WITH_DECK: u16 = 24;
pub const MAX_SPEC_ROWS: u16 = 18;
const MIN_SPEC_ROWS: u16 = 8;
/// Spectrum box rows besides the bars: border×2, freq axis, blank, VU×2.
const SPEC_EXTRA_ROWS: u16 = 6;
const DECK_MIN_W: u16 = 28;

const GAP_X: u16 = 2;
const CPU_W: u16 = 37;
const MEMGPU_W: u16 = 38;
const NET_W: u16 = 38;
const PROC_MIN_W: u16 = 36;
const BOTTOM_H: u16 = 12;
const MEM_H: u16 = 5;

#[derive(Debug, Default, PartialEq)]
pub struct Layout {
    pub header: Rect,
    pub spectrum: Rect,
    pub bands: u16,
    pub spec_rows: u16,
    pub deck: Option<Rect>,
    pub cpu: Option<Rect>,
    pub mem: Option<Rect>,
    pub gpu: Option<Rect>,
    pub net: Option<Rect>,
    pub procs: Option<Rect>,
    pub footer: Option<Rect>,
}

fn spec_box_w(bands: u16) -> u16 {
    bands * BAND_W + 4
}

pub fn compute(area: Rect) -> Layout {
    let (w, h) = (area.width, area.height);
    let mut l = Layout {
        header: Rect::new(area.x, area.y, w, 1.min(h)),
        ..Default::default()
    };

    // --- top row widths
    let mut bands = MAX_BANDS;
    while bands > MIN_BANDS_WITH_DECK && w < spec_box_w(bands) + GAP_X + DECK_MIN_W {
        bands -= 1;
    }
    let show_deck = w >= spec_box_w(bands) + GAP_X + DECK_MIN_W;
    if !show_deck {
        bands = (w.saturating_sub(4) / BAND_W).clamp(1, MAX_BANDS);
    }
    l.bands = bands;

    // --- vertical plan
    // full:    header, blank, top, gap, bottom, blank, footer
    // compact: header, top, bottom, footer
    // no bottom row: header, blank, top, footer (blank dropped if tight)
    let full_rows = h.saturating_sub(1 + 1 + SPEC_EXTRA_ROWS + 1 + BOTTOM_H + 1 + 1);
    let compact_rows = h.saturating_sub(1 + SPEC_EXTRA_ROWS + BOTTOM_H + 1);
    let (spec_rows, spacer, bottom) = if full_rows >= MIN_SPEC_ROWS {
        (full_rows.min(MAX_SPEC_ROWS), 1, true)
    } else if compact_rows >= MIN_SPEC_ROWS {
        (compact_rows, 0, true)
    } else {
        let rows = h.saturating_sub(1 + 1 + SPEC_EXTRA_ROWS + 1);
        if rows >= 3 {
            (rows.min(MAX_SPEC_ROWS), 1, false)
        } else {
            (h.saturating_sub(1 + SPEC_EXTRA_ROWS + 1).max(1), 0, false)
        }
    };
    l.spec_rows = spec_rows;

    let top_y = area.y + 1 + spacer;
    let top_h = spec_rows + SPEC_EXTRA_ROWS;
    let spec_w = spec_box_w(bands).min(w);
    l.spectrum = Rect::new(area.x, top_y, spec_w, top_h);
    if show_deck {
        let x = spec_w + GAP_X;
        l.deck = Some(Rect::new(area.x + x, top_y, w - x, top_h));
    }

    let mut next_y = top_y + top_h;
    if bottom {
        let y = next_y + spacer;
        let mut x = area.x;
        let fits = |x: u16, bw: u16| x - area.x + bw <= w;
        if fits(x, CPU_W) {
            l.cpu = Some(Rect::new(x, y, CPU_W, BOTTOM_H));
            x += CPU_W + GAP_X;
            if fits(x, MEMGPU_W) {
                l.mem = Some(Rect::new(x, y, MEMGPU_W, MEM_H));
                l.gpu = Some(Rect::new(x, y + MEM_H + 1, MEMGPU_W, BOTTOM_H - MEM_H - 1));
                x += MEMGPU_W + GAP_X;
                if fits(x, NET_W) {
                    l.net = Some(Rect::new(x, y, NET_W, BOTTOM_H));
                    x += NET_W + GAP_X;
                    if fits(x, PROC_MIN_W) {
                        l.procs = Some(Rect::new(x, y, w - (x - area.x), BOTTOM_H));
                    }
                }
            }
        }
        next_y = y + BOTTOM_H;
    }

    // Without the bottom row the row budget above has no blank before the footer.
    let footer_y = next_y + if bottom { spacer } else { 0 };
    if footer_y < area.bottom() {
        l.footer = Some(Rect::new(area.x, footer_y, w, 1));
    }
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_size_matches_mock() {
        let l = compute(Rect::new(0, 0, 165, 41));
        assert_eq!(l.bands, 36);
        assert_eq!(l.spec_rows, 18);
        assert_eq!(l.spectrum, Rect::new(0, 2, 112, 24));
        assert_eq!(l.deck, Some(Rect::new(114, 2, 51, 24)));
        assert_eq!(l.cpu, Some(Rect::new(0, 27, 37, 12)));
        assert_eq!(l.mem, Some(Rect::new(39, 27, 38, 5)));
        assert_eq!(l.gpu, Some(Rect::new(39, 33, 38, 6)));
        assert_eq!(l.net, Some(Rect::new(79, 27, 38, 12)));
        assert_eq!(l.procs, Some(Rect::new(119, 27, 46, 12)));
        assert_eq!(l.footer, Some(Rect::new(0, 40, 165, 1)));
    }

    #[test]
    fn default_windows_terminal_size() {
        let l = compute(Rect::new(0, 0, 120, 30));
        assert!(l.deck.is_some());
        assert_eq!(l.bands, 28);
        assert!(l.net.is_some() && l.procs.is_none());
        assert!(l.spec_rows >= MIN_SPEC_ROWS);
        assert_eq!(l.footer.map(|f| f.y), Some(29));
    }

    #[test]
    fn footer_shown_without_bottom_row() {
        let l = compute(Rect::new(0, 0, 90, 24));
        assert!(l.cpu.is_none());
        assert_eq!(l.footer.map(|f| f.y), Some(23));
    }

    #[test]
    fn nothing_overflows_at_any_size() {
        for w in 10..=200 {
            for h in 5..=60 {
                let area = Rect::new(0, 0, w, h);
                let l = compute(area);
                for r in [Some(l.spectrum), l.deck, l.cpu, l.mem, l.gpu, l.net, l.procs, l.footer]
                    .into_iter()
                    .flatten()
                {
                    assert!(r.right() <= area.right(), "{w}x{h}: {r:?} too wide");
                }
            }
        }
    }
}
