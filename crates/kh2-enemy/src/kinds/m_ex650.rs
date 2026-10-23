//! The enemy kind `M_EX650`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX650` (object 14, `ObjectType` 4; `00objentry` row 14: `NeoStatus` 1019, flags 0, Weight 100, `SpawnLimiter` 10, no spawn objects),
/// a Heartless of 13 areas (the area survey: ca01, ca02, ca03, ca13, ca18, ca19 ..), called by its model. Its script hands `enmp` row
/// 65 to `set_obj_param` (host 2.9: BDX `M_EX650` R14e1 at 0x1508, its eighth word, 65, pushed at 0x0c62 by the call at 0x0c7a); its
/// `dead` mode (registered at 0x096e, routine R0e2d -> R0e34) hands +0x1c to host 1.92, starts element 2 of its effect file, releases
/// it, fades out and hides, like the Dusk's.
pub const KIND: Kind = Kind { name: "M_EX650", enmp: 65, model: "M_EX650", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX650` (object 14) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1019 and no spawn
    /// objects, its script's `set_obj_param` row is `enmp` 65, and ca03's `btl` program 0x38 loads b_40 with four of it and no
    /// other kind the app lacks beside four `M_EX210`. Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex650_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (65, "M_EX650", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX650").expect("objentry row of M_EX650");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1), (14, OBJECT_ENEMY, 1019, 0));
        let groups = area_groups(&disc, "ca03");
        let ard = disc.read("ard/ca03.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("ca03.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 0x38, 0], 0);
        let b40 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_40").expect("program 0x38 loads b_40");
        assert_eq!(b40.members.iter().filter(|m| m.kind.name == KIND.name).count(), 4);
    }
}
