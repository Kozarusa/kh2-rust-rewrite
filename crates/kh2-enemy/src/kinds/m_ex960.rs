//! The enemy kind `M_EX960`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX960` (object 315 = 0x13b, `ObjectType` 4; `00objentry` row 315: flags 8, `NeoStatus` 3010, Weight 100, `SpawnLimiter` 0), the body `M_EX870` makes with host 1.0 (its `SpawnObject1`); no area group lists it. Its constructor R0dc5 (0x0dc5, called at 0x0539)
/// takes eight values after the object (pushed 0x0521..0x0536: 51, 28, 300.0, 500.0, 8.0, 4, 0.7, 0.35) and hands the first, 51, to `set_obj_param` (host 2.9 at 0x0dec): `enmp` row 51. The second, 28, is kept at +0x1c (the drop row of host 1.92).
pub const KIND: Kind = Kind { name: "M_EX960", enmp: 51, model: "M_EX960", death_effect: true, status: true };
