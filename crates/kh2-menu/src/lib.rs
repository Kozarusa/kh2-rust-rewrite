//! kh2-rs game menus: the pause menu and its pages, the journal, shops, game over, the ending report, the Struggle menu and the party switch notice.
//! It re-exports the lower game crates (`pub use kh2_game_core::*;` and so on), so `crate::assets::Disc` and the other
//! paths resolve the same as in every game crate.

pub use kh2_game_core::*;
pub use kh2_sound::*;
pub use kh2_enemy::*;
pub use kh2_player::*;
pub use kh2_party::*;
pub use kh2_event::*;
pub use kh2_hud::*;

pub mod menu;
pub mod menu_abilities;
pub mod menu_ending;
pub mod menu_gameover;
pub mod menu_gfx;
pub mod menu_journal;
pub mod menu_pages;
pub mod menu_shop;
pub mod menu_status;
pub mod menu_struggle;
pub mod menu_text;
pub mod party_switch;
