//! The enemy kind `M_EX610`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX610` (object 1114 = 0x45a, `ObjectType` 4; `00objentry` row 1114: `NeoStatus` 1015), called by its model; the next kind of docs/systems/script-host/area-survey.md (hb09, hb16, hb18, tr01, tr04, tr09).
/// Its script (7410 words) hands `enmp` row 101 to `set_obj_param` (host 2.9: BDX `M_EX610` R1b10 at 0x1b10, called at 0x1024 with 101 as the first of its eight values after the object, pushed at 0x100c, which R1b10 stores to local+0x20; host 2.9 at 0x1b37). It calls no host that the app lacks (survey).
pub const KIND: Kind = Kind { name: "M_EX610", enmp: 101, model: "M_EX610", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    #[test]
    fn m_ex610_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (101, "M_EX610", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX610").expect("objentry row of M_EX610");
        assert_eq!((row.object_id, row.object_type, row.neo_status), (1114, 4, 1015));
    }
}
