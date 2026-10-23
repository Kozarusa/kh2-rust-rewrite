//! The enemy kind `M_EX870`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX870` (object 316 = 0x13c, `ObjectType` 4; `00objentry` row 316: flags 8, `NeoStatus` 0, Weight 100, `SpawnLimiter` 34, `SpawnObject1` 315 = `M_EX960`), a Nobody of eh17 (the area survey: b_00 holds eight, beside five `M_EX920`), called by its model.
/// Its script (3606 words) never calls `set_obj_param` and its row has `NeoStatus` 0, so nothing builds a status block (FUN_001b2148 is the only builder): `Kind::status` is false and it cannot be hurt, like `M_EX850`. Its constructor R041c (0x041c) only sets
/// fields, then its set-up routine makes the hittable body with host 1.0 `create_object(315)` (call at 0x03ed) and moves it to its own point. `enmp` is a stand-in for the numbers `spawn_enemy` asks of the data [inferred].
pub const KIND: Kind = Kind { name: "M_EX870", enmp: 49, model: "M_EX870", death_effect: false, status: false };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX870` (object 316) is an enemy kind of the app without a status block (`NeoStatus` 0, no `set_obj_param`) that makes `M_EX960` (object 315, `enmp` 51) with host 1.0; eh17's `btl` program 1 loads b_00 with eight `M_EX870` and five
    /// `M_EX920`. Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex870_is_an_enemy_kind_and_makes_m_ex960() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert!(KINDS.iter().any(|k| k.model == kinds::m_ex960::KIND.model));
        assert_eq!((KIND.model, KIND.status), ("M_EX870", false));
        assert_eq!((kinds::m_ex960::KIND.enmp, kinds::m_ex960::KIND.model, kinds::m_ex960::KIND.status, kinds::m_ex960::KIND.death_effect), (51, "M_EX960", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX870").expect("objentry row of M_EX870");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (316, OBJECT_ENEMY, 0, 315, 34));
        let body = rows.iter().find(|r| r.model_name == "M_EX960").expect("objentry row of M_EX960");
        assert_eq!((body.object_id, body.object_type, body.neo_status), (315, OBJECT_ENEMY, 3010));
        let groups = area_groups(&disc, "eh17");
        let ard = disc.read("ard/eh17.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("eh17.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 1, 0], 0);
        let b00 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_00").expect("program 1 loads b_00");
        assert_eq!(b00.members.iter().filter(|m| m.kind.name == KIND.name).count(), 8);
        assert!(b00.skipped.is_empty());
    }
}
