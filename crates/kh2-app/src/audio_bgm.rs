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
//! A frame of the fades is one 240 Hz driver frame (`kh2_audio::ssl::FRAME_HZ`).
//!
//! The battle flag is the game's own ([`BattleFlag`], docs/audio.md "The battle flag"): it is
//! wanted while a normal enemy stands within 1200 units of the player horizontally
//! (`FUN_00190900`), a limit runs or a friend's script thread runs (`FUN_001904e0`), and it follows
//! that wish with a hold of 45 ticks up and 120 down (`FUN_001903e8`). Sora's attacks do not
//! raise it (the keyblade-out timer `sora::Battle` is a different thing).

use bevy::prelude::*;
use kh2_audio::ssl::{BGM_FULL, Driver};

use crate::assets::{CurrentArea, Disc};
use crate::audio::{Audio, load_sequence};
use crate::enemy::Enemy;
use crate::sora::Sora;
use crate::SCALE;

/// The battle flag's constants, read from the executable's data (`FUN_00190370`, `FUN_001903e8`,
/// `FUN_00190900`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlagParams {
    /// Ticks after a reset during which a change of the wish is taken at once (0x37ad14).
    pub grace: f32,
    /// Hold before the flag falls, reloaded while it is 1 (0x37ad18).
    pub hold_fall: f32,
    /// Hold before the flag rises, reloaded while it is 0 (0x37ad1c).
    pub hold_rise: f32,
    /// Horizontal distance, game units, under which an enemy raises the wish (0x37ad20).
    pub near: f32,
}

/// Virtual address of the first byte of the main executable image; file offset = VA - 0xff000.
const EXE_BIAS: usize = 0xff000;

impl FlagParams {
    pub fn from_exe(exe: &[u8]) -> Option<FlagParams> {
        let f = |va: usize| -> Option<f32> {
            let o = va.checked_sub(EXE_BIAS)?;
            exe.get(o..o + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };
        let p = FlagParams { grace: f(0x37ad14)?, hold_fall: f(0x37ad18)?, hold_rise: f(0x37ad1c)?, near: f(0x37ad20)? };
        let ok = |v: f32, hi: f32| v.is_finite() && v > 0.0 && v < hi;
        (ok(p.grace, 1000.0) && ok(p.hold_fall, 1000.0) && ok(p.hold_rise, 1000.0) && ok(p.near, 100_000.0)).then_some(p)
    }
}

/// The game's battle flag (word 0x1c8a7d8) with its two timers (0x1c8a7dc grace, 0x1c8a7e0
/// hold). `params` is `None` until the executable was read; the flag then stays 0.
#[derive(Resource, Default, Debug)]
pub struct BattleFlag {
    /// 0, 1, or 2 (forced by a script, `FUN_001905a8`; nothing in the app sets it yet).
    pub state: u8,
    grace: f32,
    hold: f32,
    pub params: Option<FlagParams>,
    /// The area the flag was last reset for.
    area: Option<String>,
    /// The executable was read for `params` (once).
    read: bool,
}

impl BattleFlag {
    /// `FUN_001903d8`.
    pub fn active(&self) -> bool {
        self.state != 0
    }

    /// `FUN_00190370` (area setup): flag 0, hold 0, grace from the table.
    pub fn reset(&mut self) {
        self.state = 0;
        self.hold = 0.0;
        self.grace = self.params.map_or(0.0, |p| p.grace);
    }

    /// One tick of `FUN_001903e8` with the wish of `FUN_001904e0`: `None` when the player object
    /// is not valid (false, and the hold timer zeroed), else the wish. Returns the new state when it
    /// changed.
    pub fn step(&mut self, wish: Option<bool>) -> Option<u8> {
        let p = self.params?;
        if self.state == 2 {
            return None;
        }
        let want = match wish {
            None => {
                self.hold = 0.0;
                0
            }
            Some(w) => u8::from(w),
        };
        if self.grace > 0.0 {
            self.grace -= 1.0;
        }
        let mut changed = None;
        if self.state != want && (self.grace > 0.0 || {
            self.hold -= 1.0;
            self.hold <= 0.0
        }) {
            self.state = want;
            changed = Some(want);
        }
        if self.state == want {
            self.hold = if want == 1 { p.hold_fall } else { p.hold_rise };
        }
        changed
    }
}

/// The part of `FUN_001904e0`'s wish the app can have: `FUN_00190900`'s test, a living normal
/// enemy (objentry type 4: +0x588 bit 0x10, team +0x3b0 = 2) whose horizontal distance to the player
/// is under `near`. Positions in game units; the y component is ignored. (No boss, limit task or
/// friend thread exists in the app, so the other terms are false.)
pub fn battle_flag_desire(player: Vec3, enemies: impl IntoIterator<Item = Vec3>, near: f32) -> bool {
    enemies.into_iter().any(|e| (e - player).xz().length() < near)
}

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

/// The battle flag's tick (`FUN_001903e8`, once per 60 Hz tick), then the music task
/// (`FUN_001373d0`). The flag is reset when an area finishes loading (`FUN_00190370`). With
/// `KH2_NO_MUSIC` the flag still runs but no music switches (nothing is loaded).
#[allow(clippy::too_many_arguments)] // Bevy system parameters
pub fn battle_music(
    mut flag: ResMut<BattleFlag>,
    area: Res<CurrentArea>,
    sora: Query<&Sora>,
    enemies: Query<(&GlobalTransform, &Enemy)>,
    disc: Res<Disc>,
    audio: Res<Audio>,
    mut music: ResMut<AreaMusic>,
) {
    let flag = &mut *flag;
    if !flag.read {
        flag.read = true;
        flag.params = disc.read_root("SLUS_210.05").and_then(|exe| FlagParams::from_exe(&exe));
        if flag.params.is_none() {
            warn!("battle flag: could not read its constants from SLUS_210.05; the flag stays 0");
        }
    }
    if let Some(loaded) = area.loaded.as_deref()
        && flag.area.as_deref() != Some(loaded)
    {
        flag.area = Some(loaded.to_owned());
        flag.reset();
    }
    let near = flag.params.map_or(0.0, |p| p.near);
    let wish = sora.single().ok().map(|s| {
        let me = s.motor.pos;
        battle_flag_desire(me, enemies.iter().filter(|(_, e)| e.alive()).map(|(g, _)| g.translation() / SCALE), near)
    });
    if let Some(now) = flag.step(wish) {
        let me = sora.single().map(|s| s.motor.pos).unwrap_or_default();
        let nearest = enemies
            .iter()
            .filter(|(_, e)| e.alive())
            .map(|(g, e)| (e.name, (g.translation() / SCALE - me).xz().length()))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        info!(
            "battle flag: {} -> {now} (nearest living enemy {}, threshold {near:.0} units)",
            1 - now.min(1),
            nearest.map_or("none".to_owned(), |(n, d)| format!("{n} at {d:.0} units"))
        );
    }
    let sw = music.step(flag.active());
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

    fn flag(grace: f32) -> BattleFlag {
        let mut f = BattleFlag { params: Some(FlagParams { grace, hold_fall: 120.0, hold_rise: 45.0, near: 1200.0 }), ..Default::default() };
        f.reset();
        f
    }

    /// Ticks of `wish` until the flag changes (at most `max`).
    fn ticks_to_change(f: &mut BattleFlag, wish: Option<bool>, max: u32) -> Option<u32> {
        (1..=max).find(|_| f.step(wish).is_some())
    }

    #[test]
    fn battle_flag_rises_after_45_and_falls_after_120() {
        let mut f = flag(0.0);
        // Settle at 0: the rise hold is loaded while flag and wish agree.
        assert_eq!(f.step(Some(false)), None);
        assert_eq!(ticks_to_change(&mut f, Some(true), 500), Some(45));
        assert!(f.active());
        assert_eq!(ticks_to_change(&mut f, Some(false), 500), Some(120));
        assert!(!f.active());
        // A wish that comes back before the hold is over reloads it.
        for _ in 0..30 {
            assert_eq!(f.step(Some(true)), None);
        }
        assert_eq!(f.step(Some(false)), None);
        assert_eq!(ticks_to_change(&mut f, Some(true), 500), Some(45));
        // No valid player: the wish is false and the hold is zeroed, so the flag falls at once.
        assert_eq!(ticks_to_change(&mut f, None, 500), Some(1));
        // A script's state 2 is left alone.
        f.state = 2;
        assert_eq!(ticks_to_change(&mut f, Some(false), 500), None);
    }

    #[test]
    fn battle_flag_grace_after_reset_switches_at_once() {
        let mut f = flag(30.0);
        // The grace timer is decremented before the test: changes are immediate for 29 ticks.
        assert_eq!(f.step(Some(true)), Some(1));
        assert_eq!(f.step(Some(false)), Some(0));
        for _ in 0..27 {
            f.step(Some(false));
        }
        // Tick 30: the grace is over, the hold rules.
        assert_eq!(ticks_to_change(&mut f, Some(true), 500), Some(45));
        // Without constants the flag never rises.
        let mut none = BattleFlag::default();
        assert_eq!(ticks_to_change(&mut none, Some(true), 500), None);
    }

    #[test]
    fn battle_flag_desire_is_horizontal_distance_under_1200() {
        let me = Vec3::new(100.0, 0.0, -35.0);
        assert!(!battle_flag_desire(me, [], 1200.0), "no enemy, no battle (attacks do not count)");
        // 1199 away on x: near. Height does not count.
        assert!(battle_flag_desire(me, [me + Vec3::new(1199.0, 5000.0, 0.0)], 1200.0));
        // Exactly 1200 is not under the threshold.
        assert!(!battle_flag_desire(me, [me + Vec3::new(0.0, 0.0, 1200.0)], 1200.0));
        assert!(!battle_flag_desire(me, [me + Vec3::new(900.0, 0.0, 900.0)], 1200.0), "1273 diagonally");
        assert!(battle_flag_desire(me, [me + Vec3::new(5000.0, 0.0, 0.0), me + Vec3::new(800.0, -300.0, 800.0)], 1200.0));
    }

    #[test]
    fn battle_flag_constants_from_exe() {
        // A fake image with the four constants, to check the addresses.
        let mut exe = vec![0u8; 0x37b000 - EXE_BIAS];
        for (va, v) in [(0x37ad14usize, 30.0f32), (0x37ad18, 120.0), (0x37ad1c, 45.0), (0x37ad20, 1200.0)] {
            exe[va - EXE_BIAS..va - EXE_BIAS + 4].copy_from_slice(&v.to_le_bytes());
        }
        let p = FlagParams::from_exe(&exe).expect("constants");
        assert_eq!(p, FlagParams { grace: 30.0, hold_fall: 120.0, hold_rise: 45.0, near: 1200.0 });
        assert!(FlagParams::from_exe(&exe[..0x1000]).is_none());
        assert!(FlagParams::from_exe(&vec![0u8; 0x37b000 - EXE_BIAS]).is_none());
        // The user's executable: the four read and the fall hold is the longer one.
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped the disc part: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let p = disc.read_root("SLUS_210.05").and_then(|e| FlagParams::from_exe(&e)).expect("constants on the disc");
        eprintln!("battle flag constants: {p:?}");
        assert!(p.hold_fall > p.hold_rise && p.near > 100.0, "{p:?}");
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
