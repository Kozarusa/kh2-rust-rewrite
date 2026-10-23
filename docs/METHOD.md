# METHOD: how kh2-rs is built

## The reference is the spec

kh2-rs does what the PS2 USA release (`SLUS_210.05`) does. Every format and behaviour comes from evidence:
- the bytes on the user's disc;
- community documentation (see `docs/systems/foundations/research.md`);
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

## The ladder (rule C1)

The game is built system by system, bottom of the dependency stack first (L1 core runtime, L2 player and camera, L3 combat and effects, L4 magic,
L5 drive/limits/summons, L6 party, L7 script host, L8 UI, L9 audio, L10 events and movies, L11 world mechanics and minigames, L12 world map and
Gummi). Exactly one step is open for code at a time; a second lane may write specs for the next step. A step closes through its exit gate: every
function of the system `verified` or `not-needed` in the ledger, no `stub`/`missing` host arm, its rig scenarios pass, its `DEVIATIONS.md` rows
fixed, its contract final. The index of systems, sheet ids, crates and doc files is `docs/README.md`.

## The function ledger (rule C2)

Progress is measured on the original's code: `sheets/functions.json` holds one row per function of the executable and its four overlays
(address, image, size, system, status, evidence, note; addresses and statuses only, no decompiled text, no community names). Statuses run
`unread`, `read` (described in a system doc), `ported` (Rust names the address in a doc comment), `verified` (an oracle test, a disc test or a rig
scenario proves it) and `not-needed` with a reason (`sdk`, `debug`, `hw`, `dead`, `fm`). `kh2 ledger scan` sets `read`/`ported`/`verified` from
addresses found in docs, doc comments and test names; nobody edits those by hand. Preflight refuses a closed system that still has `unread` or
`read` functions. The headline number is verified-or-not-needed bytes over total bytes. The ledger arrives with Phase 0 step R5.

## The diff rig (rule C3)

From L1 on every exit gate includes rig scenarios: the original (PCSX2 with PINE and Input Recording, later the headless reference runtime) and the
port are fed the same input, state is transferred at a frame-exact read point, and value traces are compared. Scenarios live in
`scenarios/<system>/` and name the values they compare. User play-tests are batched at gates and cover only what the rig cannot judge. The rig
arrives with Phase 0 step R10; until then the oracle tests (`kh2-ee` against the game's own routines) and the accept scenarios stand in.

## Documents

`docs/README.md` is the index. Each system folder under `docs/systems/` starts with `contract.md` (what the system does, its state with owners,
entry points with original addresses, open gaps, links to the detail files); the detail files are the evidence and status notes split by subject
(none above about 60 KB). A reference to a document names the file under `docs/systems/` and, where it matters, the section number or the quoted
heading. `docs/changelog/shift-NN.md` holds one file per shift; `docs/CHANGELOG.md` is the index.
