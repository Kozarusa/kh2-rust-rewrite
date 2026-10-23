//! kh2-rs game sound: sound effects, voices and the background music.
//! It re-exports the lower game crates (`pub use kh2_game_core::*;` and so on), so `crate::assets::Disc` and the other
//! paths resolve the same as in every game crate.

pub use kh2_game_core::*;

pub mod audio;
pub mod audio_bgm;
