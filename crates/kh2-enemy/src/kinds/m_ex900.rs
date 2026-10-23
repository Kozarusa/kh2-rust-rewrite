//! The enemy kind `M_EX900`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX900` (spawned in 12 areas: eh11, eh25, he09, tt03, tt06, tt07, ..; tt03 b_40 holds one beside two Dusks), called by
/// its model. Its set-up routine R1ab8 (0x1ab8, one call at 0x08c8) hands `enmp` row 45 to `set_obj_param` (host 2.9, 0x1adf;
/// the 45 is pushed at 0x08af); its `dead` mode is registered at 0x037d [inferred: same death shape as the other kinds]. In `KINDS` since WP-EN12a: the dive's
/// invulnerability (script word +0x18) is cleared by event 17 code 2, delivered from the motion frame triggers (docs/systems/script-host/enemies-m-ex8-9.md "M_EX900").
pub const KIND: Kind = Kind { name: "M_EX900", enmp: 45, model: "M_EX900", death_effect: true, status: true };
