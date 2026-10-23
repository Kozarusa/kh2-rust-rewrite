//! The enemy kind `M_EX690`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX690` (`ObjectType` 4, object 17, `NeoStatus` 1023, `SpawnLimiter` 13, no spawn objects; a Heartless of the areas bb00, bb07, bb11, ca02, ca09, ca15 ..), called by its model.
/// Its script hands `enmp` row 67 to `set_obj_param` (host 2.9: BDX `M_EX690` R0fe9 at 0x0fe9, called at 0x05dd with 67 as the first of its eight values after the object,
/// pushed at 0x05c5, which R0fe9 stores to local+0x20; host 2.9 at 0x1010).
pub const KIND: Kind = Kind { name: "M_EX690", enmp: 67, model: "M_EX690", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX690` (object 17) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1023, `SpawnLimiter` 13 and no spawn objects, its script's `set_obj_param` row is `enmp` 67, and ca15's
    /// `btl` program 0x15 loads b_20 with three of it and nothing else. Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex690_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (67, "M_EX690", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX690").expect("objentry row of M_EX690");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (17, OBJECT_ENEMY, 1023, 0, 13));
        let groups = area_groups(&disc, "ca15");
        let ard = disc.read("ard/ca15.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("ca15.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 0x15, 0], 0);
        let b20 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_20").expect("program 0x15 loads b_20");
        assert_eq!(b20.members.len(), 3);
        assert!(b20.members.iter().all(|m| m.kind.name == KIND.name));
    }
}
