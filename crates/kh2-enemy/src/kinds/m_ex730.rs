//! The enemy kind `M_EX730`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX730` (`ObjectType` 4, object 123, `NeoStatus` 1027, `SpawnLimiter` 10, no spawn objects; a Heartless of the areas ca01, ca09, ca12, ca13, ca18, ca21 ..), called by its model.
/// Its script hands `enmp` row 12 to `set_obj_param` (host 2.9: BDX `M_EX730` R1268 at 0x1268, called at 0x05ba with 12 as the first of its eight values after the object,
/// pushed at 0x05a2, which R1268 stores to local+0x20; host 2.9 at 0x128f).
pub const KIND: Kind = Kind { name: "M_EX730", enmp: 12, model: "M_EX730", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX730` (object 123) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1027, `SpawnLimiter` 10 and no spawn objects, its script's `set_obj_param` row is `enmp` 12, and ca09's
    /// `btl` program 3 loads b_02 with three of it (beside three `M_EX560`). Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex730_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (12, "M_EX730", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX730").expect("objentry row of M_EX730");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (123, OBJECT_ENEMY, 1027, 0, 10));
        let groups = area_groups(&disc, "ca09");
        let ard = disc.read("ard/ca09.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("ca09.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 3, 0], 0);
        let b02 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_02").expect("program 3 loads b_02");
        assert_eq!(b02.members.iter().filter(|m| m.kind.name == KIND.name).count(), 3);
        assert_eq!(b02.members.iter().filter(|m| m.kind.name == kinds::m_ex560::KIND.name).count(), 3);
        assert!(b02.skipped.is_empty());
    }
}
