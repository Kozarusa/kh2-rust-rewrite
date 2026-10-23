//! The enemy kind `M_EX060`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

/// `M_EX060` (`ObjectType` 4, object 1, `NeoStatus` 1040, `SpawnLimiter` 13, no spawn objects; a Heartless of the areas al00, al01, al10, al12, al13, al14 ..), called by its model.
/// Its script hands `enmp` row 53 to `set_obj_param` (host 2.9: BDX `M_EX060` R17fa at 0x17fa, called at 0x0afa with 53 as the first of its eight values after the object,
/// pushed at 0x0ae2, which R17fa stores to local+0x20; host 2.9 at 0x1821).
pub const KIND: Kind = Kind { name: "M_EX060", enmp: 53, model: "M_EX060", death_effect: true, status: true };

#[cfg(test)]
mod tests {
    use super::KIND;
    use crate::enemy::*;

    /// `M_EX060` (object 1) is an enemy kind of the app: its `00objentry` row is type 4 with `NeoStatus` 1040, `SpawnLimiter` 13 and no spawn objects, its script's `set_obj_param` row is `enmp` 53, and al00's
    /// `btl` program 0xa loads b_20 with `M_EX060` beside ten `M_EX420`. Needs `KH2_ISO` for the table and group part.
    #[test]
    fn m_ex060_is_an_enemy_kind_with_its_objentry_row() {
        assert!(KINDS.iter().any(|k| k.model == KIND.model));
        assert_eq!((KIND.enmp, KIND.model, KIND.status, KIND.death_effect), (53, "M_EX060", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX060").expect("objentry row of M_EX060");
        assert_eq!((row.object_id, row.object_type, row.neo_status, row.spawn_object1, row.spawn_limiter), (1, OBJECT_ENEMY, 1040, 0, 13));
        let groups = area_groups(&disc, "al00");
        let ard = disc.read("ard/al00.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("al00.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 0xa, 0], 0);
        let b20 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_20").expect("program 0xa loads b_20");
        assert!(b20.members.iter().any(|m| m.kind.name == KIND.name));
    }
}
