//! The enemy kind `M_EX940`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX940` (object 0x139, `ObjectType` 0x15: the second enemy class, `FUN_0016e8c0` -> `FUN_001c1ec8`), four of
/// them in tt04's b_44, called by its model. Its BDX script (5159 words; modes `atk_brush`, `atk_jump`, `atk_throw`,
/// `atk_fullpower`, `mode_battle_boss`) hands `enmp` row 49 to `set_obj_param` (host 2.9, constant 49 pushed at
/// 0x06e2 for `R124c`); its `dead` mode (R08db) starts element 2 of its effect file, fades out and hides.
pub const KIND: Kind = Kind { name: "M_EX940", enmp: 49, model: "M_EX940", death_effect: true, status: true };
