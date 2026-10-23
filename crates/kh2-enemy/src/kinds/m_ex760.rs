//! The enemy kind `M_EX760`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX760` (object 0x131 = 305, `ObjectType` 4, `NeoStatus` 1028, Weight 10, `SpawnLimiter` 7), the enemy of 15 areas (bb08,
/// ca09, ca13, ca19, hb02, hb08 ..), called by its model (the data gives no name). Its BDX script (7982 words; modes `appear`,
/// `btl_attack`, `atk_turn`, `atk_jump`, `atk_down`, `footwork`, `mode_revenge`, `leave`, `dead`) hands `enmp` row 36 to
/// `set_obj_param` (host 2.9 at 0x1cec, in the constructor routine R1cc5 that the init calls at 0x0c02 with 36 as its 8th
/// word); its `dead` mode (R0dbf at 0x07b2) starts element 2 of its effect file (1.87), sets flags 3 and 6, fades out and hides.
pub const KIND: Kind = Kind { name: "M_EX760", enmp: 36, model: "M_EX760", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX760` (object 0x131) is an enemy kind of the app: bb08's btl groups b_20 and b_21 hold it, its `00objentry` row
    /// is type 4 with `NeoStatus` 1028 and its script's `set_obj_param` row is `enmp` 36 (checked in the disc's table).
    #[test]
    fn m_ex760_is_an_enemy_kind_that_bb08_spawns() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (36, "M_EX760", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX760").expect("objentry row of M_EX760");
        assert_eq!((row.object_id, row.object_type, row.neo_status), (305, OBJECT_ENEMY, 1028));
        let groups = area_groups(&disc, "bb08");
        for name in ["b_20", "b_21"] {
            let g = groups.iter().find(|g| g.name == name).unwrap_or_else(|| panic!("bb08 has {name}"));
            assert!(!g.members.is_empty() && g.members.iter().all(|m| m.kind.name == KIND.name || m.kind.name == kinds::m_ex130::KIND.name || m.kind.name == kinds::m_ex710::KIND.name), "{name} holds only M_EX760, M_EX130 and M_EX710");
            assert!(g.skipped.is_empty(), "{name}: every enemy of the group is a kind of the app, not {:?}", g.skipped);
        }
    }
}
