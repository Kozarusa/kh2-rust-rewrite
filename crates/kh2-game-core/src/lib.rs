//! kh2-rs game core: the lowest of the game crates. It holds the schedule, the disc assets and the
//! modules every feature crate reads, and the shared ECS types (`shared`) the feature crates exchange.
//! The game crates re-export it (`pub use kh2_game_core::*;`), so `crate::assets::Disc` and the other
//! paths resolve the same in every crate.

pub mod area;
pub mod assets;
pub mod camera;
pub mod camera_blockers;
pub mod combat;
pub mod data;
pub mod fx;
pub mod harness;
pub mod hud_gfx;
pub mod hud_text;
pub mod magic_rules;
pub mod motor;
pub mod party_rules;
pub mod progress;
pub mod progress_flags;
pub mod reaction_parts;
pub mod reaction_rules;
pub mod rig;
pub mod rig_values;
pub mod rng;
pub mod schedule;
pub mod shared;
pub mod skel;
pub mod texanim;
pub mod world;

use std::sync::OnceLock;
use std::time::Instant;

/// KH2 units are centimetres; Bevy scene units are metres.
pub const SCALE: f32 = 0.01;

static CLOCK: OnceLock<Instant> = OnceLock::new();

/// Starts the startup clock (`main` calls it first; `startup_log` measures from it).
pub fn start_clock() {
    CLOCK.get_or_init(Instant::now);
}

/// Prints a startup timing line: time since the clock started in `main`.
pub fn startup_log(stage: &str) {
    eprintln!("[startup] {stage} {:?}", CLOCK.get_or_init(Instant::now).elapsed());
}
