//! The enemy kind `M_EX680`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX680` (object 16, `ObjectType` 21; `00objentry` row 16: flags 8, `NeoStatus` 1022, `SpawnLimiter` 17, no spawn objects), called by its model; the next kind of docs/systems/script-host/area-survey.md (10 areas; hb04 group b_20 holds one beside two `M_EX420`).
/// Its script (12124 words) hands `enmp` row 66 to `set_obj_param` (host 2.9: BDX `M_EX680` R2d91 at 0x2d91, called at 0x19f4 with 66 as the first of its eight values after the object, pushed at 0x19dc, which R2d91 stores to local+0x20; host 2.9 at 0x2db8).
pub const KIND: Kind = Kind { name: "M_EX680", enmp: 66, model: "M_EX680", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX680` (object 16) is an enemy kind of the app: its `00objentry` row is type 21 with `NeoStatus` 1022 and `SpawnLimiter` 17, its script's `set_obj_param` row is `enmp` 66. Needs `KH2_ISO` for the table part.
    #[test]
    fn m_ex680_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (66, "M_EX680", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX680").expect("objentry row of M_EX680");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_object2, row.spawn_limiter), (16, 21, 1022, 0, 0, 17));
    }
}
