//! The enemy kind `M_EX660`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX660` (`ObjectType` 4, a Heartless of 11 areas: the area survey lists al14, ca14, ca15, ca20, he09, lk08), called by its model.
/// Its script hands `enmp` row 38 to `set_obj_param` (host 2.9: BDX `M_EX660` R1a7e at 0x1a7e, called at 0x080d with 38 as its eighth
/// word, pushed at 0x07f5; host 2.9 at 0x1aa5).
pub const KIND: Kind = Kind { name: "M_EX660", enmp: 38, model: "M_EX660", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX660` (object 304) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1020, Weight 10 and no spawn
    /// objects, its script's `set_obj_param` row is `enmp` 38, and al14's `btl` program 0x56 loads b_61 with six of it (beside five
    /// kinds the app lacks). Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex660_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (38, "M_EX660", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX660").expect("objentry row of M_EX660");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (304, OBJECT_ENEMY, 1020, 0, 7));
        let groups = area_groups(&disc, "al14");
        let ard = disc.read("ard/al14.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("al14.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 0x56, 0], 0);
        let b61 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_61").expect("program 0x56 loads b_61");
        assert_eq!(b61.members.iter().filter(|m| m.kind.name == KIND.name).count(), 6);
    }
}
