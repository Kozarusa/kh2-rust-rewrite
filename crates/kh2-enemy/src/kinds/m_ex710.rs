//! The enemy kind `M_EX710`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX710` (object 122 = 0x7a, `ObjectType` 21; `00objentry` row 122: `NeoStatus` 1025), called by its model; the widest kind of docs/systems/script-host/area-survey.md (28 areas; tt39 `KH2_PROGRAMS=4` group b_10, kind-2 group 156 with two).
/// Its script (6782 words) hands `enmp` row 11 to `set_obj_param` (host 2.9; survey recipe, `cargo run -p kh2 -- bdx M_EX710`).
pub const KIND: Kind = Kind { name: "M_EX710", enmp: 11, model: "M_EX710", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX710` (object 122) is an enemy kind of the app: its `00objentry` row is type 21 with `NeoStatus` 1025, its script's `set_obj_param` row is `enmp` 11. Needs `KH2_ISO` for the table part.
    #[test]
    fn m_ex710_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (11, "M_EX710", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX710").expect("objentry row of M_EX710");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_object2), (122, 21, 1025, 0, 0));
    }
}
