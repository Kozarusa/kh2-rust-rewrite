//! The enemy kind `M_EX880`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX880` (object 317 = 0x13d, `ObjectType` 4; `00objentry` row 317: `NeoStatus` 3011, flags 0, `SpawnLimiter` 9), a Nobody
/// of 13 areas of the other worlds (the area survey: eh12, eh25, hb08, hb18, he09, tt03), called by its model. Its script
/// hands `enmp` row 52 to `set_obj_param` (host 2.9: BDX `M_EX880` R1191 at 0x1191, the constant pushed at 0x04d3 by the
/// call at 0x04ec); its `dead` mode (R06c3) starts element 2 of its effect file, fades out and hides, like the Dusk's.
pub const KIND: Kind = Kind { name: "M_EX880", enmp: 52, model: "M_EX880", death_effect: true, status: true };
