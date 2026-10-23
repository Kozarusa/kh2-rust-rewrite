//! The enemy kind `M_EX050`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX050` (`ObjectType` 4, object 303, `NeoStatus` 1002, `SpawnLimiter` 15, no spawn objects; a Heartless of the areas bb00, bb04, bb07, bb12, he05, he07), called by its model.
/// Its script hands `enmp` row 35 to `set_obj_param` (host 2.9: BDX `M_EX050` R13cd at 0x13cd, called at 0x079a with 35 as the first of its eight values after the object,
/// pushed at 0x0782, which R13cd stores to local+0x20; host 2.9 at 0x13f4).
pub const KIND: Kind = Kind { name: "M_EX050", enmp: 35, model: "M_EX050", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX050` (object 303) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1002, `SpawnLimiter` 15 and no spawn objects, its script's `set_obj_param` row is `enmp` 35, and bb04's
    /// `btl` program 0xa loads b_20 with three of it and nothing else. Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex050_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (35, "M_EX050", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX050").expect("objentry row of M_EX050");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (303, OBJECT_ENEMY, 1002, 0, 15));
        let groups = area_groups(&disc, "bb04");
        let ard = disc.read("ard/bb04.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("bb04.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 0xa, 0], 0);
        let b20 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_20").expect("program 0xa loads b_20");
        assert_eq!(b20.members.iter().filter(|m| m.kind.name == KIND.name).count(), 3);
        assert!(b20.members.iter().all(|m| m.kind.name == KIND.name || m.kind.name == kinds::m_ex130::KIND.name || m.kind.name == kinds::m_ex710::KIND.name), "the rest of b_20 is M_EX130 and M_EX710");
    }
}
