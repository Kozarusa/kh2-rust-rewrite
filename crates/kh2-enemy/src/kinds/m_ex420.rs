//! The enemy kind `M_EX420`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX420` (object 120, `ObjectType` 4; `00objentry` row 120: `NeoStatus` 1003, flags 4), spawned in 33 areas of the other
/// worlds (the area survey), called by its model. Its script hands `enmp` row 9 to `set_obj_param` (host 2.9: BDX `M_EX420`
/// R174d at 0x1774, the constant pushed at 0x063e by the call at 0x0656); its `dead` mode (R082e) starts element 2 of its
/// effect file, fades out and hides, like the Dusk's.
pub const KIND: Kind = Kind { name: "M_EX420", enmp: 9, model: "M_EX420", death_effect: true, status: true };
