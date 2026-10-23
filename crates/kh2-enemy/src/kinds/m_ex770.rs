//! The enemy kind `M_EX770`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX770` (object 306, `ObjectType` 4, a Heartless of 11 areas: the area survey lists ca01, ca02, hb01, hb02, hb04, hb11), called by its model.
/// Its script hands `enmp` row 37 to `set_obj_param` (host 2.9: BDX `M_EX770` R1d23 at 0x1d23, called at 0x0db1 with 37 as its eighth
/// word, pushed at 0x0d99; host 2.9 at 0x1d4a).
pub const KIND: Kind = Kind { name: "M_EX770", enmp: 37, model: "M_EX770", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX770` (object 306) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1029, Weight 10 and no spawn
    /// objects, its script's `set_obj_param` row is `enmp` 37, and ca02's `btl` program 0xa loads b_20 with nine of it beside eight `M_EX420`.
    /// Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex770_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (37, "M_EX770", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX770").expect("objentry row of M_EX770");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (306, OBJECT_ENEMY, 1029, 0, 10));
        let groups = area_groups(&disc, "ca02");
        let ard = disc.read("ard/ca02.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("ca02.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 0xa, 0], 0);
        let b20 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_20").expect("program 0xa loads b_20");
        assert_eq!(b20.members.iter().filter(|m| m.kind.name == KIND.name).count(), 9);
        assert_eq!(b20.members.iter().filter(|m| m.kind.name == kinds::m_ex420::KIND.name).count(), 8);
    }
}
