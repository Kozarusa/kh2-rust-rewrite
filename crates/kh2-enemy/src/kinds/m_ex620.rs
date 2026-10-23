//! The enemy kind `M_EX620`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX620` (`ObjectType` 4, object 11, `NeoStatus` 1016, `SpawnLimiter` 20, Flags 8, no spawn objects; a Heartless of the Agrabah areas al00, al01, al07, al10, al12, al13 and al14 ..), called by its model.
/// Its script hands `enmp` row 62 to `set_obj_param` (host 2.9: BDX `M_EX620` R2331 at 0x2331, called at 0x0f1a with 62 as the first of its eight
/// values after the object, pushed at 0x0f02, which R2331 stores to local+0x20; host 2.9 at 0x2358).
pub const KIND: Kind = Kind { name: "M_EX620", enmp: 62, model: "M_EX620", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX620` (object 11) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1016, `SpawnLimiter` 20 and no spawn objects, its script's `set_obj_param` row is `enmp` 62, and al01's
    /// `btl` program 2 loads b_01 with six of it beside two Soldiers (its `M_AL020_ICEE` are skipped: not in the app). Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex620_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (62, "M_EX620", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX620").expect("objentry row of M_EX620");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (11, OBJECT_ENEMY, 1016, 0, 20));
        let groups = area_groups(&disc, "al01");
        let ard = disc.read("ard/al01.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("al01.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 2, 0], 0);
        let b01 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_01").expect("program 2 loads b_01");
        assert_eq!(b01.members.iter().filter(|m| m.kind.name == KIND.name).count(), 6);
    }
}
