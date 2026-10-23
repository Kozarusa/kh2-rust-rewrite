//! The enemy kind `M_EX020_RAW`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX020_RAW` (object 1365 = 0x555, `ObjectType` 4; `00objentry` row 1365: flags 4, `NeoStatus` 1001, Weight 100, `SpawnLimiter` 4, `AnimationName` `M_EX020_RAW.mset`), the Shadow of the areas the area survey lists (bb01, dc02, eh02, hb16, lk03, mu05) with a motion set of its own.
/// `obj/M_EX020_RAW.mdlx` is the same size as `obj/M_EX020.mdlx` and `kh2 bdx` lists the same 5333-word script for both, so its `set_obj_param` row is the Shadow's `enmp` 34 (and its row has the Shadow's `NeoStatus` 1001). Only the motion set (4.1 MB, against the Shadow's 365 KB) differs.
pub const KIND: Kind = Kind { name: "M_EX020_RAW", enmp: 34, model: "M_EX020_RAW", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX020_RAW` (object 1365) is an enemy kind of the app with the Shadow's `NeoStatus` 1001 and script (so `enmp` 34) and a motion set of its own; bb01's `btl` program 0x44 loads b_40 with sixteen of it and nothing else.
    /// Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex020_raw_is_an_enemy_kind_with_the_shadows_script() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (kinds::m_ex020::KIND.enmp, "M_EX020_RAW", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX020_RAW").expect("objentry row of M_EX020_RAW");
        let shadow = rows.iter().find(|r| r.model_name == "M_EX020").expect("objentry row of M_EX020");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_limiter, row.animation_name.as_str()), (1365, OBJECT_ENEMY, shadow.neo_status, 4, "M_EX020_RAW.mset"));
        assert_eq!(disc.read("obj/M_EX020_RAW.mdlx").map(|d| d.len()), disc.read("obj/M_EX020.mdlx").map(|d| d.len()));
        let groups = area_groups(&disc, "bb01");
        let ard = disc.read("ard/bb01.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("bb01.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 0x44, 0], 0);
        let b40 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_40").expect("program 0x44 loads b_40");
        assert_eq!(b40.members.iter().filter(|m| m.kind.name == KIND.name).count(), 16);
        assert!(b40.skipped.is_empty());
    }
}
