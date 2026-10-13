# Rendering

`src/ui/`: turns `App` into a frame on ratatui's cell buffer, then out to the
terminal. The design (originally `mockups/design_handoff_tide_mockup2`, later
revised with a top panel) is a fixed cell grid in 24-bit colour, made only of
plain Unicode glyphs: block elements, braille and box drawing.

## Layout (`layout.rs`)

At the full design size, **165×42**:

```
row 0–2    top panel (boxed): ◆ user@host ................ bat ▮▮▮▮▮▮▯▯▯▯ 60%   60 fps
row 3–26   ╭ ¹ spectrum ─ 112 wide ─╮  ╭ ² deck ─ rest of width ─╮
           │ 18 rows of bars         │  │ track line               │
           │ freq axis               │  │ record (braille)         │
           │ blank                   │  │ progress                 │
           │ VU L / VU R             │  │ volume · rpm · format    │
           ╰─────────────────────────╯  ╰ audio source line ───────╯
row 27     blank
row 28–39  cpu 37 │ mem 38 / gpu 38 │ net 38 │ proc (rest, ≥36)      — 12 rows each
row 40     blank
row 41     footer: notices on the left, key hints on the right
```

The spectrum box is `bands × 3 + 4` columns (each band is 2 cells + 1 space; 36
bands → 108 inner columns). Gaps are 2 columns horizontally and 1 row vertically.
mem (5 rows) and gpu (6 rows) share a column with a blank row between.

### Larger terminals

- **Height** goes to the spectrum: bar rows grow to fill the terminal
  (`spectrum.max_rows` caps it; 0 = fill). The deck grows with it, and so does
  the record.
- **Width** goes to the deck and the proc panel. The proc name column widens so cpu
  and mem stay at the right edge; proc rows fill the box (`proc_rows = 0`); net
  graphs grow to 3 rows each when the box has room.

### Smaller terminals

Width, in order:

1. The spectrum drops bands (down to 24) so the deck keeps the 46 columns its
   controls need.
2. Then the deck shrinks to 28 columns (the record scales down), then it's hidden
   and the spectrum takes the full width.
3. Bottom row panels drop from the right: proc first (after its name column has
   shrunk to 12), then net, then mem/gpu. The last panel shown stretches to the
   right edge so there's no empty strip.

Height, in order:

1. Spectrum bars shrink from 18 rows to 8.
2. The spacer rows go and the top panel collapses to one unboxed line (the
   "compact" plan; this is what the default 120×30 Windows Terminal gets).
3. The bottom row is hidden.

`layout::tests` pins the full-size rectangles, the 120×30 and 90×24 plans, and
checks that no rectangle overflows at any size from 10×5 to 200×60.

## Drawing primitives (`glyphs.rs`)

- **`Ln`**: a row of coloured text built span by span (adjacent spans with the same
  style merge). `put` writes it into the buffer clipped to an area. Every glyph tide
  uses is one cell wide, so `put` writes characters straight into cells
  (`Cell::set_char`) instead of ratatui's string path, which runs grapheme
  segmentation on every span.
- **`bar`**: horizontal bar at eighth-cell resolution (`█` + one of `▏▎▍▌▋▊▉`) on a
  `░` track.
- **`spark_rows`**: sparkline over any number of rows, 8 levels per row (the net
  graphs; `spark` is the 1-row case used by cpu).
- **`pips`**: the 10-step `▮▯` meter (volume, battery).
- **`Braille`**: 2×4-dot canvas per cell for the record.

`fmt.rs` holds every number format (percentages, sizes, rates, temperatures,
frequency labels).

## Panels

`ui::draw` fills the background, then draws each panel with `chrome::panel`
(rounded box, `¹ name · suffix` title on the top border) and passes its content
area (inside the border plus 1 column of padding) to the panel's draw function.

- **Spectrum** (`spectrum.rs`): each bar row has one colour from the `viz` gradient
  (bottom to top). Partial cells use `▁…▇`. A peak cap `▔▔` is drawn only on an
  empty cell at the peak's row. The frequency axis labels every 5th band. VU meters
  are `▌` cells coloured by position along the same gradient, with the dB value on
  the right.
- **Deck** (`deck.rs`): track line across the top (cut off with `…`), the record
  centred in the rows between, then a control block centred under the record
  (at least 42 columns, wider if the volume line needs it) with progress, volume
  plus `33⅓ rpm · <format>`, and the audio source line.
- **Stats** (`stats.rs`): cpu, mem, gpu (two rows per GPU, labelled `gpu1`, `gpu2`),
  net (rate right-aligned to the panel width), proc.
- **Top panel / footer** (`chrome.rs`): `◆ user@host`, then battery and fps on the
  right, falling back to shorter forms when narrow; footer notices last 3 s.

### The record

The record is a braille disc defined in the mock's units (80×80 dots, radius 31):
rim, grooves every 3 dots, two rotating "sheen" wedges that make the spin visible,
a gap, a label with a rotating notch, and a spindle hole. Smaller decks scale the
radii; grooves stay 3 dots apart because scaling them aliases.

Computing every dot each frame (an `atan2` per dot) cost 0.3–0.6 ms. Instead,
`Geometry` is built once per size: dots that never change with rotation (rim,
groove lines, label centre) go into a fixed bitmap, and only the dots the sheen
wedges or the notch can switch are kept with their base angle. A frame copies the
bitmap and tests just those dots; while the record is stopped, the last rendered
rows are reused. `deck::tests` checks the cached render matches the direct formula
exactly.

## Theme

Colours come from `theme::th()`, a global `OnceLock<Theme>` installed at startup
(the default theme until then, which is what tests see). Drawing code never uses
literal colours. See [configuration.md](configuration.md#themes).

## Terminal output

Writing to a Windows console is synchronous: the call returns only after the
console host has parsed the escape codes. That shapes the output path:

- **Fixed viewport.** ratatui's fullscreen viewport asks the console for its size
  on every `draw`, a round trip that cost ~235 µs per frame. tide uses
  `Viewport::Fixed` and calls `term.resize()` only on `Event::Resize`.
- **One write per frame.** `FrameWriter` wraps a 64 KiB `BufWriter` and ignores
  ratatui's flush until a gate is opened, so the frame and the end-of-update marker
  leave in a single write.
- **Synchronized updates.** Each frame is wrapped in `BeginSynchronizedUpdate` /
  `EndSynchronizedUpdate`, so terminals that support it never show a half-drawn
  frame.
- **Diffing.** ratatui only emits cells that changed; a typical animated frame is
  1.1–1.6 KB of output.
- **Idle rate.** With nothing moving the loop redraws at `idle_fps` (10).

### Measured cost

Live, music playing, 120×30 console (`--profile`): about 10% of one core in
total (0.6% of a 16-thread machine). Per frame: layout ~30 µs, data ~130 µs,
draw + write ~1.1 ms, of which building the frame is ~0.35–0.5 ms and the rest is
the console write. `--bench 165x42` breaks down the build per panel (record
~16 µs, spectrum ~80 µs, other panels 10–20 µs each).
