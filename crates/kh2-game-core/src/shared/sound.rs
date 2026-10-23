//! The sound-effect request and the game's shared generator the lower crates use (moved out of `audio.rs`, which re-exports them).

use bevy::prelude::*;

/// A sound-effect request: the game's SE number is `bank * 1000 + index`.
#[derive(Message, Clone, Debug)]
pub struct PlaySe {
    pub bank: u16,
    pub index: u16,
    /// The requesting object's own effect/sound file (`obj/<name>.apdx`); it carries that
    /// object's private bank (e.g. the keyblade's 500).
    pub apdx: Option<String>,
    /// World position in metres of the sound's source. The game starts every object sound at the
    /// object's position (FUN_00197008, FUN_0016baa0) and derives volume and pan from it
    /// (`kh2_audio::positional`); `None` plays at full volume and centre pan like its interface
    /// sounds.
    pub pos: Option<Vec3>,
}

/// `PlaySe::bank` value meaning "the requesting object's own bank".
pub const OWN_BANK: u16 = 0xffff;

/// The bank of sound numbers 500000 and up, which FUN_00197008 replaces by the object's own bank.
pub const OBJECT_SOUND_BANK: u16 = 500;

impl PlaySe {
    /// The sound an object plays when it is hit: entry 0/1 (light) or 2/3 (heavy) of its own bank,
    /// by the attack's `EnemyReaction`, alternating per hit. The game's class table: 1 and 12 are
    /// light, 3/6/8/10/13/16/18 light, 4/7/9/11/15/17 heavy, 5 light; the rest are silent.
    pub fn hit(reaction: u8, counter: u8, apdx: Option<String>) -> Option<PlaySe> {
        let base = match reaction {
            1 | 12 | 3 | 6 | 8 | 10 | 13 | 16 | 18 | 5 => 0,
            4 | 7 | 9 | 11 | 15 | 17 => 2,
            _ => return None,
        };
        Some(PlaySe { bank: OWN_BANK, index: base + (counter & 1) as u16, apdx, pos: None })
    }

    /// A system sound: entry `sound` of the boot bank `se/se000.seb` (game bank 0). The game's
    /// `FUN_002c14a8(sound, 0)` (interface sounds, the HUD alarm) is `FUN_002be49c(2, sound, ..)`:
    /// the first argument is the driver SLOT 2 that holds `se000.seb`, not a bank number
    /// ([`kh2_audio::ssl::SYSTEM_BANK`], docs/systems/audio/audio.md "System sounds").
    pub fn system(sound: u32) -> PlaySe {
        PlaySe::number(kh2_audio::ssl::SYSTEM_BANK as u32 * 1000 + sound, None)
    }

    pub fn number(n: u32, apdx: Option<String>) -> PlaySe {
        PlaySe { bank: (n / 1000) as u16, index: (n % 1000) as u16, apdx, pos: None }
    }

    /// A sound number played through an object's method (FUN_00197008): numbers of 500000 and up name
    /// the sound bank the object registered when its resources loaded (the short at +2 of the record
    /// at object +0x9b4, written by FUN_0016f568 from the object's `se` and `wave` blocks), and the
    /// index counts in that bank. The app's equivalent is the object's own `.apdx` block
    /// ([`OWN_BANK`]). An object without an `.apdx` (no registered bank, FUN_00197008 keeps the
    /// number) plays the global bank 500.
    pub fn object_sound(n: u32, apdx: Option<String>) -> PlaySe {
        let mut se = PlaySe::number(n, apdx);
        if se.bank == OBJECT_SOUND_BANK && se.apdx.is_some() {
            se.bank = OWN_BANK;
        }
        se
    }

    /// The same sound placed at a world position (metres).
    pub fn at(mut self, pos: Vec3) -> PlaySe {
        self.pos = Some(pos);
        self
    }
}

/// Volume and pan of a request against the listener, which is the camera (the game transforms the
/// position with the camera's view matrix, FUN_002c27b0): camera-space x is to the screen's right,
/// z in front, distances in game units.
pub fn placement(pos: Option<Vec3>, listener: Option<&Transform>) -> (i32, i32) {
    use kh2_audio::positional::{CENTRE_PAN, FULL_VOLUME, volume_pan};
    let (Some(p), Some(l)) = (pos, listener) else { return (FULL_VOLUME, CENTRE_PAN) };
    let d = (p - l.translation) / crate::SCALE;
    volume_pan([d.dot(*l.right()), d.dot(*l.up()), d.dot(*l.forward())])
}

/// The game's shared random generator (`DAT_003716a0`), reset to 1 at start (`FUN_002f5fc8`).
/// Every user steps it; this package's voice picks are the only users in the app so far.
#[derive(Resource)]
pub struct GameRng(pub u32);

impl Default for GameRng {
    fn default() -> Self {
        GameRng(1)
    }
}
