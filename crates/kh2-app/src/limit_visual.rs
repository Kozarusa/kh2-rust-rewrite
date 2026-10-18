//! The participants' motions in a limit (WP-LS2c): host 2.47 of the limit script. See docs/limits-reactions.md "Limit visuals (WP-LS2c)".
//!
//! The limit BAR (`limit/us/<file>`) holds an entry of kind 0x14 named `LIMI`. It is a BAR of 259 entries in the layout of a motion
//! set's paired entries (`player_reaction::paired_entries`): the last seven (index 252 to 258) are kind 20 sets (`A910` to `A915`), each a
//! BAR of sub-motions named `%03d0` of a model byte (`0010` Sora, `0050` Auron, ...), each sub-motion an animation BAR (type 9 motion,
//! type 16 triggers) made for that character's skeleton. Host 2.47 (`0x18e988` -> `FUN_001b3438` -> `FUN_0017f0c0` -> `FUN_0017ef98`
//! -> `FUN_0017ee40`) starts one of them on a participant's motion controller: the entry index is the script's motion id (252 to 258:
//! seven ids for seven sets), the sub-motion name is `%03d0` of the object's model byte (`FUN_001699b8`, format string `0x37a598`),
//! looked up by name in the set with up to four tries (`FUN_00168510(set, -1, name, k)`, k = 0 to 3). The same lookup the player's
//! paired motions use (host 1.29, `player_reaction.rs`).
//!
//! The app plays the sub-motion with `Animator::play_external` on the participant's body: Sora enters `State::Reaction` (no input) and
//! gets control back when the limit is over; a party member of the field (`party::PartyMember`, found by its character id, which the
//! scripts bind by NeoStatus) is held by [`LimitHold`] so the party's own motion choice leaves its animator alone.
//!
//! Not ported: the motion of a participant that is not in the field party (the app has no limit-only objects), the second sub-motion
//! of a weapon (names `0011`, `0012`: the keyblade objects `+0xaf8`, `FUN_0017ef98`'s loop), the root motion and the placement of the
//! participants (the scripts do it through hosts the limit host does not serve), and the motion queue (`+0xf0` / `+0xfc` of the controller).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;

use crate::assets::Disc;
use crate::limit::{LimitStarted, LimitTask, KIND_PARTY_SET};
use crate::player_reaction::{paired_entries, sub_name, Paired};
use crate::skel::Animator;
use crate::sora::{Sora, State};

/// One call of host 2.47 as the app plays it.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct LimitMotion {
    /// The participant handle the script gave (the player, or a [`crate::limit_script::partner`] stand-in).
    pub who: u32,
    /// The script's motion id, which is the index of the entry in the `LIMI` set.
    pub id: usize,
    /// The cross-fade in ticks (the call's float argument).
    pub blend: f32,
}

/// One call of host 2.59 as the app plays it: an element of the limit file's own PAX started on a participant.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitEffect {
    /// The participant handle (the owner the effect is bound to).
    pub who: u32,
    /// The element id (the halfword at row `+2` of the table `FUN_001db868` walks).
    pub id: u16,
}

/// Marks a party member body whose motion a limit script drives: `party_tick` does not touch its animator while this is on it.
#[derive(Component)]
pub struct LimitHold;

/// The motion sets of the limit that started last, and what the module holds.
#[derive(Resource, Default)]
pub struct LimitMotions {
    /// The file the sets were read from.
    pub file: String,
    /// `LIMI` entry index -> its sub-motions.
    pub sets: HashMap<usize, Vec<Paired>>,
    /// Sora is held in `State::Reaction` by a limit motion.
    sora_held: bool,
    /// Party bodies held.
    held: Vec<Entity>,
    /// Motions started (for the log and the tests).
    pub started: u32,
}

/// The sets of the `LIMI` entry in the limit BAR's bytes (empty for a file without one).
pub fn sets_of(bar: &[u8]) -> HashMap<usize, Vec<Paired>> {
    let Ok(es) = kh2_bar::parse(bar) else { return HashMap::new() };
    es.iter().find(|e| e.kind == KIND_PARTY_SET).map(|e| paired_entries(e.data)).unwrap_or_default()
}

/// The sub-motion of the set `id` for a character whose model byte is `model` (`FUN_0017ee40`: the first entry of that name).
pub fn find<'a>(sets: &'a HashMap<usize, Vec<Paired>>, id: usize, model: u8) -> Option<&'a Paired> {
    let name = sub_name(model);
    sets.get(&id)?.iter().find(|(n, _)| *n == name)
}

pub struct LimitVisualPlugin;

impl Plugin for LimitVisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LimitMotion>()
            .add_message::<LimitEffect>()
            .init_resource::<LimitMotions>()
            .init_resource::<LimitPax>()
            .init_resource::<ModelBytes>()
            .add_systems(FixedUpdate, (load_sets, play_motions, start_effects, release).chain().after(crate::schedule::Sim::Sora));
    }
}

/// The limit file's PAX (the type 0x12 entry), loaded when its limit starts.
#[derive(Resource, Default)]
struct LimitPax {
    file: String,
    pax: Option<Arc<crate::fx::LoadedPax>>,
}

/// The objentry byte `+7` (`WeaponJoint`) by model name: the number `%03d0` is made of (`FUN_001699b8`).
#[derive(Resource, Default)]
struct ModelBytes(HashMap<String, u8>);

fn load_sets(
    disc: Option<Res<Disc>>,
    mut started: MessageReader<LimitStarted>,
    mut sets: ResMut<LimitMotions>,
    mut bytes: ResMut<ModelBytes>,
    mut pax: ResMut<LimitPax>,
) {
    for s in started.read() {
        let Some(disc) = disc.as_deref() else { continue };
        if bytes.0.is_empty()
            && let Some(rows) = disc.0.lock().ok().and_then(|mut a| kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).ok())
        {
            bytes.0 = rows.rows.iter().map(|r| (r.model_name.clone(), r.weapon_joint)).collect();
        }
        if pax.file != s.file {
            let common = disc.read("effect/texcommon.dpd").and_then(|b| kh2_fx::Dpd::parse(&b).ok());
            pax.pax = crate::fx::load(disc, &format!("limit/us/{}", s.file), PAX_ID, common.as_ref()).map(Arc::new);
            pax.file = s.file.clone();
            info!("limit visual: {} PAX {}", s.file, pax.pax.as_ref().map_or("not readable".to_string(), |p| format!("holds {} elements", p.pax.elements.len())));
        }
        sets.file = s.file.clone();
        sets.sets = disc.read(&format!("limit/us/{}", s.file)).map(|b| sets_of(&b)).unwrap_or_default();
        info!("limit visual: {} holds {} motion sets (entries {:?})", s.file, sets.sets.len(), { let mut k: Vec<_> = sets.sets.keys().copied().collect(); k.sort(); k });
    }
}

#[allow(clippy::too_many_arguments)]
fn play_motions(
    mut cmds: MessageReader<LimitMotion>,
    mut motions: ResMut<LimitMotions>,
    bytes: Res<ModelBytes>,
    mut sora_q: Query<&mut Sora>,
    members: Query<&crate::party::PartyMember>,
    mut anims: Query<&mut Animator>,
    mut commands: Commands,
) {
    for c in cmds.read() {
        let Some((body, model)) = body_of(c.who, sora_q.single().ok().map(|s| s.body), members.iter(), &bytes) else {
            info!("limit visual: motion {} for handle {:#x}: no body in the field", c.id, c.who);
            continue;
        };
        let Some((name, m)) = find(&motions.sets, c.id, model).cloned() else {
            info!("limit visual: set {} has no sub-motion {} (handle {:#x})", c.id, sub_name(model), c.who);
            continue;
        };
        let Ok(mut a) = anims.get_mut(body) else { continue };
        a.play_external(m, c.blend);
        motions.started += 1;
        if c.who == crate::enemy_obj::PLAYER {
            if let Ok(mut s) = sora_q.single_mut() {
                s.enter(State::Reaction, None);
            }
            motions.sora_held = true;
        } else if !motions.held.contains(&body) {
            motions.held.push(body);
            commands.entity(body).insert(LimitHold);
        }
        info!("limit visual: motion set {} sub-motion {name} on handle {:#x} (blend {})", c.id, c.who, c.blend);
    }
}

/// The body entity and model byte of a participant handle: the player (Sora's body, the byte of his objentry row `P_EX100`) or the
/// party member whose character id the stand-in handle carries. `None` for a character that is not in the field.
fn body_of<'a>(who: u32, sora: Option<Entity>, mut members: impl Iterator<Item = &'a crate::party::PartyMember>, bytes: &ModelBytes) -> Option<(Entity, u8)> {
    if who == crate::enemy_obj::PLAYER {
        return Some((sora?, bytes.0.get("P_EX100").copied().unwrap_or(1)));
    }
    members.find(|m| crate::limit_script::partner(u32::from(m.neo)) == who).map(|m| (m.body, bytes.0.get(&m.model).copied().unwrap_or(0)))
}

/// Host 2.59: the element starts as an effect owned by the participant (`FUN_001db868` -> `FUN_001db5f0`; the owner is the sixth
/// argument, the participant `FUN_0017fbe0` hangs the effect on when the element is bound).
fn start_effects(
    mut cmds: MessageReader<LimitEffect>,
    pax: Res<LimitPax>,
    bytes: Res<ModelBytes>,
    sora_q: Query<&Sora>,
    members: Query<&crate::party::PartyMember>,
    mut commands: Commands,
) {
    for c in cmds.read() {
        let Some(lib) = pax.pax.as_ref() else {
            info!("limit visual: effect {} asked but {} has no readable PAX", c.id, pax.file);
            continue;
        };
        let Some((body, _)) = body_of(c.who, sora_q.single().ok().map(|s| s.body), members.iter(), &bytes) else {
            info!("limit visual: effect {} for handle {:#x}: no body in the field", c.id, c.who);
            continue;
        };
        let started = crate::fx::start_placed(&mut commands, lib, c.id, crate::fx::Bind::Owner(body), crate::fx::Placement::default());
        info!("limit visual: effect element {} of {} on handle {:#x}: {}", c.id, pax.file, c.who, if started.is_some() { "started" } else { "no such element" });
    }
}

/// The id the limit PAX is registered under (the effect library keys its texture caches by it; the app's own files use 0 to 10).
const PAX_ID: usize = 0x4c49;

/// The limit is over: Sora and the party members get their own motions back.
fn release(task: Option<Res<LimitTask>>, mut motions: ResMut<LimitMotions>, mut sora_q: Query<&mut Sora>, mut commands: Commands) {
    if task.as_deref().is_some_and(|t| t.0.is_some()) {
        return;
    }
    if motions.sora_held {
        motions.sora_held = false;
        if let Ok(mut s) = sora_q.single_mut()
            && s.state == State::Reaction
        {
            let state = if s.motor.grounded { State::Ground } else { State::Air };
            s.enter(state, None);
            if state == State::Air {
                s.motor.start_fall();
            }
        }
        info!("limit visual: the limit is over, Sora's control returns");
    }
    for e in motions.held.drain(..) {
        if let Ok(mut c) = commands.get_entity(e) {
            c.remove::<LimitHold>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lookup is by the first entry of the name `%03d0`, and a set that does not exist gives nothing.
    #[test]
    fn the_sub_motion_is_found_by_the_model_byte() {
        let slot = std::sync::Arc::new(kh2_anim::Slot { tag: "A910".into(), motion: None, triggers: Default::default() });
        let mut sets: HashMap<usize, Vec<Paired>> = HashMap::new();
        sets.insert(252, vec![("0010".into(), slot.clone()), ("0050".into(), slot.clone())]);
        assert_eq!(find(&sets, 252, 1).map(|p| p.0.as_str()), Some("0010"));
        assert_eq!(find(&sets, 252, 5).map(|p| p.0.as_str()), Some("0050"));
        assert!(find(&sets, 252, 2).is_none(), "Donald has no sub-motion in this set");
        assert!(find(&sets, 253, 1).is_none(), "no such set");
    }

    /// The disc: Auron's `LIMI` has the seven sets at entries 252 to 258, each with Sora's `0010` and Auron's `0050`; Trinity's names the
    /// three characters of its party. Skipped without a disc.
    #[test]
    fn the_limit_files_hold_their_motion_sets() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let auron = sets_of(&disc.read("limit/us/auron.bar").expect("auron.bar"));
        let mut ids: Vec<_> = auron.keys().copied().collect();
        ids.sort();
        assert_eq!(ids, (252..=258).collect::<Vec<_>>());
        for id in 252..=258 {
            assert!(find(&auron, id, 1).is_some() && find(&auron, id, 5).is_some(), "set {id}");
            assert!(find(&auron, id, 1).is_some_and(|p| p.1.motion.is_some()));
        }
        assert!(sets_of(&disc.read("limit/us/donald.bar").expect("donald.bar")).len() <= 8);
    }
}
