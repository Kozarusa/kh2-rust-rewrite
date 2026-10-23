//! The enemy kind `M_EX930`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// The Nobody `M_EX930` (object 0x138), with two Dusks in tt04's battle group b_21; "Dancer" is
/// inferred from its mode names (`atk_force_dance`, `atk_spin_*`, `atk_kick`). Its script hands
/// `enmp` row 48 to `set_obj_param` (BDX `M_EX930` R0f13 at 0x0f3a, the constant pushed at 0x04ce). Its `dead` mode (R06bd) starts element 2 of its effect file, fades out and hides, like
/// the Dusk's.
pub const KIND: Kind = Kind { name: "Dancer", enmp: 48, model: "M_EX930", death_effect: true, status: true };
