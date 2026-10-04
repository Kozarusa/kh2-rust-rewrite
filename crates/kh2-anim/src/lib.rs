//! KH2 animation: motion sets, motions and motion triggers.
//!
//! * MSET: a BAR whose N-th entry is motion slot N. For player motion sets the slot of a motion
//!   ID is `state + id * 4` with a fallback order between the four states (see [`Mset::slot`]).
//! * ANB (BAR type 17): a motion (BAR type 9) and optional motion triggers (BAR type 16).
//! * Interpolated motion: initial pose + F-curves (constant / linear / Hermite keys) per bone channel.
//!
//! Layouts per OpenKH `docs/kh2/file/anb/{mset,anb,motion}.md` (Apache-2.0), which credit
//! kenjiuno's msetDoc; see CREDITS.md.

pub mod motion;
pub mod triggers;

pub use motion::{Motion, Pose};
pub use triggers::{FrameTrigger, RangeTrigger, Triggers};

/// Player state used to pick a slot (`relative slot index` in the MSET docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState {
    BattleWeapon = 0,
    BattleNoWeapon = 1,
    FieldNoWeapon = 2,
    FieldWeapon = 3,
}

impl PlayerState {
    fn fallback(self) -> [usize; 4] {
        match self {
            PlayerState::FieldWeapon => [3, 2, 0, 1],
            PlayerState::FieldNoWeapon => [2, 3, 1, 0],
            PlayerState::BattleNoWeapon => [1, 0, 2, 3],
            PlayerState::BattleWeapon => [0, 1, 3, 2],
        }
    }
}

/// Well-known motion IDs (from the game's debug motion list).
pub mod id {
    pub const IDLE: usize = 0;
    pub const WALK: usize = 1;
    pub const RUN: usize = 2;
    pub const JUMP: usize = 3;
    pub const FALL: usize = 4;
    pub const LAND: usize = 5;
    pub const DAMAGE_S_FRONT_HIGH: usize = 10;
    pub const DAMAGE_S_BACK_HIGH: usize = 12;
    pub const DAMAGE_BLOW_FRONT: usize = 16;
    pub const DAMAGE_BLOW_FRONT_LAND: usize = 17;
    pub const DAMAGE_BLOW_RECOV: usize = 40;
    pub const REFLECT: usize = 41;
    pub const AUTOGUARD: usize = 42;
    pub const APPEAR: usize = 44;
    pub const DEAD_LAND: usize = 76;
}

pub struct Slot {
    pub tag: String,
    pub motion: Option<Motion>,
    pub triggers: Triggers,
}

pub struct Mset {
    pub slots: Vec<Option<Slot>>,
}

impl Mset {
    pub fn parse(file: &[u8]) -> Option<Mset> {
        let entries = kh2_bar::parse(file).ok()?;
        let slots = entries
            .iter()
            .map(|e| {
                if e.data.is_empty() {
                    return None;
                }
                let anb = kh2_bar::parse(e.data).ok()?;
                let motion = anb.iter().find(|x| x.kind == 9).and_then(|x| Motion::parse(x.data));
                let triggers = anb.iter().find(|x| x.kind == 16).map(|x| Triggers::parse(x.data)).unwrap_or_default();
                Some(Slot { tag: e.tag.to_string(), motion, triggers })
            })
            .collect();
        Some(Mset { slots })
    }

    /// Slot for a motion ID in a player MSET, with the game's state fallback.
    pub fn slot(&self, motion_id: usize, state: PlayerState) -> Option<usize> {
        state.fallback().into_iter().map(|rel| rel + motion_id * 4).find(|&s| self.has(s))
    }

    pub fn has(&self, slot: usize) -> bool {
        self.slots.get(slot).is_some_and(|s| s.as_ref().is_some_and(|s| s.motion.is_some()))
    }

    pub fn get(&self, slot: usize) -> Option<&Slot> {
        self.slots.get(slot)?.as_ref()
    }

    /// Slots whose BAR tag matches (e.g. `"A320"`), in file order.
    pub fn by_tag<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = usize> + 'a {
        self.slots.iter().enumerate().filter(move |(_, s)| s.as_ref().is_some_and(|s| s.tag == tag && s.motion.is_some())).map(|(i, _)| i)
    }
}
