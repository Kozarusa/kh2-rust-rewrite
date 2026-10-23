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
use crate::shared::sora::Sora;

pub use crate::shared::sound::{placement, GameRng, PlaySe, OWN_BANK};

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

/// Harness: `KH2_AUDIO_DUMP=<file>` appends every rendered chunk to the file as raw little-endian f32
/// stereo at 48 kHz (what the stream hands to the output device, before the device), so a live run can
/// be measured without loopback recording.
fn dump_chunk(buf: &[f32]) {
    use std::io::Write;
    static FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);
    let Some(path) = std::env::var_os("KH2_AUDIO_DUMP") else { return };
    let Ok(mut g) = FILE.lock() else { return };
    if g.is_none() {
        *g = std::fs::File::create(&path).ok();
    }
    let Some(f) = g.as_mut() else { return };
    let bytes: Vec<u8> = buf.iter().flat_map(|x| x.to_le_bytes()).collect();
    let _ = f.write_all(&bytes);
}

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
            dump_chunk(&self.buf);
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

/// A voice line: line `line` of the voice bank `file` (e.g. `voice/us/battle/tt0_sora.vsb`).
#[derive(Message, Clone, Debug)]
pub struct PlayVoice {
    /// The bank's file: a `voice/<lang>/battle/*.vsb` file, or (`in_bar`) an object's `.apdx`.
    pub file: String,
    /// The bank is the entry of type [`VOICE_BAR_ENTRY`] of the BAR `file`: an enemy's own voice
    /// bank, which its object file carries (`FUN_00196b58` looks it up there for the voice slot).
    pub in_bar: bool,
    pub line: u16,
    /// The record's priority byte (`vtbl` +2): a line with a lower priority than the one queued
    /// or playing on the speaker's slot is dropped (`FUN_001ae798`).
    pub priority: u8,
    /// World position in metres of the speaker (voice lines are positional sounds of kind 5).
    pub pos: Option<Vec3>,
    /// The speaker's priority slot (actor +0x9b8, `FUN_001ae798`): [`SORA_SLOT`] for Sora, the
    /// entity's bits for an enemy.
    pub speaker: u64,
    /// The speaker's body: the slot keeps the playing sound at the actor's position every tick
    /// (`FUN_001ae7d8` -> `FUN_002c34c8(handle, FUN_0016b960(actor))`).
    pub follow: Option<Entity>,
}

/// Entry type of a BAR (an object's `.apdx`) that holds the object's own voice bank: the base
/// object constructors `FUN_00196b58` and `FUN_00196d18` look it up with `FUN_001684c0(file, 0x22)`
/// and make the actor's voice slot (+0x9b8) only when it exists.
pub const VOICE_BAR_ENTRY: u16 = 0x22;

/// The voice slot key of Sora (the party use their own banks and slots; enemies use entity bits).
pub const SORA_SLOT: u64 = 0;

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
            .init_resource::<crate::audio_bgm::BattleFlag>()
            .add_message::<PlaySe>()
            .add_message::<PlayVoice>()
            .add_message::<SoraVoice>()
            .add_systems(Startup, (start_stream, load_voice_table, load_voice_info))
            .add_systems(Update, play_requests)
            .add_systems(Update, area_bgm.after(crate::assets::load_area))
            .add_systems(
                FixedUpdate,
                (
                    motion_sounds,
                    sora_voices,
                    enemy_voice_calls,
                    footsteps,
                    enemy_footsteps,
                    wall_sounds,
                    crate::audio_bgm::battle_music,
                    update_mixes,
                )
                    .chain()
                    .in_set(crate::schedule::Sim::Sounds),
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
    /// A voice line moves with its speaker: the voice slot rewrites the sound's position from the
    /// actor every tick (`FUN_001ae7d8` -> `FUN_002c34c8`). Sound effects never do.
    pub follow: Option<Entity>,
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
fn update_mixes(
    audio: Res<Audio>,
    mut live: ResMut<LiveMixes>,
    listener: Query<&Transform, With<crate::camera::OrbitCamera>>,
    places: Query<&GlobalTransform>,
) {
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
        if let Some(g) = snd.follow.and_then(|e| places.get(e).ok()) {
            snd.pos = g.translation();
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

/// The voice bank of a request: a `.vsb` file, or the entry of type [`VOICE_BAR_ENTRY`] of the
/// BAR file the request names (cached by file).
pub fn voice_bank(disc: &Disc, cache: &SeCache, v: &PlayVoice) -> Option<Arc<Vsb>> {
    voice_bank_file(disc, cache, &v.file, v.in_bar)
}

/// The voice bank of `file` (a `.vsb`, or with `in_bar` the [`VOICE_BAR_ENTRY`] entry of the BAR),
/// through the same cache the voice requests use. The sound viewer (`sound_test.rs`) reads banks
/// through it.
pub fn voice_bank_file(disc: &Disc, cache: &SeCache, file: &str, in_bar: bool) -> Option<Arc<Vsb>> {
    let key = if in_bar { format!("{file}#{VOICE_BAR_ENTRY:#x}") } else { file.to_string() };
    cache
        .voices
        .lock()
        .ok()?
        .entry(key)
        .or_insert_with(|| {
            let raw = disc.read(file)?;
            if in_bar {
                let bar = kh2_bar::parse(&raw).ok()?;
                Vsb::parse(bar.iter().find(|e| e.kind == VOICE_BAR_ENTRY)?.data).map(Arc::new)
            } else {
                Vsb::parse(&raw).map(Arc::new)
            }
        })
        .clone()
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
    let bank = voice_bank(disc, cache, v)?;
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
    mut slots: Local<HashMap<u64, kh2_audio::voice::VoiceSlot>>,
    mut live: ResMut<LiveMixes>,
) {
    if std::env::var_os("KH2_NO_SOUND").is_some() {
        req.clear();
        voices.clear();
        return;
    }
    for v in voices.read() {
        let vp = placement(v.pos, listener.single().ok());
        let h = play_voices(&disc, &audio, &cache, v, vp, slots.entry(v.speaker).or_default());
        if let (Some(h), Some(pos)) = (h, v.pos) {
            live.0.push(LiveSound { handle: h, kind: LiveKind::Voice(kh2_audio::voice::MIN_VOLUME), pos, last: vp, follow: v.follow });
        }
        if std::env::var_os("KH2_SOUNDLOG").is_some() {
            info!("voice {}{}:{} {}", v.file, if v.in_bar { "#0x22" } else { "" }, v.line, if h.is_some() { "playing" } else { "not found or dropped by priority" });
        }
    }
    let log = std::env::var_os("KH2_SOUNDLOG").is_some();
    for r in req.read() {
        let vp = placement(r.pos, listener.single().ok());
        let started = play_se(&disc, &audio, &cache, r, vp);
        let ok = started.is_some();
        if let (Some((handle, flags)), Some(pos)) = (started, r.pos) {
            live.0.push(LiveSound { handle, kind: LiveKind::Se(flags), pos, last: vp, follow: None });
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
        let mut se = PlaySe::object_sound(crate::combat::WALL_HIT_SOUND, anims.get(w.body).ok().and_then(|a| a.rig.apdx.clone()));
        se.pos = places.get(w.body).ok().map(|g| g.translation());
        out.write(se);
    }
}

/// The request of a motion trigger 8: the sound goes through the object's method (FUN_00197008), so a
/// bank number of 500 names the object's own bank ([`PlaySe::object_sound`]), placed at the object.
pub fn motion_sound_request(bank: u16, index: u16, apdx: Option<String>, at: Vec3) -> PlaySe {
    PlaySe::object_sound(bank as u32 * 1000 + index as u32, apdx).at(at)
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
    enemies: Query<&crate::shared::enemy::Enemy>,
    speakers: Query<(Entity, &crate::shared::enemy::Enemy)>,
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
                    if std::env::var_os("KH2_SOUNDLOG").is_some() {
                        info!("motion sound {bank}:{index} of {} motion {} frame {:.1}", a.rig.apdx.as_deref().unwrap_or("-"), a.tag(), a.frame);
                    }
                    out.write(motion_sound_request(*bank as u16, *index as u16, a.rig.apdx.clone(), place.translation()));
                }
                // Voice lines: the speaker's bank for the current world.
                (frame::VOICE | frame::VOICE2, [voice, ..]) if is_sora => {
                    let ctx = VoiceCtx { area: &area, table: &table, info: &info, disc: &disc };
                    let other = sora_other(&sora, &enemies, &disc, &mut neo_cache);
                    if let Some(v) = sora_voice(&ctx, *voice as u16, other, &mut rng.0, (place.translation(), e), a.tag()) {
                        voices.write(v);
                    }
                }
                // An enemy's voice trigger (the dispatcher calls the object's method +0x54, FUN_001970b8,
                // with the first parameter as the line of the enemy's own bank and the second, when the
                // trigger has two, as the priority; 0 otherwise).
                (frame::VOICE | frame::VOICE2, [line, rest @ ..]) if !is_sora => {
                    let Some((owner, enemy)) = speakers.iter().find(|(_, en)| en.body == e) else { continue };
                    let priority = rest.first().copied().unwrap_or(0) as i32;
                    if let Some(v) = enemy_voice(enemy.name, a.rig.apdx.as_deref(), *line as i32, priority, place.translation(), (owner, e)) {
                        voices.write(v);
                    }
                }
                _ => {}
            }
        }
    }
}

/// What picking a voice line of Sora reads: the loaded area (its voice bank), the voice table, the
/// speaker names and language, the disc.
pub struct VoiceCtx<'a> {
    pub area: &'a CurrentArea,
    pub table: &'a VoiceTable,
    pub info: &'a VoiceInfo,
    pub disc: &'a Disc,
}

/// The other party of a voice pick: the NeoStatus of the actor's target object (FUN_00175d18 takes
/// +0x9f4, the command's target record; the app's is the lock-on or auto target [inferred]).
fn sora_other(sora: &Query<&Sora>, enemies: &Query<&crate::shared::enemy::Enemy>, disc: &Disc, cache: &mut HashMap<&'static str, u16>) -> u16 {
    sora.single()
        .ok()
        .and_then(|s| s.target)
        .and_then(|t| enemies.get(t).ok())
        .map(|e| {
            let model = e.kind.model;
            *cache.entry(e.name).or_insert_with(|| neo_status(disc, model).unwrap_or(0))
        })
        .unwrap_or(0)
}

/// FUN_00175d18 (the player's method +0x54) for Sora: the voice table picks a line of `voice` against
/// the other party's NeoStatus, from the speaker's bank of the current world, played at `at`.
/// `None` when no area is loaded or the record answers no line.
pub fn sora_voice(ctx: &VoiceCtx, voice: u16, other: u16, rng: &mut u32, at: (Vec3, Entity), why: &str) -> Option<PlayVoice> {
    let area_name = ctx.area.loaded.as_deref()?;
    let world = area_name.get(..2)?;
    let picked = ctx.table.pick(SORA_SPEAKER, voice, other, rng, &|_| false);
    let bank = area_voice_bank(ctx.disc, area_name).unwrap_or(0);
    let name = ctx.info.names.name(SORA_SPEAKER).unwrap_or("sora");
    if std::env::var_os("KH2_SOUNDLOG").is_some() {
        info!("voice id {voice} ({why}) other {other} -> line {picked:?} rng {:#x}", *rng);
    }
    let (line, priority) = picked?;
    Some(PlayVoice {
        file: kh2_audio::voice::bank_path(&ctx.info.lang, world, bank, name),
        in_bar: false,
        line,
        priority,
        pos: Some(at.0),
        speaker: SORA_SLOT,
        follow: Some(at.1),
    })
}

/// An enemy's voice line (`FUN_001970b8` -> `FUN_001ae798`, the object's method +0x54): the line is
/// the line number of the enemy's own voice bank, the entry of type [`VOICE_BAR_ENTRY`] of its
/// `.apdx` (the constructors `FUN_00196b58` and `FUN_00196d18` take it from there for the voice slot
/// at +0x9b8; an object file without the entry has no slot and the call does nothing), played
/// at the speaker with the slot's minimum volume (`MIN_VOLUME`) and priority `priority`. `at` is the
/// speaker's position and its owner and body entities. `None` without a bank file or with a negative line.
pub fn enemy_voice(name: &str, apdx: Option<&str>, line: i32, priority: i32, at: Vec3, speaker: (Entity, Entity)) -> Option<PlayVoice> {
    let file = apdx?.to_string();
    let line = u16::try_from(line).ok()?;
    if std::env::var_os("KH2_SOUNDLOG").is_some() {
        info!("voice enemy {name} line {line} priority {priority} -> bank {file} entry {VOICE_BAR_ENTRY:#x} id {line}");
    }
    Some(PlayVoice {
        file,
        in_bar: true,
        line,
        priority: priority.clamp(0, 255) as u8,
        pos: Some(at),
        speaker: speaker.0.to_bits(),
        follow: Some(speaker.1),
    })
}

/// A voice request of an enemy script (host call 1.262, `enemy_host.rs`): who asked (the script's tag,
/// `<kind name>#<serial>`), the line, the priority and where it stands (metres, world space). The
/// host runs inside the enemy AI system, which does not own the sound messages, so the request waits
/// here until [`enemy_voice_calls`] picks it up.
#[derive(Clone, Debug)]
pub struct EnemyVoiceCall {
    pub tag: String,
    pub line: i32,
    pub priority: i32,
    pub at: Vec3,
}

static ENEMY_VOICE_CALLS: std::sync::Mutex<Vec<EnemyVoiceCall>> = std::sync::Mutex::new(Vec::new());

/// Queues an enemy script's voice request (host 1.262).
pub fn queue_enemy_voice(call: EnemyVoiceCall) {
    if let Ok(mut q) = ENEMY_VOICE_CALLS.lock() {
        q.push(call);
    }
}

/// Plays the queued voice requests of enemy scripts. The speaker is the nearest living enemy of the
/// requesting script's kind (the host has no entity; the tag names the kind, and two enemies of one
/// kind are told apart by where they stand).
pub fn enemy_voice_calls(enemies: Query<(Entity, &crate::shared::enemy::Enemy, &GlobalTransform)>, anims: Query<&Animator>, mut voices: MessageWriter<PlayVoice>) {
    let calls: Vec<EnemyVoiceCall> = match ENEMY_VOICE_CALLS.lock() {
        Ok(mut q) => std::mem::take(&mut *q),
        Err(_) => return,
    };
    for c in calls {
        let kind = c.tag.split('#').next().unwrap_or_default();
        let nearest = enemies
            .iter()
            .filter(|(_, e, _)| e.name == kind)
            .min_by(|a, b| a.2.translation().distance_squared(c.at).total_cmp(&b.2.translation().distance_squared(c.at)));
        let Some((owner, e, place)) = nearest else { continue };
        let apdx = anims.get(e.body).ok().and_then(|a| a.rig.apdx.clone());
        if let Some(v) = enemy_voice(e.name, apdx.as_deref(), c.line, c.priority, place.translation(), (owner, e.body)) {
            voices.write(v);
        }
    }
}

/// A voice of Sora asked for outside the motion triggers: the cast start (FUN_001a4798) calls the
/// player's method +0x54 (FUN_00175d18) with the `magc` row's `Voice`, `VoiceFinisher` or
/// `VoiceSelf`. `why` names the caller in the sound log.
#[derive(Message, Clone, Debug)]
pub struct SoraVoice {
    pub voice: u16,
    pub why: String,
}

/// Plays the `SoraVoice` requests through the same pick as the motion's voice triggers, at Sora's body.
#[allow(clippy::too_many_arguments)]
pub fn sora_voices(
    mut req: MessageReader<SoraVoice>,
    sora: Query<&Sora>,
    places: Query<&GlobalTransform>,
    area: Res<CurrentArea>,
    table: Res<VoiceTable>,
    mut rng: ResMut<GameRng>,
    info: Res<VoiceInfo>,
    disc: Res<Disc>,
    enemies: Query<&crate::shared::enemy::Enemy>,
    mut neo_cache: Local<HashMap<&'static str, u16>>,
    mut voices: MessageWriter<PlayVoice>,
) {
    for r in req.read() {
        let Ok(s) = sora.single() else { continue };
        let at = places.get(s.body).map_or(Vec3::ZERO, |g| g.translation());
        let ctx = VoiceCtx { area: &area, table: &table, info: &info, disc: &disc };
        let other = sora_other(&sora, &enemies, &disc, &mut neo_cache);
        if let Some(v) = sora_voice(&ctx, r.voice, other, &mut rng.0, (at, s.body), &r.why) {
            voices.write(v);
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
pub fn neo_status(disc: &Disc, model: &str) -> Option<u16> {
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
    pad: Option<Res<crate::shared::sora::Pad>>,
    area: Res<CurrentArea>,
    disc: Res<Disc>,
    cache: Res<SeCache>,
    mut info: Local<Option<(String, Option<kh2_audio::footstep::SoundInfoRecord>)>>,
    mut neo: Local<Option<Option<u16>>>,
    mut counter: Local<u8>,
    mut last: Local<Option<(u32, bool)>>,
    mut out: MessageWriter<PlaySe>,
    mut dust: MessageWriter<crate::fx::FootDust>,
) {
    let Ok(s) = sora.single() else { return };
    let Ok(a) = anims.get(s.body) else { return };
    let neo_status = neo.get_or_insert_with(|| neo_status(&disc, SORA_MODEL)).unwrap_or(1);
    // The ground mover (FUN_0017af10) runs for a grounded Sora in his ground state; he is a party actor.
    let commanded = pad.as_ref().map_or(0.0, |p| p.stick);
    let mover = s.state == crate::shared::sora::State::Ground && s.motor.grounded;
    let stop = mover && last.is_some_and(|(id, r42)| kh2_audio::footstep::stop_step(true, commanded, r42, id));
    // The same call starts the dust of the first step out of idle (its commanded branch).
    let start = mover && last.is_some_and(|(id, _)| kh2_audio::footstep::start_dust(true, commanded, id, neo_status));
    *last = Some((a.slot.map_or(u32::MAX, |sl| (sl / 4) as u32), a.trigger(kh2_anim::triggers::range::KEEP_MOMENTUM)));
    let mut kinds: Vec<(u16, String)> = Vec::new();
    // Dust requests (FUN_001b1c90) in the order the game makes them: the stop step's sound first,
    // then its dust 8 and 9 (not for NeoStatus 0xe), the start dust 10 and 11, and the trigger 19 dust.
    let mut dusts: Vec<(u16, String)> = Vec::new();
    if stop {
        kinds.push((kh2_audio::footstep::kind::STOP, "stop (FUN_0017af10)".to_string()));
        if neo_status != kh2_audio::footstep::dust_kind::NO_DUST_NEO_STATUS {
            dusts.extend(kh2_audio::footstep::dust_kind::STOP.map(|k| (k, "stop (FUN_0017af10)".to_string())));
        }
    }
    if start {
        dusts.extend(kh2_audio::footstep::dust_kind::START.map(|k| (k, "start (FUN_0017af10)".to_string())));
    }
    if let (Some(t), Some(m)) = (a.triggers(), a.motion()) {
        let g = |f: f32| f * crate::data::LOGIC_HZ / m.fps.max(1.0);
        let (from, to) = (g(a.prev_frame), g(a.frame));
        if from != to {
            for ft in t.crossed(from, to) {
                let kind = ft.params.first().copied().unwrap_or(0).max(0) as u16;
                match ft.id {
                    kh2_anim::triggers::frame::FOOTSTEP_SOUND => kinds.push((kind, format!("frame {}", ft.frame))),
                    kh2_anim::triggers::frame::FOOTSTEP_EFFECT => dusts.push((kind, format!("trigger 19 frame {}", ft.frame))),
                    _ => {}
                }
            }
        }
    }
    if kinds.is_empty() && dusts.is_empty() {
        return;
    }
    let Some(name) = area.loaded.clone() else { return };
    if info.as_ref().is_none_or(|(n, _)| *n != name) {
        *info = Some((name.clone(), area_soundinfo(&disc, &name)));
    }
    let record = info.as_ref().and_then(|(_, r)| r.as_ref());
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
    let (at, facing) = (s.motor.pos * crate::SCALE, Quat::from_rotation_y(s.motor.yaw) * Vec3::Z);
    for (kind, why) in dusts {
        if log {
            info!("dust {} {why} kind {kind} material {material} neo {neo_status}", a.tag());
        }
        dust.write(crate::fx::FootDust { kind, at, facing, material, why: "Sora" });
    }
}

/// Footsteps of enemies: motion frame trigger 2 resolves the sound the same way as for Sora
/// (`footstep_number` with the enemy's own step counter, its NeoStatus from its `objentry` row, and
/// `FUN_00183730`'s ground material under it) and plays it at the enemy; trigger 19 asks for the
/// footstep dust (`FUN_001b1c90`). The enemy ground mover has no stop step (`FUN_0017af10` needs a
/// party actor, +0x588 bit 0x80), so the triggers are all there is.
///
/// None of the motion sets of the app's enemy kinds carries a trigger 2 or 19 today (checked with
/// `kh2 triggers obj/<model>.mset 2`, see `docs/systems/audio/footsteps.md`); the system is the hook for kinds that
/// do, and logs a `footstep enemy` line when it plays one.
#[allow(clippy::too_many_arguments)]
pub fn enemy_footsteps(
    enemies: Query<(Entity, &crate::shared::enemy::Enemy, &GlobalTransform)>,
    anims: Query<&Animator>,
    world: Option<Res<crate::world::World>>,
    area: Res<CurrentArea>,
    disc: Res<Disc>,
    cache: Res<SeCache>,
    mut info: Local<Option<(String, Option<kh2_audio::footstep::SoundInfoRecord>)>>,
    mut neo: Local<HashMap<&'static str, u16>>,
    mut counters: Local<HashMap<Entity, u8>>,
    mut out: MessageWriter<PlaySe>,
    mut dust: MessageWriter<crate::fx::FootDust>,
) {
    let log = std::env::var_os("KH2_SOUNDLOG").is_some();
    for (owner, e, place) in &enemies {
        let Ok(a) = anims.get(e.body) else { continue };
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
        let crossed: Vec<(u8, u16, i16)> = t
            .crossed(from, to)
            .filter(|f| f.id == frame::FOOTSTEP_SOUND || f.id == frame::FOOTSTEP_EFFECT)
            .map(|f| (f.id, f.params.first().copied().unwrap_or(0).max(0) as u16, f.frame))
            .collect();
        if crossed.is_empty() {
            continue;
        }
        let Some(name) = area.loaded.clone() else { return };
        if info.as_ref().is_none_or(|(n, _)| *n != name) {
            *info = Some((name.clone(), area_soundinfo(&disc, &name)));
        }
        let record = info.as_ref().and_then(|(_, r)| r.as_ref());
        let neo_status = *neo.entry(e.name).or_insert_with(|| neo_status(&disc, e.kind.model).unwrap_or(0));
        let at = place.translation();
        // FUN_00183730 for an enemy: the material of the floor polygon under it (0 in the air). The
        // enemy's own ground contact is kept by its script object, which this system cannot see; the
        // floor under its position is looked up again [inferred: the same polygon on ordinary terrain].
        let cm = at / crate::SCALE;
        let ground = world.as_ref().and_then(|w| w.ground(cm, cm.y + 20.0, 40.0));
        let material = kh2_audio::footstep::actor_material(0, ground.map_or(0, |g| g.material));
        let facing = place.rotation() * Vec3::Z;
        let counter = counters.entry(owner).or_insert(0);
        for (id, kind, frame_no) in crossed {
            if id == frame::FOOTSTEP_EFFECT {
                if log {
                    info!("dust enemy {} trigger 19 frame {frame_no} kind {kind} material {material} neo {neo_status}", e.name);
                }
                dust.write(crate::fx::FootDust { kind, at, facing, material, why: e.name });
                continue;
            }
            let number = kh2_audio::footstep::footstep_number(record, neo_status, material, kind, counter);
            if log {
                let sample = number.and_then(|n| {
                    let bank = load_bank(&disc, &cache, (n / 1000) as u16, None)?;
                    let en = bank.block.entries.get((n % 1000) as usize)?;
                    Some(format!("wd {} tracks {}", en.wd, en.tracks.len()))
                });
                info!(
                    "footstep enemy {} {} frame {frame_no} kind {kind} material {material} neo {neo_status} -> {number:?} {}",
                    e.name,
                    a.tag(),
                    sample.as_deref().unwrap_or("no sample")
                );
            }
            if let Some(n) = number {
                out.write(PlaySe::number(n, None).at(at));
            }
        }
    }
}

/// World codes in the game's world order (OpenKH `docs/kh2/worlds.md`).
pub const WORLDS: [&str; 19] = ["zz", "es", "tt", "di", "hb", "bb", "he", "al", "mu", "po", "lk", "lm", "dc", "wi", "nm", "wm", "ca", "tr", "eh"];

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

/// The area's music when a new area finishes loading (`FUN_00137150`): the pair of the area
/// script's opcode 0x10 once `area::run_area_script` has run (`AreaRecord.run`), else the pair
/// already playing, else the `arif` pair as a start-up stand-in (`audio_bgm::area_pair`). The
/// field music restarts only when its number changes (`FUN_001371d8` / `FUN_00137300`); number 0
/// silences it.
pub fn area_bgm(
    disc: Res<Disc>,
    audio: Res<Audio>,
    area: Res<crate::assets::CurrentArea>,
    rec: Option<Res<crate::area::AreaRecord>>,
    mut music: ResMut<crate::audio_bgm::AreaMusic>,
    mut last: Local<Option<String>>,
) {
    let Some(loaded) = area.loaded.clone() else { return };
    if last.as_deref() == Some(loaded.as_str()) {
        return;
    }
    // Wait for this area's script run (it may land a frame after the load).
    let script = match rec.as_deref() {
        Some(r) => match r.run.as_deref() {
            Some(run) if run.name == loaded => run.script.music,
            _ => return,
        },
        None => None,
    };
    *last = Some(loaded.clone());
    if std::env::var_os("KH2_NO_MUSIC").is_some() {
        return;
    }
    let previous = music.field.zip(music.battle);
    let Some(((field, battle), source)) = crate::audio_bgm::area_pair(script, previous, || area_music(&disc, &loaded)) else {
        warn!("{loaded}: no area music");
        return;
    };
    let why = match source {
        crate::audio_bgm::PairSource::Script => "area script opcode 0x10",
        crate::audio_bgm::PairSource::Kept => "no opcode 0x10: previous pair kept",
        crate::audio_bgm::PairSource::Arif => "no opcode 0x10 since start-up: arif stand-in",
    };
    if !music.load_pair(field, battle) {
        info!("{loaded}: music field {field} battle {battle} ({why}); field music unchanged, not restarted");
        return;
    }
    info!("{loaded}: music field {field} battle {battle} ({why})");
    if field == 0 {
        if let Ok(mut s) = audio.0.lock()
            && let Some(d) = s.driver.as_mut()
        {
            d.bgm_stop(0, false);
            d.bgm_stop(1, false);
        }
    } else if !play_bgm(&disc, &audio, field as u32) {
        warn!("music {field}: cannot load");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::sound::OBJECT_SOUND_BANK;
    use kh2_audio::footstep::footstep_number;
    use kh2_map::{Coct, flags};

    /// The interface sounds and the HUD alarm live in the boot bank `se000.seb` (driver slot 2,
    /// `FUN_002c0eb0` / `FUN_002c0da8`), not in `se002.seb` (an area bank with two entries).
    #[test]
    fn system_sounds_resolve_in_the_boot_bank() {
        assert_eq!(kh2_audio::ssl::boot_slot(0), Some(2));
        assert_eq!(kh2_audio::ssl::boot_slot(999), Some(3));
        assert_eq!(kh2_audio::ssl::boot_slot(2), None);
        let req = PlaySe::system(9);
        assert_eq!((req.bank, req.index), (0, 9));
        let Some(d) = disc() else { return };
        let cache = SeCache::default();
        let bank = load_bank(&d, &cache, req.bank, None).expect("se000.seb");
        // cursor 1, confirm 2, open 3, cancel 4, refused 5, danger alarm 9
        for sound in [1usize, 2, 3, 4, 5, 9] {
            let e = bank.block.entries.get(sound).unwrap_or_else(|| panic!("se000 has no entry {sound}"));
            assert!(!e.tracks.is_empty(), "se000 entry {sound} is empty");
        }
        assert!(load_wd(&d, &cache, bank.block.entries[9].wd).is_some(), "the alarm's wave bank loads");
    }

    /// Metrics of a stereo render: peak, samples at the clamp, DC offset, rms and the share of
    /// energy in the first difference (a high-frequency measure: 4 is white noise, near 0 is bass).
    fn metrics(out: &[f32]) -> (f32, usize, f32, f32, f32) {
        let peak = out.iter().fold(0f32, |m, &x| m.max(x.abs()));
        let clipped = out.iter().filter(|x| x.abs() >= 0.9999).count();
        let dc = out.iter().sum::<f32>() / out.len().max(1) as f32;
        let e: f32 = out.iter().map(|x| x * x).sum();
        let d: f32 = out.chunks_exact(2).collect::<Vec<_>>().windows(2).map(|w| (w[1][0] - w[0][0]).powi(2)).sum::<f32>() * 2.0;
        (peak, clipped, dc, (e / out.len().max(1) as f32).sqrt(), if e > 0.0 { d / e } else { 0.0 })
    }

    /// Offline render of system sounds and the save point candidates (WP-AU4). Needs the disc.
    /// `KH2_AU4_OUT=<dir>` also writes each render as a 48 kHz stereo 16-bit WAV there.
    #[test]
    fn render_system_sounds_metrics() {
        let Some(d) = disc() else { return };
        let Some(irx) = d.read_root("LIBSSL.IRX") else { return };
        let tables = Arc::new(Tables::from_irx(&irx).expect("tables"));
        let cache = SeCache::default();
        let mut worst = 0f32;
        let n = load_bank(&d, &cache, 0, None).expect("se000").block.entries.len() as u16;
        for (bank, index) in (0..n).map(|i| (0u16, i)) {
            let req = PlaySe { bank, index, apdx: None, pos: None };
            let b = load_bank(&d, &cache, bank, None).expect("bank");
            if b.block.entries[index as usize].tracks.is_empty() {
                continue;
            }
            let entry = &b.block.entries[index as usize];
            let Some(wd) = load_wd(&d, &cache, entry.wd) else { continue };
            let mut dr = Driver::new(tables.clone());
            dr.mixer.master = 0.6;
            assert!(dr.play_se_handle(&b.block, req.index as usize, wd.clone(), 0x3fff, 0x40).is_some());
            let mut out = vec![0f32; 2 * 48_000 * 3];
            dr.render(&mut out);
            let (peak, clipped, dc, rms, hf) = metrics(&out);
            let last = out.chunks_exact(2).rposition(|f| f[0].abs() > 0.001 || f[1].abs() > 0.001).map_or(0.0, |i| i as f32 / 48_000.0);
            eprintln!(
                "AU4 se {bank}:{index} wd {} flags {:#x} tracks {} peak {peak:.3} clipped {clipped} dc {dc:.5} rms {rms:.4} hf {hf:.3} audible {last:.2}s",
                entry.wd, entry.flags, entry.tracks.len()
            );
            assert_eq!(clipped, 0, "se 0:{index} clips");
            if (1..=5).contains(&index) {
                assert!(peak < 0.95 && dc.abs() < 0.01, "menu sound {index}: peak {peak}, dc {dc}");
            }
            worst = worst.max(peak);
            if let Some(dir) = std::env::var_os("KH2_AU4_OUT") {
                let mut w = Vec::new();
                let data: Vec<u8> = out.iter().flat_map(|x| ((x * 32767.0) as i16).to_le_bytes()).collect();
                w.extend_from_slice(b"RIFF");
                w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
                w.extend_from_slice(b"WAVEfmt ");
                for v in [16u32, 0x0002_0001, 48_000, 192_000] {
                    w.extend_from_slice(&v.to_le_bytes());
                }
                w.extend_from_slice(&0x0010_0004u32.to_le_bytes());
                w.extend_from_slice(b"data");
                w.extend_from_slice(&(data.len() as u32).to_le_bytes());
                w.extend_from_slice(&data);
                std::fs::write(std::path::Path::new(&dir).join(format!("se_{bank}_{index}.wav")), w).unwrap();
            }
        }
        assert!(worst > 0.0, "at least one system sound produced output");
    }

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

    /// The cast voices (FUN_001a4798 -> FUN_00175d18): the `magc` rows of Fire, Blizzard, Thunder and
    /// Cure name voice ids that the voice table answers with a line of Sora's tt bank, through the
    /// same pick the motion triggers use (`sora_voice`); -1 asks for none.
    #[test]
    fn cast_voices_pick_lines_from_the_bank() {
        let Some(d) = disc() else { return };
        let mut table = VoiceTable::default();
        let magc = {
            let mut a = d.0.lock().unwrap();
            table.0 = kh2_tables::load::<kh2_tables::tables::Vtbl>(&mut a, &kh2_tables::tables::VTBL).expect("vtbl").rows;
            kh2_tables::load::<kh2_tables::tables::Magc>(&mut a, &kh2_tables::tables::MAGC).expect("magc").rows
        };
        let exe = d.read_root("SLUS_210.05").expect("exe");
        let info = VoiceInfo { names: kh2_audio::voice::SpeakerNames::from_exe(&exe).unwrap_or_default(), lang: kh2_audio::voice::language(&exe).unwrap_or_else(|| "us".into()) };
        let area = CurrentArea { name: "tt04".into(), loaded: Some("tt04".into()) };
        let ctx = VoiceCtx { area: &area, table: &table, info: &info, disc: &d };
        let mut rng = 1u32;
        let mut played = 0;
        for id in 0..4u8 {
            let row = crate::magic_rules::pick_row(&magc, id, 1, 0).expect("level-1 row");
            for (what, v) in [("Voice", row.voice), ("VoiceFinisher", row.voice_finisher), ("VoiceSelf", row.voice_self)] {
                if v < 0 {
                    continue;
                }
                // A record's pairs pass by chance: a few generator states find a line. A record whose
                // lines all depend on the other party (-3) answers only for one (Cure's Voice: cast
                // on another member; Donald's NeoStatus 2 here).
                let got = [0u16, kh2_audio::voice::speaker::DONALD]
                    .into_iter()
                    .find_map(|other| (0..32).find_map(|_| sora_voice(&ctx, v as u16, other, &mut rng, (Vec3::ZERO, Entity::PLACEHOLDER), "test")));
                eprintln!("{} {what} {v}: {:?}", row.file_name, got.as_ref().map(|p| (&p.file, p.line, p.priority)));
                let p = got.unwrap_or_else(|| panic!("{} {what} {v}: no line", row.file_name));
                assert!(d.read(&p.file).is_some(), "bank {} on the disc", p.file);
                played += 1;
            }
        }
        assert!(played >= 4, "every spell asks for a voice");
    }

    /// WP-AU6: weapon sounds (500xxx). The keyblade objects' motion triggers and hit rows name bank 500;
    /// FUN_00197008 swaps it for the bank the object registered, which in the app is the object's own
    /// `.apdx` block. Before, only the weapon whose block happens to be numbered 500 played them
    /// (`W_EX010`); the others logged "not found" and fell to the global `se500.seb`.
    #[test]
    fn weapon_sounds_resolve_in_every_keyblades_own_bank() {
        let Some(d) = disc() else { return };
        let cache = SeCache::default();
        let mut resolved = 0;
        for name in ["W_EX010", "W_EX020", "W_EX030", "W_EX040", "W_EX050"] {
            let apdx = format!("obj/{name}.apdx");
            let Some(block) = load_bank(&d, &cache, OWN_BANK, Some(&apdx)).map(|b| b.block.clone()) else {
                eprintln!("AU6 {name}: no own bank");
                continue;
            };
            let live: Vec<usize> = block.entries.iter().enumerate().filter(|(_, e)| !e.tracks.is_empty()).map(|(i, _)| i).collect();
            eprintln!("AU6 {name}: block bank {} entries {} with tracks {live:?}", block.bank, block.entries.len());
            for &i in &live {
                let se = motion_sound_request(OBJECT_SOUND_BANK, i as u16, Some(apdx.clone()), Vec3::ZERO);
                assert_eq!(se.bank, OWN_BANK);
                let b = load_bank(&d, &cache, se.bank, se.apdx.as_deref()).expect("own bank");
                assert!(b.block.entries.get(se.index as usize).is_some_and(|e| !e.tracks.is_empty()));
                resolved += 1;
            }
        }
        assert!(resolved > 0, "at least one keyblade sound resolves");
        // The USA disc has no global `se/se500.seb`: the weapons' banks live only in their `.apdx`.
        eprintln!("AU6 global se500 present: {}", load_bank(&d, &cache, 500, None).is_some());
    }

    /// WP-AU6: motion trigger 8 is placed at the object; bank 500 becomes the object's own bank only
    /// when the object has an `.apdx`.
    #[test]
    fn motion_sound_requests_are_placed_and_remap_bank_500() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let own = motion_sound_request(500, 35, Some("obj/W_EX030.apdx".into()), at);
        assert_eq!((own.bank, own.index, own.pos), (OWN_BANK, 35, Some(at)));
        let global = motion_sound_request(500, 35, None, at);
        assert_eq!((global.bank, global.index), (500, 35));
        let other = motion_sound_request(214, 22, Some("obj/M_EX990.apdx".into()), at);
        assert_eq!((other.bank, other.index), (214, 22));
        assert_eq!(placement(own.pos, Some(&Transform::from_translation(at + Vec3::Z * 30.0))).0 < 0x3fff, true);
    }

    /// WP-AU5: a hit sound placed at the victim is quieter and panned when the victim is away from the
    /// camera; unplaced it would play at 0x3fff centre. Hit, effect and enemy requests carry a position.
    #[test]
    fn placed_hit_sounds_fall_with_distance() {
        let cam = Transform::IDENTITY;
        let hit = PlaySe::hit(4, 0, None).expect("a heavy reaction has a hit sound");
        assert_eq!(hit.pos, None, "unplaced until the caller places it");
        let near = hit.clone().at(Vec3::new(0.0, 0.0, -3.0));
        let far = hit.clone().at(Vec3::new(14.0, 0.0, -8.0));
        assert_eq!(far.pos, Some(Vec3::new(14.0, 0.0, -8.0)));
        let (vn, _) = placement(near.pos, Some(&cam));
        let (vf, pf) = placement(far.pos, Some(&cam));
        assert!(vf < vn && vn < 0x3fff, "near {vn:#x} far {vf:#x}");
        assert!(pf > 0x40, "a source to the right pans right: {pf:#x}");
        assert_eq!(placement(hit.pos, Some(&cam)), (0x3fff, 0x40));
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
        let snd = LiveSound { handle: Handle(1), kind: LiveKind::Se(0), pos: Vec3::new(0.0, 0.0, -6.0), last: (0, 0), follow: None };
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
        let snd = LiveSound { handle: Handle(0), kind: LiveKind::Voice(0), pos: source, last: (0, 0), follow: None };
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

    /// A request is one line of the speaker's own bank with its priority; the slot is per speaker.
    #[test]
    fn enemy_voice_requests_carry_their_slot() {
        let a = Entity::from_raw_u32(5).expect("entity");
        let b = Entity::from_raw_u32(6).expect("entity");
        let v = enemy_voice("Seifer", Some("obj/N_EX570_BTL.apdx"), 26, 2, Vec3::new(1.0, 0.0, 2.0), (a, b)).expect("a request");
        assert_eq!((v.line, v.priority, v.in_bar, v.file.as_str()), (26, 2, true, "obj/N_EX570_BTL.apdx"));
        assert_eq!((v.speaker, v.follow, v.pos), (a.to_bits(), Some(b), Some(Vec3::new(1.0, 0.0, 2.0))));
        let other = enemy_voice("Seifer", Some("obj/N_EX570_BTL.apdx"), 17, 2, Vec3::ZERO, (b, a)).expect("a request");
        assert_ne!(other.speaker, v.speaker, "each enemy has its own priority slot");
        assert_ne!(v.speaker, SORA_SLOT);
        // No object file, a negative line: nothing; a large priority is held to a byte.
        assert!(enemy_voice("Dusk", None, 1, 0, Vec3::ZERO, (a, b)).is_none());
        assert!(enemy_voice("Seifer", Some("obj/N_EX570_BTL.apdx"), -1, 0, Vec3::ZERO, (a, b)).is_none());
        assert_eq!(enemy_voice("Seifer", Some("x"), 1, 1000, Vec3::ZERO, (a, b)).unwrap().priority, 255);
        // The slot rule over those requests (FUN_001ae798): a lower priority is dropped while a line
        // is queued, an equal one replaces it.
        let mut slot = kh2_audio::voice::VoiceSlot::default();
        slot.request(17, 2);
        slot.request(26, 1);
        assert_eq!(slot.pending, Some((17, 2)));
        slot.request(37, 2);
        assert_eq!(slot.pending, Some((37, 2)));
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
