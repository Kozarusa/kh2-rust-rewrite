//! kh2-rig: the diff rig. Original (PCSX2) versus port, by machine (local/PLAN.md section 6, local/specs/RIG.md).
//!
//! * [`p2s`] and [`save`]: read a PCSX2 save state and turn it into a `KH2_STATE` file (`kh2 state from-p2s`).
//! * [`pad`]: the pad script (`.pad`), one text format for both sides, keyed on the vsync counter.
//! * [`scenario`]: scenario files `scenarios/<system>/<name>.toml` and the value list `scenarios/values.toml` (the format is
//!   documented there).
//! * [`client`]: the PCSX2 rig client (debug server + PINE): launch the patched PCSX2-MCP with a state, stop at the pad read of every
//!   game frame, inject the pad bytes, read the values, write the trace.
//! * [`report`]: original versus port, joined by name with the scenario's tolerances (`kh2-rig compare`, used by `tools/rig.ps1`).
//! * [`trace`]: the trace CSV both sides write (`k,name,value,stop`).
//!
//! # State transfer
//!
//! `kh2 state from-p2s <file.p2s> <out.json>` writes `out.json`, a `KH2_STATE` file with exactly the keys `Progress::apply_json` reads
//! today (`block`, `munny`, `game_mode`, `record_bits`, `story_raw`, `world_points`, `world_state`; an unknown key makes the app drop the
//! whole file), and `out.rig.json` with the keys only the app side can take once RIG-3 teaches it: `save_block` (hex of 0x33e860,
//! 0xb4e0 bytes, which `Progress` splits by its own address table), `place` (world, area, entrance from 0x33e830), `sora` (position,
//! facing, object pointer), `rig` (vsync counter 0x33fe64, frame step, source name). `--with-rig-keys` merges both into one file.
//!
//! # Running a scenario on the original
//!
//! ```text
//! set KH2_ISO=C:\...\Kingdom Hearts II (USA).iso
//! kh2-rig check scenarios/movement/tt-run-06.toml          parse and validate, no emulator
//! kh2-rig run scenarios/movement/tt-run-06.toml --id P0E   writes local/rig/tt-run-06/trace-orig.csv and meta.txt
//! kh2-rig determinism scenarios/movement/tt-run-06.toml --unlimited --ram     two launches, traces and the whole EE RAM compared
//! ```
//!
//! Options: `--unlimited` (PCSX2 `-unlimited`: about 130 to 190 pad stops per second against 29 at normal speed), `--attach bp|load`
//! (default `bp`: the breakpoint is set first, then PINE slot 0 loads the state; `load` is the other order), `--out DIR`, `--id NAME`
//! (owner written into `local/pcsx2.lock`), `--lock-wait SECONDS`. The paths come from the environment: `KH2_ISO`, and optionally
//! `KH2_RIG_LOCAL`, `KH2_PCSX2_MCP`, `KH2_SSTATES`; the main checkout's `local` is found by walking up from the current directory.
//! The rig refuses to start when PCSX2's debug or PINE port already answers, and it waits for `local/pcsx2.lock` (a lock younger than two
//! hours is respected). The PINE slot 0 file in the sstates folder (`<id>.00.p2s`) is the rig's own working file; the numbered slots
//! are never written.

pub mod cli;
pub mod client;
pub mod p2s;
pub mod pad;
pub mod save;
pub mod report;
pub mod scenario;
pub mod trace;
