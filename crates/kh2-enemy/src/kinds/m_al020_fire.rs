//! The enemy kind `M_AL020_FIRE`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_AL020_FIRE` (object 76, `ObjectType` 4; `00objentry` row 76: `NeoStatus` 1030, `SpawnLimiter` 6, no spawn objects), called by its model; a Heartless of the Agrabah areas (al00 `btl` programs 2 and 0xa hold thirteen to a dozen per group, per the area survey).
/// Its script hands `enmp` row 5 to `set_obj_param` (host 2.9: BDX `M_AL020_FIRE` R0e42 at 0x0e42, called at 0x046d with 5 as the first of its eight values after the
/// object, pushed at 0x0455, which R0e42 stores to local+0x20; host 2.9 at 0x0e69).
pub const KIND: Kind = Kind { name: "M_AL020_FIRE", enmp: 5, model: "M_AL020_FIRE", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_AL020_FIRE` (object 76) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1030 and `SpawnLimiter` 6, its script's `set_obj_param` row is `enmp` 5, and al00's
    /// `btl` program 0xa loads b_20 with eighteen of it. Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_al020_fire_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (5, "M_AL020_FIRE", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_AL020_FIRE").expect("objentry row of M_AL020_FIRE");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_object2, row.spawn_limiter), (76, OBJECT_ENEMY, 1030, 0, 0, 6));
        let groups = area_groups(&disc, "al00");
        let ard = disc.read("ard/al00.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("al00.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 0xa, 0], 0);
        let b20 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_20").expect("program 0xa loads b_20");
        assert_eq!(b20.members.iter().filter(|m| m.kind.name == KIND.name).count(), 18);
    }
}
