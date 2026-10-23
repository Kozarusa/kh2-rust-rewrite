//! The enemy kind `M_EX610_RAW`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX610_RAW` (object 1875, tr03 b_40, 24 members; `00objentry` row 1875: `AnimationName` `M_EX610.mset`), the `M_EX610` body with the base kind's motion set: the rig loader takes the motion-set name from the row (`mset_name`).
/// Its script is the `M_EX610` one (`enmp` row 101, see there).
pub const KIND: Kind = Kind { name: "M_EX610_RAW", enmp: 101, model: "M_EX610_RAW", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX610_RAW` (object 1875): its row names `M_EX610.mset`, `obj/M_EX610_RAW.mset` does not exist, and the kind loads its rig. Needs `KH2_ISO`.
    #[test]
    fn m_ex610_raw_loads_with_the_base_motion_set() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model), (101, "M_EX610_RAW"));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX610_RAW").expect("objentry row of M_EX610_RAW");
        assert_eq!((row.object_id, row.object_type, row.animation_name.as_str()), (1875, OBJECT_ENEMY, "M_EX610.mset"));
        assert!(disc.read("obj/M_EX610_RAW.mset").is_none());
        let name = mset_name(Some(row.animation_name.as_str()), "M_EX610_RAW");
        assert!(load_rig(&disc, "M_EX610_RAW", &name).is_some());
    }
}
