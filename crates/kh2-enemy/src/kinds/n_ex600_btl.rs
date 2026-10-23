//! The enemy kind `N_EX600_BTL`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// The Struggle opponent `N_EX600_BTL` (object 0x82f) of tt04's battle group b_52, called by its model.
/// Its script hands `enmp` row 158 to `set_obj_param` (BDX `N_EX600_BTL` 0x07fd); its `dead` mode
/// (R09c8) starts element 2 of its effect file, fades out and hides.
pub const KIND: Kind = Kind { name: "N_EX600", enmp: 158, model: "N_EX600_BTL", death_effect: true, status: true };
