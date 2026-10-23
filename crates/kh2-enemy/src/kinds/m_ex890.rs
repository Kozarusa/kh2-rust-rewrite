//! The enemy kind `M_EX890`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX890` (object 308, `ObjectType` 21; `00objentry` row 308: `NeoStatus` 3003), called by its model; the next kind of docs/systems/script-host/area-survey.md (tt29 `KH2_PROGRAMS=7` group b_21 holds four).
/// Its script (6901 words) hands `enmp` row 44 to `set_obj_param` (host 2.9: BDX `M_EX890` R18e1 at 0x18e1, called at 0x0b91 with 44 as the first of its eight values after the object, pushed at 0x0b79; host 2.9 at 0x1908).
pub const KIND: Kind = Kind { name: "M_EX890", enmp: 44, model: "M_EX890", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX890` (object 308) is an enemy kind of the app: its `00objentry` row is type 21 with `NeoStatus` 3003, its script's `set_obj_param` row is `enmp` 44. Needs `KH2_ISO` for the table part.
    #[test]
    fn m_ex890_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (44, "M_EX890", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX890").expect("objentry row of M_EX890");
        assert_eq!((row.object_id, row.object_type, row.neo_status), (308, 21, 3003));
    }
}
