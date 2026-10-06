//! Battle music: the switch between the area's field BGM (driver slot 0) and battle BGM (slot 1).
//!
//! Read in the decompilation of the user's USA executable. `FUN_00136ec0` (area load, called
//! from `FUN_001048e0`) loads the field BGM into slot 0 and, when the area's battle number
//! differs from the field number (`FUN_001371d8` clears the "battle music" flag at 0x354ebc when
//! they are equal, `FUN_00137300` sets it when they differ), the battle BGM. A per-frame task
//! (`FUN_001373d0`, registered by `FUN_00137128` with priority 300000) compares the battle flag
//! (`FUN_001903d8`, the `Battle` resource here) with the state it last acted on (0x354ebd):
//!
//! - flag rises and no battle fade is running (0x354eb8 is 0): `FUN_00137458`. The field slot
//!   fades to 0 over 0x3c0 (960) driver frames (command 0x11) and the battle slot starts fresh
//!   from volume 0 to 0x3000 over 0xf0 (240) frames after a delay of 0xf0 (command 0xf, mode 0),
//!   so the field music is already half gone when the battle music begins.
//! - flag falls while a battle fade is running: `FUN_001374c0`. The battle slot fades to 0 over
//!   0x2d0 (720) frames (command 0x11) and the field slot resumes (mode 1) from 0 to 0x3000 over
//!   0x1e0 (480) frames after a delay of 0xf0 (240).
//!
//! A frame of the fades is one 240 Hz driver frame (`kh2_audio::ssl::FRAME_HZ`). The flag itself
//! in the game has its own hysteresis (`FUN_001903e8`: a rise needs the desired state to have
//! lasted 45 ticks since it last matched, unless the 30 ticks after a reset are not over; a fall
//! 120 ticks); the app's `Battle` resource already holds the flag for 240 ticks after the last
//! sign of a fight, and is used as the flag unchanged.

use bevy::prelude::*;
use kh2_audio::ssl::{BGM_FULL, Driver};

use crate::assets::Disc;
use crate::audio::{Audio, load_sequence};
use crate::sora::Battle;

/// Fade lengths and delays in driver frames (240 Hz), straight from the arguments of the game's
/// calls (see the module comment).
pub mod timing {
    pub const FIELD_OUT: i32 = 0x3c0;
    pub const BATTLE_IN: i32 = 0xf0;
    pub const BATTLE_IN_DELAY: i32 = 0xf0;
    pub const BATTLE_OUT: i32 = 0x2d0;
    pub const FIELD_IN: i32 = 0x1e0;
    pub const FIELD_IN_DELAY: i32 = 0xf0;
}

/// The two music numbers of the loaded area and the switch state (0x354ebc, 0x354ebd, 0x354eb8).
#[derive(Resource, Default, Debug)]
pub struct AreaMusic {
    pub field: Option<u16>,
    pub battle: Option<u16>,
    /// Battle music exists: the numbers differ.
    pub switching: bool,
    /// The battle flag last acted on.
    pub state: bool,
    /// A battle fade is running (the battle slot is, or is being, the audible one).
    pub in_battle_music: bool,
}

impl AreaMusic {
    pub fn set_area(&mut self, field: u16, battle: u16) {
        *self = AreaMusic { field: Some(field), battle: Some(battle), switching: field != battle, ..Default::default() };
    }
}

/// What the per-frame task asks for.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Switch {
    Enter,
    Leave,
    None,
}

impl AreaMusic {
    /// `FUN_001373d0` for one frame with the battle flag `flag`.
    pub fn step(&mut self, flag: bool) -> Switch {
        if !self.switching || self.state == flag {
            return Switch::None;
        }
        self.state = flag;
        if flag {
            if self.in_battle_music {
                return Switch::None;
            }
            self.in_battle_music = true;
            Switch::Enter
        } else if self.in_battle_music {
            self.in_battle_music = false;
            Switch::Leave
        } else {
            Switch::None
        }
    }
}

/// Applies the fades of `FUN_00137458` (enter) and `FUN_001374c0` (leave) to the driver.
pub fn apply(driver: &mut Driver, sw: Switch, battle_seq: impl FnOnce() -> Option<kh2_audio::ssl::Sequence>) {
    match sw {
        Switch::Enter => {
            driver.bgm_fade(0, 0, timing::FIELD_OUT);
            driver.bgm_start(1, battle_seq(), 0, BGM_FULL, timing::BATTLE_IN, false, timing::BATTLE_IN_DELAY);
        }
        Switch::Leave => {
            driver.bgm_fade(1, 0, timing::BATTLE_OUT);
            driver.bgm_start(0, None, 0, BGM_FULL, timing::FIELD_IN, true, timing::FIELD_IN_DELAY);
        }
        Switch::None => {}
    }
}

/// The per-frame task. `KH2_NO_MUSIC` disables it (nothing is loaded then).
pub fn battle_music(battle: Res<Battle>, disc: Res<Disc>, audio: Res<Audio>, mut music: ResMut<AreaMusic>) {
    let sw = music.step(battle.active());
    if sw == Switch::None {
        return;
    }
    let n = music.battle.unwrap_or(0) as u32;
    info!("battle music: {sw:?} (field {:?}, battle {:?})", music.field, music.battle);
    if let Ok(mut s) = audio.0.lock()
        && let Some(d) = s.driver.as_mut()
    {
        apply(d, sw, || load_sequence(&disc, n));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn write_wav(path: &std::path::Path, pcm: &[f32]) {
        let data: Vec<u8> = pcm.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
        let mut b = Vec::new();
        b.extend(b"RIFF");
        b.extend((36 + data.len() as u32).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(2u16.to_le_bytes());
        b.extend(kh2_audio::RATE.to_le_bytes());
        b.extend((kh2_audio::RATE * 4).to_le_bytes());
        b.extend(4u16.to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(b"data");
        b.extend((data.len() as u32).to_le_bytes());
        b.extend(data);
        std::fs::write(path, b).unwrap();
    }

    /// Renders the field-to-battle-and-back switch of tt00 (music 118 / 119) and three of Sora's
    /// voice lines into the untracked local/ folder for listening. Skipped without a disc.
    #[test]
    fn render_listening_wavs() {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists());
        let Some(iso) = iso else { return };
        let disc = Disc::open(&iso);
        let exe = disc.read_root("LIBSSL.IRX").expect("irx");
        let tables = kh2_audio::ssl::Tables::from_irx(&exe).expect("tables");
        let mut d = Driver::new(Arc::new(tables));
        d.mixer.master = 0.6;
        let field = crate::audio::load_sequence(&disc, 118).expect("field music");
        d.play_bgm(field);
        let mut m = AreaMusic::default();
        m.set_area(118, 119);
        let mut pcm = Vec::new();
        let mut render = |d: &mut Driver, secs: f32| {
            let mut buf = vec![0f32; (secs * kh2_audio::RATE as f32) as usize * 2];
            for c in buf.chunks_mut(512) {
                d.render(c);
            }
            pcm.extend(buf);
        };
        render(&mut d, 3.0);
        apply(&mut d, m.step(true), || crate::audio::load_sequence(&disc, 119));
        render(&mut d, 14.0);
        apply(&mut d, m.step(false), || None);
        render(&mut d, 14.0);
        let dir = std::path::Path::new("../../local");
        std::fs::create_dir_all(dir).unwrap();
        write_wav(&dir.join("bgm_switch_tt00.wav"), &pcm);

        // Voice lines through the driver's pan law, centred.
        let mut v = Driver::new(d.tables.clone());
        v.mixer.master = 0.8;
        let bank = disc.read("voice/us/battle/tt0_sora.vsb").and_then(|b| kh2_audio::vsb::Vsb::parse(&b)).expect("bank");
        let mut out = Vec::new();
        for line in [9usize, 11, 16] {
            let (s, rate) = bank.line(line).expect("line");
            v.play_voice_at(Arc::new(s), rate, 0x3fff, 0x40);
            let mut buf = vec![0f32; kh2_audio::RATE as usize * 2 * 2];
            for c in buf.chunks_mut(512) {
                v.render(c);
            }
            out.extend(buf);
        }
        write_wav(&dir.join("voice_sora_tt0_9_11_16.wav"), &out);
        assert!(pcm.iter().any(|s| s.abs() > 0.01));
    }

    #[test]
    fn rise_and_fall_switch_once_and_only_with_distinct_numbers() {
        let mut m = AreaMusic::default();
        m.set_area(10, 10);
        assert_eq!(m.step(true), Switch::None);
        m.set_area(10, 11);
        assert_eq!(m.step(false), Switch::None);
        assert_eq!(m.step(true), Switch::Enter);
        assert_eq!(m.step(true), Switch::None);
        assert_eq!(m.step(false), Switch::Leave);
        assert_eq!(m.step(false), Switch::None);
        assert_eq!(m.step(true), Switch::Enter);
    }
}
