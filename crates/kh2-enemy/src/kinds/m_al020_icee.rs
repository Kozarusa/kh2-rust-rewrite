//! The enemy kind `M_AL020_ICEE`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_AL020_ICEE` (object 77, `ObjectType` 4; `00objentry` row 77: `NeoStatus` 1031, `SpawnLimiter` 7, no spawn objects), the ice twin of `M_AL020_FIRE` in the Agrabah areas (al01 `btl` program 2 b_01 holds thirteen, per EN10n).
/// Its script hands `enmp` row 6 to `set_obj_param` (host 2.9: BDX `M_AL020_ICEE` R1224 at 0x1224, called at 0x046d with 6 as the first of its eight values after the object, pushed at 0x0455; host 2.9 at 0x124b).
pub const KIND: Kind = Kind { name: "M_AL020_ICEE", enmp: 6, model: "M_AL020_ICEE", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_AL020_ICEE` (object 77) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1031 and `SpawnLimiter` 7, its script's `set_obj_param` row is `enmp` 6, and al01's
    /// `btl` program 2 loads b_01 with thirteen of it. Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_al020_icee_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (6, "M_AL020_ICEE", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_AL020_ICEE").expect("objentry row of M_AL020_ICEE");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_object2, row.spawn_limiter), (77, OBJECT_ENEMY, 1031, 0, 0, 7));
        let groups = area_groups(&disc, "al01");
        let ard = disc.read("ard/al01.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("al01.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 2, 0], 0);
        let b01 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_01").expect("program 2 loads b_01");
        assert_eq!(b01.members.iter().filter(|m| m.kind.name == KIND.name).count(), 13);
    }
}
