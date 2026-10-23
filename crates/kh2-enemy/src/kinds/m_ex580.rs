//! The enemy kind `M_EX580`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX580` (object 10; `00objentry` row 10: `NeoStatus` 1012), called by its model; the next kind of docs/systems/script-host/area-survey.md (7 areas, 24 groups; mu01 `KH2_PROGRAMS=1` group b_00 holds one beside three `M_EX020`).
/// Its script (4331 words) hands `enmp` row 61 to `set_obj_param` (host 2.9; BDX addresses in docs/systems/script-host/contract.md, WP-EN12e).
pub const KIND: Kind = Kind { name: "M_EX580", enmp: 61, model: "M_EX580", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX580` (object 10) is an enemy kind of the app: its `00objentry` row has `NeoStatus` 1012, its script's `set_obj_param` row is `enmp` 61. Needs `KH2_ISO` for the table part.
    #[test]
    fn m_ex580_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (61, "M_EX580", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX580").expect("objentry row of M_EX580");
        assert_eq!((row.object_id, row.neo_status), (10, 1012));
    }
}
