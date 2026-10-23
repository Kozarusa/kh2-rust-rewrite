//! The enemy kind `M_EX920`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX920` (`ObjectType` 4, object 311, `NeoStatus` 3006, `SpawnLimiter` 15, no spawn objects; a Heartless of the areas eh06, eh10, eh11, eh17, he09, mu10), called by its model.
/// Its script hands `enmp` row 47 to `set_obj_param` (host 2.9: BDX `M_EX920` R2683 at 0x2683, called at 0x1ab4 with 47 as the first of its eight values after the object,
/// pushed at 0x1a9c, which R2683 stores to local+0x20; host 2.9 at 0x26aa).
pub const KIND: Kind = Kind { name: "M_EX920", enmp: 47, model: "M_EX920", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX920` (object 311) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 3006, `SpawnLimiter` 15 and no spawn objects, its script's `set_obj_param` row is `enmp` 47, and eh10's
    /// `btl` program 1 loads b_00 with three of it. Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex920_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (47, "M_EX920", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX920").expect("objentry row of M_EX920");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (311, OBJECT_ENEMY, 3006, 0, 15));
        let groups = area_groups(&disc, "eh10");
        let ard = disc.read("ard/eh10.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("eh10.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 1, 0], 0);
        let b00 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_00").expect("program 1 loads b_00");
        assert_eq!(b00.members.iter().filter(|m| m.kind.name == KIND.name).count(), 3);
        assert!(b00.skipped.is_empty());
    }
}
