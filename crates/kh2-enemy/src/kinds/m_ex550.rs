//! The enemy kind `M_EX550`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX550` (object 7, `ObjectType` 4; `00objentry` row 7: flags 0, `NeoStatus` 1009, `SpawnLimiter` 9, no spawn objects), called by its model; the next kind of docs/systems/script-host/area-survey.md (he11 `KH2_PROGRAMS=a` group b_20, kind-2 group 84 with four; dc00, dc02, dc03 and he05 also spawn it).
/// Its script (7185 words) hands `enmp` row 58 to `set_obj_param` (host 2.9 at 0x1a02; `cargo run -p kh2 -- bdx M_EX550`).
pub const KIND: Kind = Kind { name: "M_EX550", enmp: 58, model: "M_EX550", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX550` (object 7) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1009 and `SpawnLimiter` 9, its script's `set_obj_param` row is `enmp` 58. Needs `KH2_ISO` for the table part.
    #[test]
    fn m_ex550_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (58, "M_EX550", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX550").expect("objentry row of M_EX550");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_object2, row.spawn_limiter), (7, 4, 1009, 0, 0, 9));
    }
}
