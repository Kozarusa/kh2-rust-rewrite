//! The enemy kind `M_EX630`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX630` (object 12, `ObjectType` 4; `00objentry` row 12: `NeoStatus` 1017, `SpawnLimiter` 15, no spawn objects), called by its model; a Heartless of the Agrabah areas (al03 b_01 holds ten).
/// Its script hands `enmp` row 63 to `set_obj_param` (host 2.9: BDX `M_EX630` R1396 at 0x1396, called at 0x0595 with 63 as the first of its eight values after the
/// object, pushed at 0x057d, which R1396 stores to local+0x20; host 2.9 at 0x13bd).
pub const KIND: Kind = Kind { name: "M_EX630", enmp: 63, model: "M_EX630", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX630` (object 12) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1017 and `SpawnLimiter` 15, its script's `set_obj_param` row is `enmp` 63, and al03's
    /// `btl` program 2 loads b_01 with ten of it. Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex630_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (63, "M_EX630", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX630").expect("objentry row of M_EX630");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_object2, row.spawn_limiter), (12, OBJECT_ENEMY, 1017, 0, 0, 15));
        let groups = area_groups(&disc, "al03");
        let ard = disc.read("ard/al03.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("al03.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 2, 0], 0);
        let b01 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_01").expect("program 2 loads b_01");
        assert_eq!(b01.members.iter().filter(|m| m.kind.name == KIND.name).count(), 10);
    }
}
