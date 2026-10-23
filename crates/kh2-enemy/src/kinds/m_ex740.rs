//! The enemy kind `M_EX740`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX740` (object 124 = 0x7c, `ObjectType` 4; `00objentry` row 124: flags 1, `NeoStatus` 1044, `SpawnLimiter` 7, no spawn objects), called by its model; the next kind of docs/systems/script-host/area-survey.md (he16 `KH2_PROGRAMS=a` group b_20, kind-2 group 59 with three beside two `M_EX420`; he15 holds groups of 9 and 15).
/// Its script (5677 words) hands `enmp` row 13 to `set_obj_param` (host 2.9; survey recipe, `cargo run -p kh2 -- bdx M_EX740`).
pub const KIND: Kind = Kind { name: "M_EX740", enmp: 13, model: "M_EX740", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX740` (object 124) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1044 and `SpawnLimiter` 7, its script's `set_obj_param` row is `enmp` 13. Needs `KH2_ISO` for the table part.
    #[test]
    fn m_ex740_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (13, "M_EX740", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX740").expect("objentry row of M_EX740");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_object2, row.spawn_limiter), (124, 4, 1044, 0, 0, 7));
    }
}
