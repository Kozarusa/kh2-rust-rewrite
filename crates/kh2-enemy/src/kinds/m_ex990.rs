//! The enemy kind `M_EX990`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// The Dusk. Its `enmp` row is the constant its script hands to `set_obj_param` (host 2.9, the
/// call that builds the status block from the row, FUN_001b2148): 41 (BDX `M_EX990` at 0x073c).
pub const KIND: Kind = Kind { name: "Dusk", enmp: 41, model: "M_EX990", death_effect: true, status: true };
