//! The enemy kind `M_AL020_FIRE_RAW`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_AL020_FIRE_RAW` (object 1670, `ObjectType` 4; `00objentry` row 1670: `NeoStatus` 1030, `SpawnLimiter` 4, `AnimationName` `M_AL020_FIRE_RAW.mset`), the fire Heartless of al10 and he09 (area survey: three groups, thirty members) with a motion set of its own.
/// `kh2 bdx` lists the same 4137-word script for it and `M_AL020_FIRE`, so its `set_obj_param` row is `enmp` 5.
pub const KIND: Kind = Kind { name: "M_AL020_FIRE_RAW", enmp: 5, model: "M_AL020_FIRE_RAW", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_AL020_FIRE_RAW` (object 1670) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1030 and a motion set of its own, its script is `M_AL020_FIRE`'s (so `enmp` 5); he09's `btl` program 0x25 loads b_9d with 3 of it and al10's program 0x3a loads b_40 with the rest of the survey's members.
    /// Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_al020_fire_raw_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (5, "M_AL020_FIRE_RAW", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_AL020_FIRE_RAW").expect("objentry row of M_AL020_FIRE_RAW");
        let base = rows.iter().find(|r| r.model_name == "M_AL020_FIRE").expect("objentry row of M_AL020_FIRE");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.animation_name.as_str()), (1670, OBJECT_ENEMY, base.neo_status, "M_AL020_FIRE_RAW.mset"));
        assert_eq!(row.neo_status, 1030);
        assert!(disc.read("obj/M_AL020_FIRE_RAW.mset").is_some(), "the kind has a motion set of its own");
        assert!(load_rig(&disc, "M_AL020_FIRE_RAW", &mset_name(Some(row.animation_name.as_str()), "M_AL020_FIRE_RAW")).is_some());
        let groups = area_groups(&disc, "he09");
        let ard = disc.read("ard/he09.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("he09.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 0x25, 0], 0);
        let g = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_9d").expect("the program loads the group");
        assert_eq!(g.members.iter().filter(|m| m.kind.name == KIND.name).count(), 3);
        assert!(g.skipped.is_empty());
        let groups = area_groups(&disc, "al10");
        let ard = disc.read("ard/al10.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("al10.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 0x3a, 0], 0);
        let b40 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_40").expect("program 0x3a loads b_40");
        assert_eq!(b40.members.iter().filter(|m| m.kind.name == KIND.name).count(), 26);
    }
}
