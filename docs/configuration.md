# Configuration and themes

`src/config.rs` and `src/theme.rs`. User-facing docs (keys and examples) are in
the [top-level README](../README.md#configuration); this covers how loading works
and how to extend it.

## Loading order

`main::load_settings`:

1. **Built-in defaults** (`impl Default` for each config struct).
2. **Config file:** `--config PATH` (must exist), else `%APPDATA%\tide\config.toml`
   if it exists, else none.
3. **Command-line overrides:** currently `--fps`.
4. **Theme:** resolved from `cfg.theme` and installed globally.

Warnings (unknown keys in either file) are printed to stderr before the terminal
switches to the alternate screen, so they're readable after tide exits. Errors
abort startup with exit code 2.

## Parsing (`config::parse`)

- Files are read with `read_text`: UTF-8 with or without a byte-order mark, or
  UTF-16 LE/BE with a BOM (what Windows PowerShell 5.1's `>` writes).
- `toml::Deserializer` keeps source spans, so both syntax errors and type errors
  report the line and column.
- `serde_ignored` collects keys the structs don't know; they become sorted
  warnings like `unknown key `audio.fft` (ignored)`.
- Every struct is `#[serde(default)]`, so any key can be left out.
- `Config::validate` checks ranges after parsing and reports **all** problems at
  once, e.g. `fps must be 1–240 (got 0)`.

`StatsCfg::gpus` has a hand-written `Deserialize`: a string is a kind (`"all"`,
`"discrete"`, `"integrated"`) or a single name; an array is a list of names.

## `--print-config`

`config::default_toml()` is a hand-written template with every key, a comment,
and the default values formatted in. The `printed_defaults_round_trip` test parses
it back and requires it to equal `Config::default()` with no unknown keys, so the
template can't drift from the code.

## Adding a setting

1. Add the field to the right struct (`Config`, `AudioCfg`, `SpectrumCfg`,
   `DeckCfg`, `MediaCfg` or `StatsCfg`) with a doc comment.
2. Set its default in that struct's `impl Default`.
3. If it has a valid range, add a `check(...)` in `Config::validate`.
4. Add it to the `default_toml()` template with a comment, and pass its value in
   the `format!` arguments. Enums print through `lower()`.
5. Read it where it's used: drawing code through `app.cfg`, threads through the
   config slice they were spawned with (`AudioCfg`, `StatsCfg`, `MediaCfg`).
6. If users would care, add it to the README's settings table.

`cargo test` catches a template that doesn't round-trip.

## Themes

`theme::load(name, config_dir)` resolves `theme`:

| Value | Loads |
| --- | --- |
| `"tide"` | the built-in `Theme::default()` (the design tokens) |
| ends in `.toml` or contains `/` or `\` | that file; relative paths are relative to the config file's folder |
| anything else | `%APPDATA%\tide\themes\<name>.toml` |

A theme file is `ThemeFile`: every colour optional, `"#RRGGBB"` strings, applied
over the built-in theme. `viz` needs at least two stops. Unknown keys warn like
config keys; a bad colour is an error naming the file.

The result is installed once into a `OnceLock`; `th()` returns it everywhere.
There's no live theme switching.

`theme::grad(stops, f)` interpolates the `viz` gradient. Like the mock, it
quantises `f` to 1/20 steps, which also keeps the number of distinct colours per
frame (and so escape codes) low.

### Adding a colour

1. Add the field to `Theme` with a doc comment saying where it's used.
2. Give it a default in `impl Default for Theme`.
3. Add `Option<Hex>` to `ThemeFile` and the field name to the `set!` list in
   `ThemeFile::apply`.
4. Use it through `th().<name>` in the UI.
5. Add it to the README's theme key list.
