//! The enemy kind `M_EX210`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX210` (`ObjectType` 4, a Heartless of 11 areas: the area survey lists ca02, ca03, ca09, ca12, ca13, ca19), called by its model.
/// Its script hands `enmp` row 3 to `set_obj_param` (host 2.9: BDX `M_EX210` R18d6 at 0x18d6, called at 0x08fa with 3 as the first of its eight
/// values after the object, pushed at 0x08e2, which R18d6 stores to local+0x20; host 2.9 at 0x18fd).
pub const KIND: Kind = Kind { name: "M_EX210", enmp: 3, model: "M_EX210", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX210` (object 74) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1043, `SpawnLimiter` 13 and no spawn objects, its script's `set_obj_param` row is `enmp` 3, and ca03's
    /// `btl` program 0x38 loads b_40 with four of it (beside four `M_EX650`). Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex210_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (3, "M_EX210", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX210").expect("objentry row of M_EX210");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (74, OBJECT_ENEMY, 1043, 0, 13));
        let groups = area_groups(&disc, "ca03");
        let ard = disc.read("ard/ca03.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("ca03.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 0x38, 0], 0);
        let b40 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_40").expect("program 0x38 loads b_40");
        assert_eq!(b40.members.iter().filter(|m| m.kind.name == KIND.name).count(), 4);
    }
}
