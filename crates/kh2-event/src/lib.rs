//! kh2-rs game events: the event player, townspeople, save points, the Struggle and the area jumps.
//! It re-exports the lower game crates (`pub use kh2_game_core::*;` and so on), so `crate::assets::Disc` and the other
//! paths resolve the same as in every game crate.

pub use kh2_game_core::*;
pub use kh2_sound::*;
pub use kh2_enemy::*;
pub use kh2_player::*;
pub use kh2_party::*;

pub mod area_jump;
pub mod intro;
pub mod npc;
pub mod savepoint;
pub mod struggle;
