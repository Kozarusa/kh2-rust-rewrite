//! The defeated-enemy counters the journal's enemy pages show (docs/menus.md 20.11).
//!
//! The original raises a counter in the enemies' defeat method `FUN_00184a30` (and the party members' `FUN_001c46a8`, which counts the
//! ids 0x404 / 0x405): `FUN_00187e08(0x33fec0, NeoStatus)` adds 1 to the word `FUN_00187dc8` finds (ids 1000 .. 1046 and 3000 .. 3011),
//! up to 9999, while story flag 0x8d0 is set. The port has one defeat point it can see without touching the enemy module: an `Enemy`
//! whose HP has reached 0. Each enemy is counted once (the `Counted` marker); an enemy of the harness's own waves has no object id and
//! counts nothing.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::assets::Disc;
use crate::enemy::Enemy;
use crate::progress::Progress;

/// An enemy whose defeat has been counted.
#[derive(Component)]
pub struct Counted;

/// `NeoStatus` (`objentry` +0x4c) by `00objentry` id, read once from the disc.
#[derive(Resource, Default)]
pub struct NeoByObject(pub HashMap<u32, u16>);

/// `NeoStatus` by `00objentry` id, read from the disc.
pub fn read_neo(disc: &Disc) -> HashMap<u32, u16> {
    use kh2_tables::tables as t;
    let Ok(mut a) = disc.0.lock() else { return HashMap::new() };
    kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).map(|d| d.rows.into_iter().map(|r| (r.object_id, r.neo_status)).collect()).unwrap_or_default()
}

/// The `rcct` rows (five reaction commands per enemy, `FUN_001d0258`).
pub fn read_rcct(disc: &Disc) -> Vec<kh2_tables::player::Rcct> {
    let Ok(mut a) = disc.0.lock() else { return Vec::new() };
    kh2_tables::player::load_rcct(&mut a).map(|r| r.1).unwrap_or_default()
}

/// The `limt` slot of each limit command (`FUN_001d2518`: the row whose Command is the record's id; its `Id` byte indexes the counters).
pub fn read_limit_slots(disc: &Disc) -> HashMap<u16, u8> {
    use kh2_tables::tables as t;
    let Ok(mut a) = disc.0.lock() else { return HashMap::new() };
    kh2_tables::load::<t::Limt>(&mut a, &t::LIMT).map(|d| d.rows.into_iter().map(|r| (r.command, r.id)).collect()).unwrap_or_default()
}

/// Each command's name message (`+8`) and use-counter slot (`ReactionCount`, `+0x26`) by command id (`FUN_001b6f48`).
pub fn read_cmds(disc: &Disc) -> HashMap<u16, super::enemies::CmdInfo> {
    use kh2_tables::tables as t;
    let Ok(mut a) = disc.0.lock() else { return HashMap::new() };
    kh2_tables::load::<t::Cmd>(&mut a, &t::CMD).map(|d| d.rows.into_iter().map(|c| (c.id, (c.message_id, c.reaction_count))).collect()).unwrap_or_default()
}

pub fn load_neo(mut commands: Commands, disc: Option<Res<Disc>>) {
    commands.insert_resource(NeoByObject(disc.map(|d| read_neo(&d)).unwrap_or_default()));
}

/// The id the counter of a defeated enemy is raised under: its objentry's NeoStatus (`FUN_001699a8`).
pub fn counter_id(neo: &NeoByObject, object_id: u32) -> Option<u32> {
    neo.0.get(&object_id).map(|&n| u32::from(n)).filter(|&n| Progress::enemy_counter_id(n))
}

/// Counts one defeated enemy; returns the counter id and its new value when it counted.
pub fn count_one(progress: &mut Progress, neo: &NeoByObject, object_id: u32) -> Option<(u32, u32)> {
    let id = counter_id(neo, object_id)?;
    progress.count_defeat(id).then(|| (id, progress.enemy_defeats(id)))
}

/// The enemies not counted yet whose component changed this frame.
type Uncounted = (Without<Counted>, Changed<Enemy>);

pub fn count_defeats(
    mut commands: Commands,
    progress: Option<ResMut<Progress>>,
    neo: Option<Res<NeoByObject>>,
    enemies: Query<(Entity, &Enemy), Uncounted>,
) {
    let (Some(mut progress), Some(neo)) = (progress, neo) else { return };
    for (e, enemy) in &enemies {
        if enemy.hp > 0 {
            continue;
        }
        commands.entity(e).insert(Counted);
        if let Some((id, n)) = count_one(&mut progress, &neo, enemy.object_id) {
            info!("journal: {} defeated: counter {id} is now {n} (FUN_00184a30 -> FUN_00187e08)", enemy.name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neo() -> NeoByObject {
        NeoByObject(HashMap::from([(0x10, 1000), (0x11, 1), (0x12, 3005), (0x13, 2000)]))
    }

    #[test]
    fn a_counter_id_needs_a_known_object_and_a_slot() {
        let neo = neo();
        assert_eq!(counter_id(&neo, 0x10), Some(1000));
        assert_eq!(counter_id(&neo, 0x12), Some(3005));
        assert_eq!((counter_id(&neo, 0x11), counter_id(&neo, 0x13), counter_id(&neo, 0x99)), (None, None, None));
    }

    /// A defeat counts under the object's NeoStatus, only with story flag 0x8d0.
    #[test]
    fn a_defeat_counts_under_the_neo_status() {
        let neo = neo();
        let mut p = Progress::from_block(&[], std::sync::Arc::new(crate::progress::ItemIndex::default()));
        assert_eq!(count_one(&mut p, &neo, 0x10), None, "flag 0x8d0 is clear");
        p.story().set(0x8d0);
        assert_eq!(count_one(&mut p, &neo, 0x10), Some((1000, 1)));
        assert_eq!(count_one(&mut p, &neo, 0x10), Some((1000, 2)));
        assert_eq!(count_one(&mut p, &neo, 0x12), Some((3005, 1)));
        assert_eq!((count_one(&mut p, &neo, 0x11), count_one(&mut p, &neo, 0)), (None, None));
    }
}
