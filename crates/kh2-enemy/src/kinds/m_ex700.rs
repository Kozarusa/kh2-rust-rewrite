//! The enemy kind `M_EX700`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX700` (object 18 = 0x12, `ObjectType` 4; `00objentry` row 18: `NeoStatus` 1024, flags 0, `SpawnLimiter` 9), a Nobody of 9
/// areas (the area survey: he05, he07, he09, he11, tt13, tt25), called by its model. Its script hands `enmp` row 68 to
/// `set_obj_param` (host 2.9: BDX `M_EX700` R12d5 at 0x12d5, the constant pushed at 0x03fd by the call at 0x0415); its `dead` mode
/// (R05f0) starts element 2 of its effect file, releases it, fades out and hides, like the Dusk's.
pub const KIND: Kind = Kind { name: "M_EX700", enmp: 68, model: "M_EX700", death_effect: true, status: true };
