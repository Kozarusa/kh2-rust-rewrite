# METHOD: how kh2-rs is built

## The reference is the spec

kh2-rs does what the PS2 USA release (`SLUS_210.05`) does. Every format and behaviour comes from evidence:
- the bytes on the user's disc;
- community documentation (see `research.md`);
- decompiled behaviour, studied locally and **never committed**.

A deviation from the original is written down where it lives.

## The Spreadsheet Method

1. **Sheets are the source of truth.** `sheets/*.json`, grouped by kind. Columns are variables; rows are objects (a system, a binary table, …). When unsure about anything, read the sheet. If the sheet is wrong, fix the sheet first.
2. **Every row generates code**, called its *strut*. The sheet's `strut` field names the generator in `crates/kh2-sheets/src/strut.rs`. For example, a `tables.json` row becomes a Rust struct with a parser. Generated code is never edited by hand.
3. **Each cell is a checkbox.** `null` means unchecked, and a row with any unchecked cell is unfinished. A cell is filled when the fact is known (a layout) or proven (`validated` is written by `kh2 validate` after decoding from the disc). All cells must end up checked.
4. **Preflight before every compile.** `crates/kh2-tables/build.rs` runs `kh2_sheets::preflight`, which overlaps all sheets:
   - shape, duplicate keys and unknown struts → **error** (the build stops);
   - each `ref:`/`refs:` cell must name a real row of the target sheet → **error** if not, **gap** if that row is unfinished;
   - each `crate` cell must exist under `crates/` → **gap** if not created yet;
   - each `layout` must parse, and every strut must render → **error**;
   - unidentified fields (`Unk*`) → **warning**.

   `KH2_STRICT=1 cargo build` also stops on any unfinished row or gap. `cargo run -p kh2 -- preflight` prints the grid.
5. **Sheets hold structure and status, never game data.** Values such as HP or damage are read from the disc at runtime.

## Never in the repository

The ISO, files extracted from it, decompiled code, or the downloaded name list. The `.gitignore` is a whitelist, so anything not listed there is ignored by default.

## Gates for a change

`cargo build` (preflight), `cargo test`, `cargo clippy`, and `kh2 validate` against your disc when a layout changes.
