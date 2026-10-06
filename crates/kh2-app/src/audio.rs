//! Audio: one endless stream that mixes everything the game plays (BGM sequences and sound
//! effects) with `kh2-audio`'s software SPU2, rendered on the audio thread in small chunks.

use std::collections::HashMap;
use std::num::NonZero;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::audio::{AddAudioSource, Decodable, Source};
use bevy::prelude::*;
use kh2_audio::ssl::{Driver, Handle, SeBlock, Sequence, Tables};
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
    /// World position in metres of the sound's source. The game starts every object sound at the
    /// object's position (FUN_00197008, FUN_0016baa0) and derives volume and pan from it
    /// (`kh2_audio::positional`); `None` plays at full volume and centre pan like its interface
    /// sounds.
    pub pos: Option<Vec3>,
}

/// A voice line: line `line` of the voice bank `file` (e.g. `voice/us/battle/tt0_sora.vsb`).
#[derive(Message, Clone, Debug)]
pub struct PlayVoice {
    pub file: String,
    pub line: u16,
    /// The record's priority byte (`vtbl` +2): a line with a lower priority than the one queued
    /// or playing on the speaker's slot is dropped (`FUN_001ae798`).
    pub priority: u8,
    /// World position in metres of the speaker (voice lines are positional sounds of kind 5).
    pub pos: Option<Vec3>,
}

/// The voice table (`vtbl` of `00battle.bin`): a motion names a voice ID, and the table says which
/// lines of the speaker's voice bank can answer it.
#[derive(Resource, Default)]
pub struct VoiceTable(Vec<kh2_tables::tables::Vtbl>);

/// Sora's entry in the game's speaker list (`sora`, `donald`, `goofy`, ...); his voice bank is the
/// `<world>0_sora.vsb` file.
pub const SORA_SPEAKER: u16 = kh2_audio::voice::speaker::SORA;

impl VoiceTable {
    /// The line the game plays for `voice` of `speaker` (`FUN_001ae538`, ported in
    /// `kh2_audio::voice`): the five (line, chance) pairs of the record are tried in order, a pair
    /// with a chance wins when the percent roll runs out in it, pairs without one form a pool that
    /// is drawn from when no chance pair won, and a line of -3 is the other party's line (`other`
    /// is its NeoStatus). `rng` is the game's shared generator ([`GameRng`]). Returns the line and
    /// the record's priority.
    pub fn pick(&self, speaker: u16, voice: u16, other: u16, rng: &mut u32, flags: &impl Fn(u16) -> bool) -> Option<(u16, u8)> {
        use kh2_audio::voice::{pick_line, resolve_line};
        let rec = self.0.iter().find(|r| r.character_id as u16 == speaker && r.id as u16 == voice)?;
        let mut pairs = [0i8; 10];
        for (p, v) in pairs.iter_mut().zip(rec.voices.iter()) {
            *p = *v;
        }
        let line = pick_line(&pairs, rng, |l| resolve_line(l, speaker, other, flags));
        (line >= 0).then_some((line as u16, rec.priority))
    }
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

/// Speaker names and the voice language folder, read from the executable at start.
#[derive(Resource, Default)]
pub struct VoiceInfo {
    pub names: kh2_audio::voice::SpeakerNames,
    pub lang: String,
}

fn load_voice_info(disc: Res<Disc>, mut info: ResMut<VoiceInfo>) {
    let Some(exe) = disc.read_root("SLUS_210.05") else { return };
    info.names = kh2_audio::voice::SpeakerNames::from_exe(&exe).unwrap_or_default();
    info.lang = kh2_audio::voice::language(&exe).unwrap_or_else(|| "us".into());
    info!("voice: language folder {}, {} speaker names", info.lang, info.names.0.iter().flatten().count());
}

fn load_voice_table(disc: Res<Disc>, mut table: ResMut<VoiceTable>) {
    let mut a = disc.0.lock().unwrap();
    match kh2_tables::load::<kh2_tables::tables::Vtbl>(&mut a, &kh2_tables::tables::VTBL) {
        Ok(t) => table.0 = t.rows,
        Err(e) => warn!("vtbl: {e}; voices play by raw line number"),
    }
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
        Some(PlaySe { bank: OWN_BANK, index: base + (counter & 1) as u16, apdx, pos: None })
    }

    pub fn number(n: u32, apdx: Option<String>) -> PlaySe {
        PlaySe { bank: (n / 1000) as u16, index: (n % 1000) as u16, apdx, pos: None }
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
            .init_resource::<VoiceTable>()
            .init_resource::<GameRng>()
            .init_resource::<VoiceInfo>()
            .init_resource::<LiveMixes>()
            .init_resource::<crate::audio_bgm::AreaMusic>()
            .add_message::<PlaySe>()
            .add_message::<PlayVoice>()
            .add_systems(Startup, (start_stream, load_voice_table, load_voice_info))
            .add_systems(Update, play_requests)
            .add_systems(Update, area_bgm.after(crate::assets::load_area))
            .add_systems(
                FixedUpdate,
                (motion_sounds, footsteps, wall_sounds, crate::audio_bgm::battle_music, update_mixes).chain().in_set(crate::schedule::Sim::Sounds),
            );
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

/// The sequence of BGM `music<nnn>` with its WD bank.
pub fn load_sequence(disc: &Disc, number: u32) -> Option<Sequence> {
    let raw = disc.read(&format!("bgm/music{number:03}.bgm"))?;
    let bgm = kh2_audio::Bgm::parse(&raw)?;
    let wd = disc.read(&format!("bgm/wave{:04}.wd", bgm.wd)).and_then(|b| Wd::parse(&b))?;
    Sequence::bgm(&raw, Arc::new(wd), 0)
}

/// Starts BGM `music<nnn>` in the field slot at the full level (replacing the current one).
pub fn play_bgm(disc: &Disc, audio: &Audio, number: u32) -> bool {
    let Some(seq) = load_sequence(disc, number) else { return false };
    if let Ok(mut s) = audio.0.lock()
        && let Some(d) = s.driver.as_mut()
    {
        d.bgm_stop(1, false);
        d.play_bgm(seq);
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

/// Plays a sound effect by bank and index. The handle and the entry's flags go to [`LiveMixes`].
pub fn play_se(disc: &Disc, audio: &Audio, cache: &SeCache, req: &PlaySe, volume_pan: (i32, i32)) -> Option<(Handle, u8)> {
    let bank = load_bank(disc, cache, req.bank, req.apdx.as_deref())?;
    let entry = bank.block.entries.get(req.index as usize)?;
    let wd = bank.wd.clone().or_else(|| load_wd(disc, cache, entry.wd))?;
    let mut s = audio.0.lock().ok()?;
    let h = s.driver.as_mut()?.play_se_handle(&bank.block, req.index as usize, wd, volume_pan.0, volume_pan.1)?;
    Some((h, entry.flags))
}

/// How a live sound is re-mixed every tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LiveKind {
    /// A sound effect with its entry flags (fixed volume / fixed pan).
    Se(u8),
    /// A voice line (kind 5) with its minimum volume.
    Voice(i32),
}

/// A playing positional sound. The game's request slot (FUN_002c2380 / FUN_002c2530 via
/// FUN_002c2760) copies the position once at request time; every tick FUN_002c2e90 recomputes the
/// volume and pan from that fixed position and the camera's current view matrix and sends them to
/// the driver (FUN_002be564, FUN_002bef1c). Only kind 4 sounds (map emitters, FUN_002c2b08) follow
/// an actor, and those are not ported. So the moving part is the listener, not the source.
#[derive(Clone, Copy, Debug)]
pub struct LiveSound {
    pub handle: Handle,
    pub kind: LiveKind,
    /// World position in metres at the time of the request.
    pub pos: Vec3,
    /// The last volume and pan sent.
    pub last: (i32, i32),
}

/// The positional sounds that are still playing.
#[derive(Resource, Default)]
pub struct LiveMixes(pub Vec<LiveSound>);

/// Volume and pan of a live sound for a listener (camera) transform.
pub fn live_mix(sound: &LiveSound, listener: &Transform) -> (i32, i32) {
    use kh2_audio::positional::{se_mix, voice_mix};
    let d = (sound.pos - listener.translation) / crate::SCALE;
    let rel = [d.dot(*listener.right()), d.dot(*listener.up()), d.dot(*listener.forward())];
    match sound.kind {
        LiveKind::Se(flags) => se_mix(rel, flags),
        LiveKind::Voice(min) => voice_mix(rel, min),
    }
}

/// The per-tick mix update of every live positional sound (FUN_002c30d0 -> FUN_002c2e90).
fn update_mixes(audio: Res<Audio>, mut live: ResMut<LiveMixes>, listener: Query<&Transform, With<crate::camera::OrbitCamera>>) {
    if live.0.is_empty() {
        return;
    }
    let Ok(l) = listener.single() else { return };
    let Ok(mut s) = audio.0.lock() else { return };
    let Some(d) = s.driver.as_mut() else { return };
    let log = std::env::var_os("KH2_SOUNDLOG").is_some();
    live.0.retain_mut(|snd| {
        let playing = match snd.kind {
            LiveKind::Se(_) => d.se_playing(snd.handle),
            LiveKind::Voice(_) => d.voice_playing(snd.handle),
        };
        if !playing {
            return false;
        }
        let mix = live_mix(snd, l);
        match snd.kind {
            LiveKind::Se(_) => d.set_se_mix(snd.handle, mix.0, mix.1),
            LiveKind::Voice(_) => d.set_voice_mix(snd.handle, mix.0, mix.1),
        }
        if log && mix != snd.last {
            info!("mix {:?} handle {} volume {:#x} pan {:#x}", snd.kind, snd.handle.0, mix.0, mix.1);
        }
        snd.last = mix;
        true
    });
}

/// Plays a voice line at the speaker's slot rules: a lower priority than the queued or playing
/// line is dropped, a started line stops the one before it (`FUN_001ae798`, `FUN_001ae7d8`), and
/// the sound is positional with a minimum volume (`kh2_audio::voice`).
fn play_voices(
    disc: &Disc,
    audio: &Audio,
    cache: &SeCache,
    v: &PlayVoice,
    vp: (i32, i32),
    slot: &mut kh2_audio::voice::VoiceSlot,
) -> Option<Handle> {
    let bank = cache.voices.lock().ok().and_then(|mut m| m.entry(v.file.clone()).or_insert_with(|| Vsb::parse(&disc.read(&v.file)?).map(Arc::new)).clone())?;
    let (sample, rate) = bank.line(v.line as usize)?;
    let mut s = audio.0.lock().ok()?;
    let d = s.driver.as_mut()?;
    if let Some(h) = slot.playing
        && !d.voice_playing(kh2_audio::ssl::Handle(h))
    {
        slot.playing = None;
    }
    slot.request(v.line, v.priority);
    slot.tick(|_| true)?;
    if let Some(h) = slot.playing.take() {
        d.stop_voice(kh2_audio::ssl::Handle(h));
    }
    let volume = vp.0.max(kh2_audio::voice::MIN_VOLUME);
    let pan = if volume > 0x39ff { kh2_audio::positional::CENTRE_PAN } else { vp.1 };
    let h = d.play_voice_at(Arc::new(sample), rate, volume, pan);
    slot.playing = Some(h.0);
    Some(h)
}

#[allow(clippy::too_many_arguments)]
fn play_requests(
    disc: Res<Disc>,
    audio: Res<Audio>,
    cache: Res<SeCache>,
    listener: Query<&Transform, With<crate::camera::OrbitCamera>>,
    mut req: MessageReader<PlaySe>,
    mut voices: MessageReader<PlayVoice>,
    mut slot: Local<kh2_audio::voice::VoiceSlot>,
    mut live: ResMut<LiveMixes>,
) {
    if std::env::var_os("KH2_NO_SOUND").is_some() {
        req.clear();
        voices.clear();
        return;
    }
    for v in voices.read() {
        let vp = placement(v.pos, listener.single().ok());
        let h = play_voices(&disc, &audio, &cache, v, vp, &mut slot);
        if let (Some(h), Some(pos)) = (h, v.pos) {
            live.0.push(LiveSound { handle: h, kind: LiveKind::Voice(kh2_audio::voice::MIN_VOLUME), pos, last: vp });
        }
        if std::env::var_os("KH2_SOUNDLOG").is_some() {
            info!("voice {}:{} {}", v.file, v.line, if h.is_some() { "playing" } else { "not found" });
        }
    }
    let log = std::env::var_os("KH2_SOUNDLOG").is_some();
    for r in req.read() {
        let vp = placement(r.pos, listener.single().ok());
        let started = play_se(&disc, &audio, &cache, r, vp);
        let ok = started.is_some();
        if let (Some((handle, flags)), Some(pos)) = (started, r.pos) {
            live.0.push(LiveSound { handle, kind: LiveKind::Se(flags), pos, last: vp });
        }
        if log {
            info!(
                "se {}:{} ({}) volume {:#x} pan {:#x}{} {}",
                r.bank,
                r.index,
                r.apdx.as_deref().unwrap_or("-"),
                vp.0,
                vp.1,
                if r.pos.is_some() { " (positional)" } else { "" },
                if ok { "playing" } else { "not found" }
            );
        }
    }
}

/// The fixed wall-hit sound 500010 (FUN_0018ca78), played from the object's own bank when the
/// attack volume belongs to an attached object such as the keyblade (owner flag 0x40). Not the
/// attack's `ReflectHitSound`, which is the sound of a hit that lands on a victim.
pub fn wall_sounds(
    mut walls: MessageReader<crate::combat::WallHit>,
    anims: Query<&Animator>,
    places: Query<&GlobalTransform>,
    mut out: MessageWriter<PlaySe>,
) {
    for w in walls.read().filter(|w| w.weapon_volume) {
        let mut se = PlaySe::number(crate::combat::WALL_HIT_SOUND, anims.get(w.body).ok().and_then(|a| a.rig.apdx.clone()));
        se.pos = places.get(w.body).ok().map(|g| g.translation());
        out.write(se);
    }
}

/// Motion frame trigger 8 ("sound effect"): params are [bank, index, ...]. Triggers 13 and 14
/// name a voice ID (not a line): the voice table picks the line.
#[allow(clippy::too_many_arguments)]
pub fn motion_sounds(
    anims: Query<(Entity, &Animator, &GlobalTransform)>,
    sora: Query<&Sora>,
    area: Res<CurrentArea>,
    table: Res<VoiceTable>,
    mut rng: ResMut<GameRng>,
    info: Res<VoiceInfo>,
    disc: Res<Disc>,
    enemies: Query<&crate::enemy::Enemy>,
    mut neo_cache: Local<HashMap<&'static str, u16>>,
    mut out: MessageWriter<PlaySe>,
    mut voices: MessageWriter<PlayVoice>,
) {
    let sora_body = sora.single().map(|s| s.body).ok();
    for (e, a, place) in &anims {
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
                    out.write(PlaySe { bank: *bank as u16, index: *index as u16, apdx: a.rig.apdx.clone(), pos: Some(place.translation()) });
                }
                // Voice lines: the speaker's bank for the current world.
                (frame::VOICE | frame::VOICE2, [voice, ..]) if is_sora => {
                    if let Some(area_name) = area.loaded.as_deref()
                        && let Some(world) = area_name.get(..2)
                    {
                        // The other party is the lock-on target's NeoStatus (FUN_00175d18 takes the
                        // actor's target object +0x9f4; that it is the lock-on target is inferred).
                        let other = sora
                            .single()
                            .ok()
                            .and_then(|s| s.target)
                            .and_then(|t| enemies.get(t).ok())
                            .and_then(|e| {
                                let model = match e.name {
                                    "Shadow" => "M_EX020",
                                    "Soldier" => "M_EX010",
                                    _ => return None,
                                };
                                Some(*neo_cache.entry(e.name).or_insert_with(|| neo_status(&disc, model).unwrap_or(0)))
                            })
                            .unwrap_or(0);
                        let picked = table.pick(SORA_SPEAKER, *voice as u16, other, &mut rng.0, &|_| false);
                        let bank = area_voice_bank(&disc, area_name).unwrap_or(0);
                        let name = info.names.name(SORA_SPEAKER).unwrap_or("sora");
                        if std::env::var_os("KH2_SOUNDLOG").is_some() {
                            info!("voice id {voice} ({}) other {other} -> line {picked:?} rng {:#x}", a.tag(), rng.0);
                        }
                        if let Some((line, priority)) = picked {
                            voices.write(PlayVoice {
                                file: kh2_audio::voice::bank_path(&info.lang, world, bank, name),
                                line,
                                priority,
                                pos: Some(place.translation()),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// The soundinfo record (`12soundinfo.bar`) of an area such as `tt02`: world entry by code, then
/// the record of the area number.
pub fn area_soundinfo(disc: &Disc, area: &str) -> Option<kh2_audio::footstep::SoundInfoRecord> {
    let (world, number) = area.split_at(2);
    let raw = disc.read("12soundinfo.bar")?;
    let entry = kh2_bar::parse(&raw).ok()?.into_iter().find(|e| e.tag == world)?;
    kh2_audio::footstep::WorldSoundInfo::parse(entry.data)?.area(number.parse().ok()?).copied()
}

/// `NeoStatus` of an object's `00objentry` row (picks the footstep bank).
fn neo_status(disc: &Disc, model: &str) -> Option<u16> {
    use kh2_tables::tables as t;
    let mut a = disc.0.lock().ok()?;
    kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()?.rows.into_iter().find(|r| r.model_name == model).map(|r| r.neo_status)
}

/// Sora's model, whose `objentry` row names his `NeoStatus`.
const SORA_MODEL: &str = "P_EX100";

/// Motion frame trigger 2 (footstep sound): `params[0]` is the `kind` the game passes to
/// FUN_0016baa0 (0 run, 4 walk, 9..11 other steps, per Sora's motions; 8 is the stop step below). The sound is
/// `bank * 1000 + material * 16 + variant`: bank from the area's soundinfo record by `NeoStatus`,
/// material of the ground polygon under Sora's feet (byte 0 of its COCT record; 0 is silent).
/// Only Sora's footsteps are handled: Shadow and Soldier motions carry no trigger 2.
///
/// Also the stop step (kind 8) of the ground mover, `kh2_audio::footstep::stop_step`: on the tick
/// the stick is let go while the last requested motion was walk or run. The app's controller has
/// already asked for idle by the time this runs (as `FUN_0017aeb8` does right after the step), so
/// the motion id and range trigger 42 are the ones of the previous tick. The stick magnitude
/// stands for the commanded speed (+0x1c is the walk or run speed exactly when the stick is out).
#[allow(clippy::too_many_arguments)]
pub fn footsteps(
    anims: Query<&Animator>,
    sora: Query<&Sora>,
    pad: Option<Res<crate::sora::Pad>>,
    area: Res<CurrentArea>,
    disc: Res<Disc>,
    cache: Res<SeCache>,
    mut info: Local<Option<(String, Option<kh2_audio::footstep::SoundInfoRecord>)>>,
    mut neo: Local<Option<Option<u16>>>,
    mut counter: Local<u8>,
    mut last: Local<Option<(u32, bool)>>,
    mut out: MessageWriter<PlaySe>,
) {
    let Ok(s) = sora.single() else { return };
    let Ok(a) = anims.get(s.body) else { return };
    // The ground mover (FUN_0017af10) runs for a grounded Sora in his ground state; he is a party actor.
    let commanded = pad.as_ref().map_or(0.0, |p| p.stick);
    let stop = last.is_some_and(|(id, r42)| {
        s.state == crate::sora::State::Ground && s.motor.grounded && kh2_audio::footstep::stop_step(true, commanded, r42, id)
    });
    *last = Some((a.slot.map_or(u32::MAX, |sl| (sl / 4) as u32), a.trigger(kh2_anim::triggers::range::KEEP_MOMENTUM)));
    let mut kinds: Vec<(u16, String)> = Vec::new();
    if stop {
        kinds.push((kh2_audio::footstep::kind::STOP, "stop (FUN_0017af10)".to_string()));
    }
    if let (Some(t), Some(m)) = (a.triggers(), a.motion()) {
        let g = |f: f32| f * crate::data::LOGIC_HZ / m.fps.max(1.0);
        let (from, to) = (g(a.prev_frame), g(a.frame));
        if from != to {
            for ft in t.crossed(from, to).filter(|f| f.id == kh2_anim::triggers::frame::FOOTSTEP_SOUND) {
                kinds.push((ft.params.first().copied().unwrap_or(0).max(0) as u16, format!("frame {}", ft.frame)));
            }
        }
    }
    if kinds.is_empty() {
        return;
    }
    let Some(name) = area.loaded.clone() else { return };
    if info.as_ref().is_none_or(|(n, _)| *n != name) {
        *info = Some((name.clone(), area_soundinfo(&disc, &name)));
    }
    let record = info.as_ref().and_then(|(_, r)| r.as_ref());
    let neo_status = neo.get_or_insert_with(|| neo_status(&disc, SORA_MODEL)).unwrap_or(1);
    // FUN_00183730: the ground material of the polygon in contact (0 in the air: no contact).
    // (Sora's descriptor flags: bit 0x20 is not set for him as far as the footstep logic shows.)
    let material = kh2_audio::footstep::actor_material(0, s.motor.ground.map_or(0, |g| g.material));
    let log = std::env::var_os("KH2_SOUNDLOG").is_some();
    for (kind, at) in kinds {
        let number = kh2_audio::footstep::footstep_number(record, neo_status, material, kind, &mut counter);
        if log {
            let sample = number.and_then(|n| {
                let bank = load_bank(&disc, &cache, (n / 1000) as u16, None)?;
                let e = bank.block.entries.get((n % 1000) as usize)?;
                Some(format!("wd {} tracks {}", e.wd, e.tracks.len()))
            });
            info!("footstep {} {at} kind {kind} material {material} neo {neo_status} -> {number:?} {}", a.tag(), sample.as_deref().unwrap_or("no sample"));
        }
        if let Some(n) = number {
            // FUN_0016baa0 passes the actor's position, so the step is placed at Sora's feet.
            out.write(PlaySe::number(n, None).at(s.motor.pos * crate::SCALE));
        }
    }
}

/// World codes in the game's world order (OpenKH `docs/kh2/worlds.md`).
const WORLDS: [&str; 19] = ["zz", "es", "tt", "di", "hb", "bb", "he", "al", "mu", "po", "lk", "lm", "dc", "wi", "nm", "wm", "ca", "tr", "eh"];

/// Field and battle music of an area from `03system.bin`'s `arif` table (BGM set 0): a header
/// (version, world count, per world: u16 area count, u16 offset) and 0x40-byte area records with
/// eight (field, battle) pairs at 0x10. Layout per OpenKH `OpenKh.Kh2/SystemData/Arif.cs`.
pub fn area_music(disc: &Disc, area: &str) -> Option<(u16, u16)> {
    let (_, field, battle) = area_arif(disc, area)?;
    Some((field, battle))
}

/// The voice bank number of an area (`arif` record +0x30, `FUN_001933a0` / `FUN_001ae180`): the
/// `<n>` of `voice/<lang>/battle/<world><n>_<speaker>.vsb`.
pub fn area_voice_bank(disc: &Disc, area: &str) -> Option<u16> {
    area_arif(disc, area).map(|r| r.0)
}

/// (voice bank, field music, battle music) of an area's `arif` record.
fn area_arif(disc: &Disc, area: &str) -> Option<(u16, u16, u16)> {
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
    Some((u16_at(rec + 0x30)?, u16_at(rec + 0x10)?, u16_at(rec + 0x12)?))
}

/// Plays the area's field music whenever a new area finishes loading.
pub fn area_bgm(
    disc: Res<Disc>,
    audio: Res<Audio>,
    area: Res<crate::assets::CurrentArea>,
    mut music: ResMut<crate::audio_bgm::AreaMusic>,
    mut last: Local<Option<String>>,
) {
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
            music.set_area(field, battle);
            if !play_bgm(&disc, &audio, field as u32) {
                warn!("music {field}: cannot load");
            }
        }
        None => warn!("{loaded}: no area music"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kh2_audio::footstep::footstep_number;
    use kh2_map::{Coct, flags};

    fn disc() -> Option<Disc> {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists());
        if iso.is_none() {
            eprintln!("skipped: no disc (set KH2_ISO)");
        }
        iso.map(|p| Disc::open(&p))
    }

    /// Materials of the polygons Sora can stand on in an area.
    fn floor_materials(disc: &Disc, area: &str) -> std::collections::BTreeSet<u8> {
        let map = disc.read(&format!("map/us/{area}.map")).expect("map");
        let coct = Coct::from_map(&map).expect("coct");
        coct.polygons.iter().filter(|p| p.flags & flags::PARTY_STAND != 0).map(|p| p.material).collect()
    }

    /// The USA executable selects the "us" voice folder (language table entry 2, passed by
    /// FUN_00101d68), names Sora's bank `<world><n>_sora` and every tt area's bank file exists.
    #[test]
    fn voice_paths_resolve_on_the_disc() {
        let Some(d) = disc() else { return };
        let exe = d.read_root("SLUS_210.05").expect("exe");
        assert_eq!(kh2_audio::voice::language(&exe).as_deref(), Some("us"));
        let names = kh2_audio::voice::SpeakerNames::from_exe(&exe).expect("names");
        assert_eq!(names.name(SORA_SPEAKER), Some("sora"));
        assert_eq!(names.name(2), Some("donald"));
        let mut found = 0;
        for area in ["tt00", "tt02", "tt04", "tt10"] {
            let Some(n) = area_voice_bank(&d, area) else { continue };
            let path = kh2_audio::voice::bank_path("us", "tt", n, "sora");
            if d.read(&path).is_some() {
                found += 1;
            }
            eprintln!("{area}: bank {n} -> {path} {}", d.read(&path).is_some());
        }
        assert!(found >= 1, "no Sora voice bank found for the tt areas");
    }

    /// Every voice id of Sora in the table resolves under the game's rules for a range of
    /// generator states: lines stay inside the bank, and -3 entries answer by the other party.
    #[test]
    fn sora_voice_records_pick_lines() {
        let Some(d) = disc() else { return };
        let mut table = VoiceTable::default();
        {
            let mut a = d.0.lock().unwrap();
            table.0 = kh2_tables::load::<kh2_tables::tables::Vtbl>(&mut a, &kh2_tables::tables::VTBL).expect("vtbl").rows;
        }
        let sora: Vec<_> = table.0.iter().filter(|r| r.character_id as u16 == SORA_SPEAKER).collect();
        assert!(!sora.is_empty());
        let with_other = sora.iter().filter(|r| r.voices.iter().step_by(2).any(|&l| l as i8 == -3)).count();
        eprintln!("Sora records {}, with an other-party line {with_other}", sora.len());
        let mut rng = 1u32;
        for r in &sora {
            for other in [0u16, 2, 3, 4] {
                if let Some((line, _)) = table.pick(SORA_SPEAKER, r.id as u16, other, &mut rng, &|_| false) {
                    assert!(line < 200, "voice id {} line {line}", r.id);
                }
            }
        }
    }

    #[test]
    fn placement_uses_the_camera_axes() {
        // Camera at the origin looking down -Z (Bevy's default), a source 6 m to its right and a
        // metre ahead: far enough for a pan, on the right half of the circle.
        let cam = Transform::IDENTITY;
        let (v, p) = placement(Some(Vec3::new(6.0, 0.0, -1.0)), Some(&cam));
        assert!(v < 0x3fff - 3000 && p > 0x40 && p <= 0x80, "{v} {p}");
        let (_, left) = placement(Some(Vec3::new(-6.0, 0.0, -1.0)), Some(&cam));
        assert!(left < 0x40);
        // No position or no listener: full volume, centre.
        assert_eq!(placement(None, Some(&cam)), (0x3fff, 0x40));
        assert_eq!(placement(Some(Vec3::X), None), (0x3fff, 0x40));
    }

    /// The source stays where the request put it; the camera moves past it (the game's listener
    /// is the camera, FUN_002c27b0). The mix follows: loudest at the closest point, pan from the
    /// left half to the right half of the circle.
    #[test]
    fn a_live_sound_follows_the_moving_listener() {
        let snd = LiveSound { handle: Handle(1), kind: LiveKind::Se(0), pos: Vec3::new(0.0, 0.0, -6.0), last: (0, 0) };
        // The camera looks down -Z (Bevy's default) and walks along +X past the source.
        let at = |x: f32| live_mix(&snd, &Transform::from_translation(Vec3::new(x, 0.0, 0.0)));
        let (v_left, p_left) = at(-6.0);
        let (v_mid, _) = at(0.0);
        let (v_right, p_right) = at(6.0);
        assert!(p_left > 0x40 && p_left <= 0x80, "source is to the right while the camera is left of it: {p_left}");
        assert!(p_right < 0x40, "source is to the left once the camera passed: {p_right}");
        assert!(v_mid > v_left && v_mid > v_right, "{v_left} {v_mid} {v_right}");
        assert_eq!(v_left, v_right);
        // A fixed-volume entry ignores the distance, a voice never drops below its minimum.
        let far = Transform::from_translation(Vec3::new(0.0, 0.0, 40.0));
        assert_eq!(live_mix(&LiveSound { kind: LiveKind::Se(0x10), ..snd }, &far).0, 0x3fff);
        assert_eq!(live_mix(&LiveSound { kind: LiveKind::Voice(0x1800), ..snd }, &far).0, 0x1800);
    }

    /// Renders a voice line from a fixed source that the listener passes from left to right, with
    /// the mix recomputed every 1/60 s tick exactly as `update_mixes` does, to
    /// `local/wp5d-audio/pan_sweep.wav` (untracked) for listening. Skipped without a disc.
    #[test]
    fn render_pan_sweep() {
        let Some(disc) = disc() else { return };
        let irx = disc.read_root("LIBSSL.IRX").expect("irx");
        let tables = Tables::from_irx(&irx).expect("tables");
        let mut d = Driver::new(Arc::new(tables));
        d.mixer.master = 0.8;
        let bank = disc.read("voice/us/battle/tt0_sora.vsb").and_then(|b| Vsb::parse(&b)).expect("bank");
        let (sample, rate) = (0..40).filter_map(|i| bank.line(i)).max_by_key(|(s, r)| s.pcm.len() as u64 * 1000 / *r as u64).expect("a line");
        let source = Vec3::new(0.0, 0.0, -6.0);
        let start = Transform::from_translation(Vec3::new(-9.0, 0.0, 0.0));
        let snd = LiveSound { handle: Handle(0), kind: LiveKind::Voice(0), pos: source, last: (0, 0) };
        let secs = sample.pcm.len() as f32 / rate as f32;
        let first = live_mix(&snd, &start);
        let h = d.play_voice_at(Arc::new(sample), rate, first.0, first.1);
        let snd = LiveSound { handle: h, ..snd };
        let ticks = (secs * 60.0).ceil() as usize + 30;
        let mut pcm = Vec::new();
        let mut pans = Vec::new();
        for t in 0..ticks {
            let x = -9.0 + 18.0 * t as f32 / ticks as f32;
            let (v, p) = live_mix(&snd, &Transform::from_translation(Vec3::new(x, 0.0, 0.0)));
            d.set_voice_mix(h, v, p);
            pans.push(p);
            let mut buf = vec![0f32; (kh2_audio::RATE / 60) as usize * 2];
            d.render(&mut buf);
            pcm.extend(buf);
        }
        assert!(pans[0] > 0x40 && *pans.last().unwrap() < 0x40, "pan sweeps from the right of the circle's front to the left: {pans:?}");
        let dir = std::path::Path::new("../../local/wp5d-audio");
        std::fs::create_dir_all(dir).unwrap();
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
        std::fs::write(dir.join("pan_sweep.wav"), b).unwrap();
        assert!(pcm.iter().any(|s| s.abs() > 0.01));
    }

    #[test]
    fn soundinfo_records_resolve() {
        let Some(d) = disc() else { return };
        // tt02 is area 2 of world tt: banks 697, 698, 699, none, 729; Sora's NeoStatus is 1.
        let r = area_soundinfo(&d, "tt02").expect("tt02 record");
        assert_eq!(r.banks, [697, 698, 699, -1, 729]);
        assert_eq!(neo_status(&d, SORA_MODEL), Some(1));
        assert_eq!(r.bank(1), Some(697));
        // tt00 uses the 694 triple, so the same step sounds different there.
        let r0 = area_soundinfo(&d, "tt00").expect("tt00 record");
        assert_eq!(r0.bank(1), Some(694));
        let (mut a, mut b) = (0, 0);
        assert_eq!(footstep_number(Some(&r), 1, 1, 4, &mut a), Some(697_020));
        assert_eq!(footstep_number(Some(&r0), 1, 1, 4, &mut b), Some(694_020));
    }

    #[test]
    fn floors_pick_different_sounds_and_all_have_samples() {
        let Some(d) = disc() else { return };
        for area in ["tt02", "tt04"] {
            let record = area_soundinfo(&d, area).expect("record");
            let bank = record.bank(1).expect("Sora's bank");
            let block = kh2_audio::ssl::SeBlock::parse(&d.read(&format!("se/se{bank:03}.seb")).expect("bank")).expect("se block");
            let materials = floor_materials(&d, area);
            let audible = materials.iter().filter(|&&m| m != 0).count();
            assert!(audible >= 2, "{area}: floors should differ: {materials:?}");
            for m in materials {
                // Every floor material is silent (0) or has a walk sample in the area's bank.
                let mut c = 0;
                let n = footstep_number(Some(&record), 1, m, 4, &mut c);
                assert_eq!(n.is_some(), m != 0);
                if let Some(n) = n {
                    let e = block.entries.get((n % 1000) as usize).unwrap_or_else(|| panic!("{area}: material {m}: entry {} missing", n % 1000));
                    assert!(!e.tracks.is_empty(), "{area}: material {m}: walk entry {} is empty", n % 1000);
                }
            }
        }
    }
}
