//! The enemy kind `M_EX850`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX850` (object 0x6bf, `ObjectType` 4), the object `M_EX940` makes with host 1.0 when a blow-class hit lands (BDX
/// `M_EX940` R0082 at 0x0082: `create_object(1727)`, the `SpawnObject1` of its objentry row). It is nobody's group
/// member: the script places it at the ground point under `M_EX940` (1.98, 1.148) and it runs its own script (3101 words;
/// modes `appear`, `idle`, `idle_time`, `freeze`, `event` ..; its event-11 handler answers reaction commands 481, 528 and
/// 586). Its script never calls `set_obj_param` and its objentry row has `NeoStatus` 0, so nothing builds a status block
/// (FUN_001b2148 is the only builder) and `Kind::status` is false: it cannot be hurt. `enmp` is only a stand-in for the
/// numbers `spawn_enemy` asks of the data (any row: HP only has to be above 0) [inferred].
pub const KIND: Kind = Kind { name: "M_EX850", enmp: 49, model: "M_EX850", death_effect: false, status: false };
