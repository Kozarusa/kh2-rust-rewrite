//! The enemy kind `M_EX120`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX120` (object 72, `ObjectType` 4; `00objentry` row 72: `NeoStatus` 1035, flags 8, Weight 300, `SpawnLimiter` 9, no spawn objects; a Heartless of the areas al01, al07, al12, al13, mu01, mu03), called by its model.
/// Its script hands `enmp` row 1 to `set_obj_param` (host 2.9: BDX `M_EX120` R0ef4 at 0x0ef4, called at 0x03d8 with 1 as the first of its eight
/// values after the object, pushed at 0x03c0, which R0ef4 stores to local+0x20; host 2.9 at 0x0f1b); its `dead` mode plays the death effect, fades and hides in play (soak below in docs/systems/script-host/contract.md).
pub const KIND: Kind = Kind { name: "M_EX120", enmp: 1, model: "M_EX120", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX120` (object 72) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1035, `SpawnLimiter` 9 and no spawn objects, its script's `set_obj_param` row is `enmp` 1, and mu01's
    /// `btl` program 0xa loads b_20 with four of it beside two `M_EX420` and two `M_EX210` (all kinds of the app). Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex120_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (1, "M_EX120", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX120").expect("objentry row of M_EX120");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_object2, row.spawn_limiter), (72, OBJECT_ENEMY, 1035, 0, 0, 9));
        let groups = area_groups(&disc, "mu01");
        let ard = disc.read("ard/mu01.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("mu01.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 10, 0], 0);
        let b20 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_20").expect("program 0xa loads b_20");
        assert_eq!(b20.members.iter().filter(|m| m.kind.name == KIND.name).count(), 4);
        assert!(b20.skipped.is_empty());
    }
}
