//! The enemy kind `M_EX110`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX110` (object 71, `ObjectType` 4; `00objentry` row 71: `NeoStatus` 1034, flags 8, Weight 100, `SpawnLimiter` 8, no spawn objects),
/// a Heartless of 12 areas (the area survey: al00, al01, al03, al07, al10, al12 ..), called by its model. Its script hands `enmp` row 0
/// to `set_obj_param` (host 2.9: BDX `M_EX110` R19da at 0x19da, the first of its eight values after the object, pushed at 0x0ba8 by the
/// call at 0x0bc0 and passed on at 0x1a01); its `dead` mode hands +0x1c (47, stored by the same routine) to host 1.92, like the Dusk's.
/// It fades out and in on its own (`FadeIn` 1.20 in `move_idle`).
pub const KIND: Kind = Kind { name: "M_EX110", enmp: 0, model: "M_EX110", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX110` (object 71) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1034 and no spawn
    /// objects, its script's `set_obj_param` row is `enmp` 0, and al03's `btl` program 2 loads b_01 with five of it. Needs
    /// `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex110_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (0, "M_EX110", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX110").expect("objentry row of M_EX110");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1), (71, OBJECT_ENEMY, 1034, 0));
        let groups = area_groups(&disc, "al03");
        let ard = disc.read("ard/al03.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("al03.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 2, 0], 0);
        let b01 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_01").expect("program 2 loads b_01");
        assert_eq!(b01.members.iter().filter(|m| m.kind.name == KIND.name).count(), 5);
    }
}
