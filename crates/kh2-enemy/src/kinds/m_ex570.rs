//! The enemy kind `M_EX570`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX570` (object 9, `ObjectType` 21; `00objentry` row 9: `NeoStatus` 1011, `SpawnLimiter` 20, no spawn objects), called by its model; the next kind of docs/systems/script-host/area-survey.md (mu01 `KH2_PROGRAMS=2` group b_01, kind-2 group 70 with three; stands with `M_EX580` in 9 groups).
/// Its script (5178 words) hands `enmp` row 60 to `set_obj_param` (host 2.9 at 0x1277 inside R1250, which is called at 0x0627 with 60 pushed at 0x060f, stored to local+0x20).
pub const KIND: Kind = Kind { name: "M_EX570", enmp: 60, model: "M_EX570", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX570` (object 9) is an enemy kind of the app: its `00objentry` row is type 21 with `NeoStatus` 1011 and `SpawnLimiter` 20, its script's `set_obj_param` row is `enmp` 60. Needs `KH2_ISO` for the table part.
    #[test]
    fn m_ex570_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (60, "M_EX570", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX570").expect("objentry row of M_EX570");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_object2, row.spawn_limiter), (9, 21, 1011, 0, 0, 20));
    }
}
