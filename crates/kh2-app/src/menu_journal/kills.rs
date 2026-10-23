//! The defeated-enemy counters the journal's enemy pages show (docs/systems/ui/menus-journal-pages.md 20.11).
//!
//! The original raises a counter in the enemies' defeat method `FUN_00184a30` (and `FUN_001c46a8`, which counts the
//! ids 0x404 / 0x405 when the player absorbs a pickup-like object, docs/systems/ui/menus-shop.md 24.4a): `FUN_00187e08(0x33fec0, NeoStatus)` adds 1 to the word `FUN_00187dc8` finds (ids 1000 .. 1046 and 3000 .. 3011),
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

/// What the counting system reads of a defeatable object: the app's `Enemy`, or a stand-in in the tests.
pub trait Defeatable: Component {
    fn hp(&self) -> i32;
    fn object_id(&self) -> u32;
    fn label(&self) -> &str;
}

impl Defeatable for Enemy {
    fn hp(&self) -> i32 {
        self.hp
    }
    fn object_id(&self) -> u32 {
        self.object_id
    }
    fn label(&self) -> &str {
        self.name
    }
}

/// The objects not counted yet whose component changed this frame.
type Uncounted<T> = (Without<Counted>, Changed<T>);
/// The counted objects whose component changed this frame (a revive shows here).
type CountedAgain<T> = (With<Counted>, Changed<T>);

/// The counting system for `T`. A defeat counts once; an object whose HP is above 0 again (a revive) is armed again, so its next
/// defeat counts too. The original runs `FUN_00184a30` on every entry into the defeat mode (`FUN_001acfe0`, the only direct
/// caller found), so each defeat of a reviving enemy counts [inferred: a revive re-enters it]. The app has no enemy that revives
/// yet, so this arm is covered by the test only.
pub fn count_generic<T: Defeatable>(
    mut commands: Commands,
    progress: Option<ResMut<Progress>>,
    neo: Option<Res<NeoByObject>>,
    fresh: Query<(Entity, &T), Uncounted<T>>,
    counted: Query<(Entity, &T), CountedAgain<T>>,
) {
    for (e, t) in &counted {
        if t.hp() > 0 {
            commands.entity(e).remove::<Counted>();
        }
    }
    let (Some(mut progress), Some(neo)) = (progress, neo) else { return };
    for (e, t) in &fresh {
        if t.hp() > 0 {
            continue;
        }
        commands.entity(e).insert(Counted);
        if let Some((id, n)) = count_one(&mut progress, &neo, t.object_id()) {
            info!("journal: {} defeated: counter {id} is now {n} (FUN_00184a30 -> FUN_00187e08)", t.label());
        }
    }
}

pub fn count_defeats(
    commands: Commands,
    progress: Option<ResMut<Progress>>,
    neo: Option<Res<NeoByObject>>,
    fresh: Query<(Entity, &Enemy), Uncounted<Enemy>>,
    counted: Query<(Entity, &Enemy), CountedAgain<Enemy>>,
) {
    count_generic::<Enemy>(commands, progress, neo, fresh, counted);
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

    #[derive(Component)]
    struct Stub {
        hp: i32,
        object: u32,
    }

    impl Defeatable for Stub {
        fn hp(&self) -> i32 {
            self.hp
        }
        fn object_id(&self) -> u32 {
            self.object
        }
        fn label(&self) -> &str {
            "stub"
        }
    }

    fn app(flag: bool) -> App {
        let mut p = Progress::from_block(&[], std::sync::Arc::new(crate::progress::ItemIndex::default()));
        if flag {
            p.story().set(0x8d0);
        }
        let mut app = App::new();
        app.insert_resource(p).insert_resource(neo()).add_systems(Update, count_generic::<Stub>);
        app
    }

    fn hits(app: &App, id: u32) -> u32 {
        app.world().resource::<Progress>().enemy_defeats(id)
    }

    /// A defeated enemy counts once however many frames pass; an enemy with HP left counts nothing; the counter needs flag 0x8d0.
    #[test]
    fn the_system_counts_a_defeat_once_and_only_with_the_flag() {
        let mut app = app(true);
        let dead = app.world_mut().spawn(Stub { hp: 0, object: 0x10 }).id();
        app.world_mut().spawn(Stub { hp: 5, object: 0x10 });
        app.world_mut().spawn(Stub { hp: 0, object: 0x99 });
        app.update();
        app.update();
        app.world_mut().get_mut::<Stub>(dead).unwrap().hp = 0;
        app.update();
        assert_eq!(hits(&app, 1000), 1);
        assert!(app.world().entity(dead).contains::<Counted>());

        let mut off = app_without_flag();
        off.world_mut().spawn(Stub { hp: 0, object: 0x10 });
        off.update();
        assert_eq!(hits(&off, 1000), 0, "flag 0x8d0 is clear");
    }

    fn app_without_flag() -> App {
        app(false)
    }

    /// A revived enemy is armed again and its next defeat counts as well.
    #[test]
    fn a_revived_enemy_counts_each_defeat() {
        let mut app = app(true);
        let e = app.world_mut().spawn(Stub { hp: 0, object: 0x10 }).id();
        app.update();
        assert_eq!(hits(&app, 1000), 1);
        app.world_mut().get_mut::<Stub>(e).unwrap().hp = 30;
        app.update();
        assert!(!app.world().entity(e).contains::<Counted>());
        app.world_mut().get_mut::<Stub>(e).unwrap().hp = 0;
        app.update();
        assert_eq!(hits(&app, 1000), 2);
    }
}
