//! The enemy kind `N_EX500_BTL`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// The Struggle opponent `N_EX500_BTL` (object 0x4d7) of tt04's battle group b_50, called by its model
/// (its name is not in the data the app reads). Its script hands `enmp` row 150 to `set_obj_param`
/// (BDX `N_EX500_BTL` 0x0736). Unlike `N_EX570_BTL` its `dead` mode (R0900) starts element 2 of its
/// effect file, fades out and hides, as the Dusk's does.
pub const KIND: Kind = Kind { name: "N_EX500", enmp: 150, model: "N_EX500_BTL", death_effect: true, status: true };
