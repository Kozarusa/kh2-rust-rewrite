//! The enemy kind `M_EX950`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX950` (object 314, `ObjectType` 4; `00objentry` row 314: `NeoStatus` 3009, flags 0, `SpawnLimiter` 17, `SpawnObject1` 1537
/// `M_EX950_CARD`, `SpawnObject2` 1538 `M_EX950_DICE`), a dice-and-card Nobody of 14 areas (the area survey: ca09, ca10, ca12, ca13,
/// ca14, ca15, tt03 ..), called by its model. Its script hands `enmp` row 50 to `set_obj_param` (host 2.9: BDX `M_EX950` R2971 at
/// 0x2998, its eighth word, 50, pushed at 0x196d by the call at 0x1985); its `dead` mode (registered at 0x1949) is the death mode.
pub const KIND: Kind = Kind { name: "M_EX950", enmp: 50, model: "M_EX950", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX950` (object 314) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 3009 and the two
    /// spawn objects 1537 (`M_EX950_CARD`) and 1538 (`M_EX950_DICE`), its script's `set_obj_param` row is `enmp` 50, and the
    /// ca areas' groups hold it. Needs `KH2_ISO` for the table part.
    #[test]
    fn m_ex950_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (50, "M_EX950", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX950").expect("objentry row of M_EX950");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_object2), (314, OBJECT_ENEMY, 3009, 1537, 1538));
        let spawned = |id: i32| rows.iter().find(|r| r.object_id as i32 == id).map(|r| r.model_name.clone());
        assert_eq!(spawned(1537).as_deref(), Some("M_EX950_CARD"));
        assert_eq!(spawned(1538).as_deref(), Some("M_EX950_DICE"));
    }
}
