//! kh2-rig: the diff rig. Original (PCSX2) versus port, by machine.
//!
//! * [`p2s`] and [`save`]: read a PCSX2 save state and turn it into a `KH2_STATE` file (`kh2 state from-p2s`).
//! * [`pad`]: the pad script (`.pad`), one text format for both sides, keyed on the vsync counter.
//! * [`scenario`]: scenario files `scenarios/<system>/<name>.toml` and the value list `scenarios/values.toml`.
//! * [`client`]: the PCSX2 rig client (debug server + PINE): launch the patched PCSX2-MCP with a state, stop at the pad read of every
//!   game frame, inject the pad bytes, read the values, write the trace. Also the `kh2-rig` binary.
//! * [`trace`]: the trace CSV both sides write.
//!
//! See `docs` in each module; the scenario format is described in [`scenario`].

pub mod cli;
pub mod client;
pub mod p2s;
pub mod pad;
pub mod save;
pub mod scenario;
pub mod trace;
