//! kh2-rs game player: Sora, abilities, drive forms, magic, clashes, prizes and the player's reactions.
//! It re-exports the lower game crates (`pub use kh2_game_core::*;` and so on), so `crate::assets::Disc` and the other
//! paths resolve the same as in every game crate.

pub use kh2_game_core::*;
pub use kh2_sound::*;
pub use kh2_enemy::*;

pub mod abilities;
pub mod clash;
pub mod controller_rules;
pub mod drive;
pub mod magic;
pub mod player_reaction;
pub mod prize;
pub mod sora;
