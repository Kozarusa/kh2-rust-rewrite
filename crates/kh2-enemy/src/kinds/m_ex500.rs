//! The enemy kind `M_EX500`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX500` (object 2, `ObjectType` 4; `00objentry` row 2: `NeoStatus` 1004), called by its model; the next kind of docs/systems/script-host/area-survey.md (he11 `KH2_PROGRAMS=1` group b_00, kind-2 group 71 with three; five Coliseum areas).
/// Its script (7498 words) hands `enmp` row 54 to `set_obj_param` (host 2.9; survey recipe, `cargo run -p kh2 -- bdx M_EX500`); it needs host 1.125 tween_start, added as Approx.
pub const KIND: Kind = Kind { name: "M_EX500", enmp: 54, model: "M_EX500", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX500` (object 2) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1004, its script's `set_obj_param` row is `enmp` 54. Needs `KH2_ISO` for the table part.
    #[test]
    fn m_ex500_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (54, "M_EX500", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX500").expect("objentry row of M_EX500");
        assert_eq!((row.object_id, row.object_type, row.neo_status), (2, 4, 1004));
    }
}
