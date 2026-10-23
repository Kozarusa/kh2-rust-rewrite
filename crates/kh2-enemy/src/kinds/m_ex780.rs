//! The enemy kind `M_EX780`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX780` (object 20, `ObjectType` 4; `00objentry` row 20: `NeoStatus` 1039, `SpawnLimiter` 7, no spawn objects), called by its model; the next kind of docs/systems/script-host/area-survey.md (lk03 `KH2_PROGRAMS=1` group b_00, kind-2 group 33 with three; shares groups with `M_EX670` and `M_EX720`).
/// Its script (5588 words) hands `enmp` row 81 to `set_obj_param` (host 2.9 at 0x1429 inside R1402, which is called at 0x0aa8 with 81 pushed at 0x0a90, stored to local+0x20 [read at 0x1424]).
pub const KIND: Kind = Kind { name: "M_EX780", enmp: 81, model: "M_EX780", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX780` is an enemy kind of the app: its `set_obj_param` row is `enmp` 81; the `00objentry` row is asserted against the disc. Needs `KH2_ISO` for the table part.
    #[test]
    fn m_ex780_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (81, "M_EX780", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX780").expect("objentry row of M_EX780");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_object2, row.spawn_limiter), (20, 4, 1039, 0, 0, 7));
    }
}
