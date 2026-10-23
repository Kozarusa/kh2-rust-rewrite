//! The enemy kind `M_EX600`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX600` (`ObjectType` 4, object 121, `NeoStatus` 1014, `SpawnLimiter` 10, Weight 200, no spawn objects; a Heartless of the areas hb02, hb11, hb19, he09, tr02 and tr03), called by its model.
/// Its script hands `enmp` row 10 to `set_obj_param` (host 2.9: BDX `M_EX600` R13af at 0x13af, called at 0x066a with 10 as the first of its eight values after the object,
/// pushed at 0x0652, which R13af stores to local+0x20; host 2.9 at 0x13d6).
pub const KIND: Kind = Kind { name: "M_EX600", enmp: 10, model: "M_EX600", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX600` (object 121) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1014, `SpawnLimiter` 10 and no spawn objects, and its script's `set_obj_param` row is `enmp` 10.
    /// Needs `KH2_ISO` for the table part.
    #[test]
    fn m_ex600_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (10, "M_EX600", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX600").expect("objentry row of M_EX600");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (121, OBJECT_ENEMY, 1014, 0, 10));
    }
}
