//! The enemy kind `M_EX910`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX910` (object 310, `ObjectType` 4, a Heartless of 11 areas: the area survey lists eh08, eh12, eh16, hb08, hb09, he09), called by its model.
/// Its script hands `enmp` row 46 to `set_obj_param` (host 2.9: BDX `M_EX910` R1a85 at 0x1a85, called at 0x0cca with 46 as the first of its eight
/// values after the object, pushed at 0x0cb2, which R1a85 stores to local+0x20; host 2.9 at 0x1aac).
pub const KIND: Kind = Kind { name: "M_EX910", enmp: 46, model: "M_EX910", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX910` is an enemy kind of the app: its `00objentry` row (object 310) is type 4 with `NeoStatus` 3005, `SpawnLimiter` 20 and no spawn objects, its script's `set_obj_param` row is `enmp` 46, and hb09's
    /// `btl` program 0x33 loads b_40 with one of it (beside five Dusk). Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex910_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (46, "M_EX910", true, true));

        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX910").expect("objentry row of M_EX910");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (310, OBJECT_ENEMY, 3005, 0, 20));
        let groups = area_groups(&disc, "hb09");
        let ard = disc.read("ard/hb09.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("hb09.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 0x33, 0], 0);
        let b40 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_40").expect("program 0x33 loads b_40");
        assert_eq!(b40.members.iter().filter(|m| m.kind.name == KIND.name).count(), 1);
    }
}
