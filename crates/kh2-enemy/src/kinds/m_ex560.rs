//! The enemy kind `M_EX560`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX560` (`ObjectType` 4, object 8, `NeoStatus` 1010, `SpawnLimiter` 12, no spawn objects; a Heartless of the areas ca09 and bb12 ..), called by its model.
/// Its script hands `enmp` row 59 to `set_obj_param` (host 2.9: BDX `M_EX560` R1080 at 0x1080, called at 0x049f with 59 as the first of its eight
/// values after the object, pushed at 0x0487, which R1080 stores to local+0x20; host 2.9 at 0x10a7).
pub const KIND: Kind = Kind { name: "M_EX560", enmp: 59, model: "M_EX560", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX560` (object 8) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1010, `SpawnLimiter` 12 and no spawn objects, its script's `set_obj_param` row is `enmp` 59, and ca09's
    /// `btl` program 3 loads b_02 with three of it beside three `M_EX730`. Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex560_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (59, "M_EX560", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX560").expect("objentry row of M_EX560");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (8, OBJECT_ENEMY, 1010, 0, 12));
        let groups = area_groups(&disc, "ca09");
        let ard = disc.read("ard/ca09.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("ca09.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 3, 0], 0);
        let b02 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_02").expect("program 3 loads b_02");
        assert_eq!(b02.members.iter().filter(|m| m.kind.name == KIND.name).count(), 3);
    }
}
