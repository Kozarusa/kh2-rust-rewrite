//! The enemy kind `M_EX590`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX590` (object 75, `ObjectType` 4; `00objentry` row 75: `NeoStatus` 1013, `SpawnLimiter` 9, no spawn objects), called by its model; the pot Heartless of al01, bb08, he09, mu02, nm02 and nm06. Its script (6556 words) hands `enmp` row 4 to `set_obj_param` (host 2.9 at 0x07b6, the 4 pushed at 0x07b0). Its fighting members sit in kind-2 groups that start disabled and are enabled by event 0x9b (docs/systems/script-host/enemies-m-ex5.md "The enemy M_EX590").
pub const KIND: Kind = Kind { name: "M_EX590", enmp: 4, model: "M_EX590", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX590` (object 75) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1013, its script's `set_obj_param` row is `enmp` 4; bb08's b_70 holds its fighting group 0x565 (kind 2, flag bit 0: disabled) with the signal
    /// (event 0x9b, argument 0x565, enable), beside the kind-1 group 0x545. Needs `KH2_ISO` for the table part.
    #[test]
    fn m_ex590_is_an_enemy_kind_whose_fighting_group_waits_for_event_0x9b() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (4, "M_EX590", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX590").expect("objentry row of M_EX590");
        assert_eq!((row.object_id, row.object_type, row.neo_status), (75, 4, 1013));
        let groups = area_groups(&disc, "bb08");
        let b70 = groups.iter().find(|g| g.name == "b_70").expect("bb08 b_70");
        let fighting = b70.parts.iter().find(|p| p.id == 0x565).expect("group record 0x565");
        assert_eq!((fighting.kind, fighting.disabled), (2, true));
        assert!(b70.members.iter().any(|m| m.kind.name == "M_EX590" && m.spawn_kind == 2 && b70.parts[m.part].id == 0x565));
        assert!(b70.members.iter().any(|m| m.kind.name == "M_EX590" && m.spawn_kind == 1 && b70.parts[m.part].id == 0x545), "the decoys are in the kind-1 group 0x545");
        let sig = area_signals(&disc, "bb08");
        assert!(sig.iter().any(|g| g.tag == "b_70" && g.id == 0x565 && (g.event, g.arg, g.action) == (0x9b, 0x565, 0)));
        // The same shape in the other four areas: a disabled kind-2 record that registers for event 0x9b.
        for (area, id) in [("mu02", 0x2c5u16), ("al01", 0x1d9), ("nm06", 0x41e)] {
            assert!(area_signals(&disc, area).iter().any(|g| g.id == id && g.event == 0x9b && g.action == 0), "{area} {id:#x}");
        }
    }
}
