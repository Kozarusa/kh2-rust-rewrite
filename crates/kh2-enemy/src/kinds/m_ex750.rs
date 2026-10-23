//! The enemy kind `M_EX750`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX750` (`ObjectType` 4, object 125, `NeoStatus` 1045, `SpawnLimiter` 13, no spawn objects; a Heartless of the areas he05, he07 ..), called by its model.
/// Its script hands `enmp` row 14 to `set_obj_param` (host 2.9: BDX `M_EX750` R14f0 at 0x14f0, called at 0x09b9 with 14 as the first of its eight values after the object,
/// pushed at 0x09a1, which R14f0 stores to local+0x20; host 2.9 at 0x1517).
pub const KIND: Kind = Kind { name: "M_EX750", enmp: 14, model: "M_EX750", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX750` (object 125) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1045, `SpawnLimiter` 13 and no spawn objects, its script's `set_obj_param` row is `enmp` 14, and he05's
    /// `btl` program 2 loads b_01 with four of it beside three `M_EX050`. Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex750_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (14, "M_EX750", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX750").expect("objentry row of M_EX750");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (125, OBJECT_ENEMY, 1045, 0, 13));
        let groups = area_groups(&disc, "he05");
        let ard = disc.read("ard/he05.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("he05.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 2, 0], 0);
        let b01 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_01").expect("program 2 loads b_01");
        assert_eq!(b01.members.iter().filter(|m| m.kind.name == KIND.name).count(), 4);
        assert_eq!(b01.members.iter().filter(|m| m.kind.name == kinds::m_ex050::KIND.name).count(), 3);
        assert!(b01.skipped.is_empty());
    }
}
