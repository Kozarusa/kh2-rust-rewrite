//! kh2-rs game enemies: the enemy kinds, their actors, the AI scripts and the script host.
//! It re-exports the lower game crates (`pub use kh2_game_core::*;` and so on), so `crate::assets::Disc` and the other
//! paths resolve the same as in every game crate.

pub use kh2_game_core::*;
pub use kh2_sound::*;

pub mod enemy;
pub mod enemy_ai;
pub mod enemy_host;
pub mod enemy_obj;
