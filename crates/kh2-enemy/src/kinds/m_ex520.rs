//! The enemy kind `M_EX520`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX520` (`ObjectType` 4, object 4, a Heartless of 11 areas: the area survey lists al14, bb00, bb04, bb06, bb09, bb11), called by its model.
/// Its script hands `enmp` row 56 to `set_obj_param` (host 2.9: BDX `M_EX520` R1781 at 0x1781, called at 0x0669 with 56 as its first pushed
/// word, pushed at 0x0651, which R1781 stores to local+0x20; host 2.9 at 0x17a8).
pub const KIND: Kind = Kind { name: "M_EX520", enmp: 56, model: "M_EX520", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX520` (object 4) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1006, Weight 100 and no spawn
    /// objects, its script's `set_obj_param` row is `enmp` 56, and al14's `btl` program 0x56 loads b_61 with three of it (beside
    /// `M_EX660` and `M_EX620`). Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex520_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (56, "M_EX520", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX520").expect("objentry row of M_EX520");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (4, OBJECT_ENEMY, 1006, 0, 8));
        let groups = area_groups(&disc, "al14");
        let ard = disc.read("ard/al14.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("al14.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 0x56, 0], 0);
        let b61 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_61").expect("program 0x56 loads b_61");
        assert_eq!(b61.members.iter().filter(|m| m.kind.name == KIND.name).count(), 3);
    }
}
