//! The enemy kind `M_EX510`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX510` (`ObjectType` 4, object 3, `NeoStatus` 1005, `SpawnLimiter` 10, no spawn objects; a Heartless of the areas ca01, ca10, ca13, ca14, ca18, ca19), called by its model.
/// Its script hands `enmp` row 55 to `set_obj_param` (host 2.9: BDX `M_EX510` R1016 at 0x1016, called at 0x049a with 55 as the first of its eight values after the object,
/// pushed at 0x0482, which R1016 stores to local+0x20; host 2.9 at 0x103d).
pub const KIND: Kind = Kind { name: "M_EX510", enmp: 55, model: "M_EX510", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX510` (object 3) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1005, `SpawnLimiter` 10 and no spawn objects, its script's `set_obj_param` row is `enmp` 55, and ca10's
    /// `btl` program 0xa loads b_20 with three of it and three `M_EX420`. Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex510_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (55, "M_EX510", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX510").expect("objentry row of M_EX510");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (3, OBJECT_ENEMY, 1005, 0, 10));
        let groups = area_groups(&disc, "ca10");
        let ard = disc.read("ard/ca10.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("ca10.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 0xa, 0], 0);
        let b20 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_20").expect("program 0xa loads b_20");
        assert_eq!(b20.members.iter().filter(|m| m.kind.name == KIND.name).count(), 3);
        assert_eq!(b20.members.iter().filter(|m| m.kind.name == "M_EX420").count(), 3);
        assert!(b20.skipped.is_empty());
    }
}
