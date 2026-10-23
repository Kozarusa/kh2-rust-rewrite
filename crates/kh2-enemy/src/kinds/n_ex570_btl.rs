//! The enemy kind `N_EX570_BTL`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// The Struggle opponent `N_EX570_BTL` (object 0x678; Seifer by the community's model names), in
/// tt04's battle groups b_41, b_42, b_45 and b_51. Its script picks the `enmp` row it hands to
/// `set_obj_param` from spawn parameter 1 (BDX `N_EX570_BTL` R1864 at 0x077d): 155 for 3 and 4, else
/// 135. The kind starts with 135; host 2.9 rebuilds the stats from the row the script asks for. Its
/// `dead` mode (BDX 0x174d) plays motion 16 or 10 and waits for ever: no death effect, no fade.
pub const KIND: Kind = Kind { name: "Seifer", enmp: 135, model: "N_EX570_BTL", death_effect: false, status: true };
