//! Audio: one endless stream that mixes everything the game plays (BGM sequences and sound
//! effects) with `kh2-audio`'s software SPU2, rendered on the audio thread in small chunks.

use std::collections::HashMap;
use std::num::NonZero;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::audio::{AddAudioSource, Decodable, Source};
use bevy::prelude::*;
use kh2_audio::ssl::{Driver, SeBlock, Sequence, Tables};
use kh2_audio::vsb::Vsb;
use kh2_audio::{RATE, Wd};

use crate::assets::{CurrentArea, Disc};
use crate::skel::Animator;
use crate::sora::Sora;

/// Everything the audio thread plays: the SSL driver (sequence players + SPU voices).
#[derive(Default)]
pub struct State {
    pub driver: Option<Driver>,
}

#[derive(Resource, Clone)]
pub struct Audio(pub Arc<Mutex<State>>);

/// The stream asset (Bevy plays it through its normal audio path).
#[derive(Asset, TypePath, Clone)]
pub struct Stream(Arc<Mutex<State>>);

pub struct StreamDecoder {
    state: Arc<Mutex<State>>,
    buf: Vec<f32>,
    pos: usize,
}

const CHUNK: usize = 256;

impl Iterator for StreamDecoder {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.pos >= self.buf.len() {
            self.buf.resize(CHUNK * 2, 0.0);
            match self.state.lock() {
                Ok(mut s) => match s.driver.as_mut() {
                    Some(d) => d.render(&mut self.buf),
                    None => self.buf.fill(0.0),
                },
                Err(_) => self.buf.fill(0.0),
            }
            self.pos = 0;
        }
        let v = self.buf[self.pos];
        self.pos += 1;
        Some(v)
    }
}

impl Source for StreamDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> NonZero<u16> {
        NonZero::new(2).unwrap()
    }
    fn sample_rate(&self) -> NonZero<u32> {
        NonZero::new(RATE).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Decodable for Stream {
    type Decoder = StreamDecoder;
    fn decoder(&self) -> StreamDecoder {
        StreamDecoder { state: self.0.clone(), buf: Vec::new(), pos: 0 }
    }
}

/// A sound-effect request: the game's SE number is `bank * 1000 + index`.
#[derive(Message, Clone, Debug)]
pub struct PlaySe {
    pub bank: u16,
    pub index: u16,
    /// The requesting object's own effect/sound file (`obj/<name>.apdx`); it carries that
    /// object's private bank (e.g. the keyblade's 500).
    pub apdx: Option<String>,
}

/// A voice line: line `line` of the voice bank `file` (e.g. `voice/us/battle/tt0_sora.vsb`).
#[derive(Message, Clone, Debug)]
pub struct PlayVoice {
    pub file: String,
    pub line: u16,
}

/// `PlaySe::bank` value meaning "the requesting object's own bank".
pub const OWN_BANK: u16 = 0xffff;

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
        Some(PlaySe { bank: OWN_BANK, index: base + (counter & 1) as u16, apdx })
    }

    pub fn number(n: u32, apdx: Option<String>) -> PlaySe {
        PlaySe { bank: (n / 1000) as u16, index: (n % 1000) as u16, apdx }
    }
}

/// An SE block with the WD it plays from (set for object-private banks; global banks pick the
/// WD per entry).
struct Bank {
    block: SeBlock,
    wd: Option<Arc<Wd>>,
}

#[derive(Resource, Default)]
pub struct SeCache {
    banks: Mutex<HashMap<String, Option<Arc<Bank>>>>,
    wds: Mutex<HashMap<u16, Option<Arc<Wd>>>>,
    voices: Mutex<HashMap<String, Option<Arc<Vsb>>>>,
}

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<Stream>()
            .insert_resource(Audio(Arc::new(Mutex::new(State::default()))))
            .init_resource::<SeCache>()
            .add_message::<PlaySe>()
            .add_message::<PlayVoice>()
            .add_systems(Startup, start_stream)
            .add_systems(Update, play_requests);
    }
}

fn start_stream(mut commands: Commands, disc: Res<Disc>, audio: Res<Audio>, mut streams: ResMut<Assets<Stream>>) {
    // The driver's pitch/pan tables come from the user's own LIBSSL.IRX on the disc.
    let tables = disc.read_root("LIBSSL.IRX").and_then(|b| Tables::from_irx(&b));
    match tables {
        Some(t) => {
            let mut d = Driver::new(Arc::new(t));
            d.mixer.master = 0.6;
            if let Ok(mut s) = audio.0.lock() {
                s.driver = Some(d);
            }
        }
        None => warn!("LIBSSL.IRX: not the USA sound driver; no audio"),
    }
    let h = streams.add(Stream(audio.0.clone()));
    commands.spawn(AudioPlayer(h));
}

/// Starts BGM `music<nnn>` with its WD bank (replacing the current one).
pub fn play_bgm(disc: &Disc, audio: &Audio, number: u32) -> bool {
    let Some(raw) = disc.read(&format!("bgm/music{number:03}.bgm")) else { return false };
    let Some(bgm) = kh2_audio::Bgm::parse(&raw) else { return false };
    let Some(wd) = disc.read(&format!("bgm/wave{:04}.wd", bgm.wd)).and_then(|b| Wd::parse(&b)) else { return false };
    let Some(seq) = Sequence::bgm(&raw, Arc::new(wd), 0) else { return false };
    if let Ok(mut s) = audio.0.lock() {
        if let Some(d) = s.driver.as_mut() {
            d.play_bgm(seq);
        }
    }
    true
}

fn load_bank(disc: &Disc, cache: &SeCache, bank: u16, apdx: Option<&str>) -> Option<Arc<Bank>> {
    // The object's private bank first.
    if let Some(name) = apdx {
        let found = cache
            .banks
            .lock()
            .ok()?
            .entry(name.to_string())
            .or_insert_with(|| {
                let raw = disc.read(name)?;
                let bar = kh2_bar::parse(&raw).ok()?;
                let block = bar.iter().find(|e| e.tag.starts_with("se")).and_then(|e| SeBlock::parse(e.data))?;
                let wd = bar.iter().find(|e| e.tag == "wave").and_then(|e| Wd::parse(e.data)).map(Arc::new);
                Some(Arc::new(Bank { block, wd }))
            })
            .clone();
        if let Some(b) = found.filter(|b| bank == OWN_BANK || b.block.bank == bank) {
            return Some(b);
        }
    }
    let name = format!("se/se{bank:03}.seb");
    cache
        .banks
        .lock()
        .ok()?
        .entry(name.clone())
        .or_insert_with(|| SeBlock::parse(&disc.read(&name)?).map(|block| Arc::new(Bank { block, wd: None })))
        .clone()
}

fn load_wd(disc: &Disc, cache: &SeCache, id: u16) -> Option<Arc<Wd>> {
    cache.wds.lock().ok()?.entry(id).or_insert_with(|| Wd::parse(&disc.read(&format!("se/wave{id:04}.wd"))?).map(Arc::new)).clone()
}

/// Plays a sound effect by bank and index.
pub fn play_se(disc: &Disc, audio: &Audio, cache: &SeCache, req: &PlaySe) -> bool {
    let Some(bank) = load_bank(disc, cache, req.bank, req.apdx.as_deref()) else { return false };
    let Some(entry) = bank.block.entries.get(req.index as usize) else { return false };
    let Some(wd) = bank.wd.clone().or_else(|| load_wd(disc, cache, entry.wd)) else { return false };
    let Ok(mut s) = audio.0.lock() else { return false };
    s.driver.as_mut().is_some_and(|d| d.play_se(&bank.block, req.index as usize, wd, 0x3fff, 0x40))
}

fn play_voices(disc: &Disc, audio: &Audio, cache: &SeCache, v: &PlayVoice) -> bool {
    let Some(bank) = cache.voices.lock().ok().and_then(|mut m| m.entry(v.file.clone()).or_insert_with(|| Vsb::parse(&disc.read(&v.file)?).map(Arc::new)).clone()) else {
        return false;
    };
    let Some((sample, rate)) = bank.line(v.line as usize) else { return false };
    let Ok(mut s) = audio.0.lock() else { return false };
    s.driver.as_mut().map(|d| d.play_voice(Arc::new(sample), rate, 0.9)).is_some()
}

fn play_requests(
    disc: Res<Disc>,
    audio: Res<Audio>,
    cache: Res<SeCache>,
    mut req: MessageReader<PlaySe>,
    mut voices: MessageReader<PlayVoice>,
) {
    if std::env::var_os("KH2_NO_SOUND").is_some() {
        req.clear();
        voices.clear();
        return;
    }
    for v in voices.read() {
        let ok = play_voices(&disc, &audio, &cache, v);
        if std::env::var_os("KH2_SOUNDLOG").is_some() {
            info!("voice {}:{} {}", v.file, v.line, if ok { "playing" } else { "not found" });
        }
    }
    let log = std::env::var_os("KH2_SOUNDLOG").is_some();
    for r in req.read() {
        let ok = play_se(&disc, &audio, &cache, r);
        if log {
            info!("se {}:{} ({}) {}", r.bank, r.index, r.apdx.as_deref().unwrap_or("-"), if ok { "playing" } else { "not found" });
        }
    }
}

/// The attack's `ReflectHitSound` when it strikes a wall (an SE number, -1 = none).
pub fn wall_sounds(
    data: Res<crate::data::GameData>,
    mut walls: MessageReader<crate::combat::WallHit>,
    anims: Query<&Animator>,
    mut out: MessageWriter<PlaySe>,
) {
    for w in walls.read() {
        let Some(atk) = data.atkp(w.atkp) else { continue };
        if atk.reflect_hit_sound >= 0 {
            out.write(PlaySe::number(atk.reflect_hit_sound as u32, anims.get(w.body).ok().and_then(|a| a.rig.apdx.clone())));
        }
    }
}

/// Motion frame trigger 8 ("sound effect"): params are [bank, index, ...].
pub fn motion_sounds(
    anims: Query<(Entity, &Animator)>,
    sora: Query<&Sora>,
    area: Res<CurrentArea>,
    mut out: MessageWriter<PlaySe>,
    mut voices: MessageWriter<PlayVoice>,
) {
    let sora_body = sora.single().map(|s| s.body).ok();
    for (e, a) in &anims {
        let is_sora = Some(e) == sora_body;
        let (Some(t), Some(m)) = (a.triggers(), a.motion()) else { continue };
        if t.frames.is_empty() {
            continue;
        }
        let g = |f: f32| f * crate::data::LOGIC_HZ / m.fps.max(1.0);
        let (from, to) = (g(a.prev_frame), g(a.frame));
        if from == to {
            continue;
        }
        use kh2_anim::triggers::frame;
        for ft in t.crossed(from, to) {
            match (ft.id, &ft.params[..]) {
                (frame::SOUND_EFFECT, [bank, index, ..]) => {
                    out.write(PlaySe { bank: *bank as u16, index: *index as u16, apdx: a.rig.apdx.clone() });
                }
                // Voice lines: the speaker's bank for the current world.
                (frame::VOICE | frame::VOICE2, [line, ..]) if is_sora => {
                    if let Some(world) = area.loaded.as_deref().and_then(|n| n.get(..2)) {
                        voices.write(PlayVoice { file: format!("voice/us/battle/{world}0_sora.vsb"), line: *line as u16 });
                    }
                }
                _ => {}
            }
        }
    }
}

/// World codes in the game's world order (OpenKH `docs/kh2/worlds.md`).
const WORLDS: [&str; 19] = ["zz", "es", "tt", "di", "hb", "bb", "he", "al", "mu", "po", "lk", "lm", "dc", "wi", "nm", "wm", "ca", "tr", "eh"];

/// Field and battle music of an area from `03system.bin`'s `arif` table (BGM set 0): a header
/// (version, world count, per world: u16 area count, u16 offset) and 0x40-byte area records with
/// eight (field, battle) pairs at 0x10. Layout per OpenKH `OpenKh.Kh2/SystemData/Arif.cs`.
pub fn area_music(disc: &Disc, area: &str) -> Option<(u16, u16)> {
    let (w, a) = area.split_at(2);
    let world = WORLDS.iter().position(|x| *x == w)?;
    let index: usize = a.parse().ok()?;
    let sys = disc.read("03system.bin")?;
    let arif = kh2_bar::parse(&sys).ok()?.into_iter().find(|e| e.tag == "arif")?.data;
    let u16_at = |o: usize| arif.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]));
    let count = u16_at(8 + 4 * world)? as usize;
    let off = u16_at(8 + 4 * world + 2)? as usize;
    if index >= count {
        return None;
    }
    let rec = off + 0x40 * index;
    Some((u16_at(rec + 0x10)?, u16_at(rec + 0x12)?))
}

/// Plays the area's field music whenever a new area finishes loading.
pub fn area_bgm(disc: Res<Disc>, audio: Res<Audio>, area: Res<crate::assets::CurrentArea>, mut last: Local<Option<String>>) {
    let Some(loaded) = area.loaded.clone() else { return };
    if last.as_deref() == Some(loaded.as_str()) {
        return;
    }
    *last = Some(loaded.clone());
    if std::env::var_os("KH2_NO_MUSIC").is_some() {
        return;
    }
    match area_music(&disc, &loaded) {
        Some((field, battle)) => {
            info!("{loaded}: music field {field} battle {battle}");
            if !play_bgm(&disc, &audio, field as u32) {
                warn!("music {field}: cannot load");
            }
        }
        None => warn!("{loaded}: no area music"),
    }
}
