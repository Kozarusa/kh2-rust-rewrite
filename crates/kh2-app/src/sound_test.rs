//! Developer sound viewer ("sound test", WP-SV1): steps through every sound the game can play and
//! plays one on demand through the same driver the game uses (`audio::play_se`, the voice bank
//! cache and `Driver::play_voice_at`, `audio::play_bgm`), showing its identifiers on screen in the
//! debug-HUD style. The original game has no sound test the app could port; this overlay is the
//! app's own and is not drawn unless it is opened.
//!
//! Open and close with F10 (`KH2_SOUNDTEST=1` starts open). While open: Left / Right step one id
//! (Shift: 100), PageUp / PageDown step 10 (Shift: 100), Up / Down change the bank, Enter or Space
//! play the current sound (stopping the one before), Backspace stops. The keys the viewer takes
//! and the game's movement keys are swallowed while it is open, so nothing drives Sora or the
//! camera. `KH2_SOUNDTEST_LIST=1` prints every bank with its id count at start.
//!
//! Ids are the ones `KH2_SOUNDLOG=1` prints for the game's own requests: a sound effect is
//! `se <bank>:<index>` (the game's number is `bank * 1000 + index`, `FUN_002c2258`), a voice line
//! is `voice <file>:<line>` (`#0x22` after an object file's name for its own bank), a BGM is the
//! `music<nnn>` number. The log line of every play repeats them: `soundtest bank <name> entry <n>
//! id <id> (<game log form>)`.

use bevy::prelude::*;
use kh2_audio::positional::{CENTRE_PAN, FULL_VOLUME};
use kh2_audio::ssl::{Handle, SeBlock};
use kh2_audio::vsb::Vsb;

use crate::assets::Disc;
use crate::audio::{Audio, PlaySe, SeCache, VoiceInfo};

pub struct SoundTestPlugin;

impl Plugin for SoundTestPlugin {
    fn build(&self, app: &mut App) {
        let open = std::env::var("KH2_SOUNDTEST").is_ok_and(|v| v != "0");
        app.insert_resource(SoundTest { open, ..default() })
            .add_systems(Startup, setup)
            .add_systems(PostStartup, list_banks)
            // The keys are taken in `PreUpdate` after the harness has pressed its scripted ones and
            // before the pause menu reads Enter and Backspace (`menu::input`), so an open viewer owns them.
            .add_systems(PreUpdate, keys.after(bevy::input::InputSystems).after(crate::harness::Keys).before(crate::menu::input))
            // Sora's pad is sampled from the keyboard every frame; while the viewer is open it is dropped
            // right after the sampling, so no held key moves him (the motion viewer's F1 state does the
            // same by leaving his tick early).
            .add_systems(Update, hold_pad.after(crate::sora::read_pad).in_set(crate::schedule::Frame::Pad))
            .add_systems(Update, show.in_set(crate::schedule::Frame::Hud));
    }
}

/// Where a bank's sounds come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// A global sound-effect bank, `se/se<nnn>.seb` (the game's `bank` of a request).
    Se,
    /// An object's private sound-effect bank: the `se` entry of its `obj/<name>.apdx`.
    Obj,
    /// A voice bank: a `voice/<lang>/battle/*.vsb` file of the party, or the entry of type 0x22 of
    /// an enemy's object file.
    Voice,
    /// The BGM sequences `bgm/music<nnn>.bgm` (played in the field slot, replacing the area music).
    Bgm,
}

impl Source {
    fn label(self) -> &'static str {
        match self {
            Source::Se => "SE bank (se/se<nnn>.seb)",
            Source::Obj => "object's own SE bank (obj/<name>.apdx, entry se)",
            Source::Voice => "voice bank",
            Source::Bgm => "BGM (bgm/music<nnn>.bgm, field slot)",
        }
    }
}

/// One id of a bank.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    /// The entry has data (an empty slot is shown as "empty" and skipped when stepping).
    pub filled: bool,
    /// What is known about the data: WD bank and track count of a sound effect, duration and
    /// sample rate of a voice line, the WD of a BGM.
    pub info: String,
}

/// A bank of ids.
#[derive(Clone, Debug)]
pub struct Bank {
    pub source: Source,
    /// The name shown and logged: `se002`, `obj/W_EX010.apdx`, `voice/us/battle/tt0_sora.vsb`, `bgm`.
    pub name: String,
    /// The disc file the bank was read from.
    pub file: String,
    /// The game's bank number of a sound effect (`n / 1000`); the id of entry `i` is `number * 1000 + i`.
    pub number: Option<u16>,
    pub entries: Vec<Entry>,
}

impl Bank {
    /// The id of entry `i` as the game numbers it: `bank * 1000 + index` for a sound effect, the
    /// line for a voice, the music number for a BGM.
    pub fn id(&self, i: usize) -> u32 {
        match self.number {
            Some(b) => b as u32 * 1000 + i as u32,
            None => i as u32,
        }
    }

    /// The form `KH2_SOUNDLOG=1` prints for a request of entry `i`.
    pub fn log_form(&self, i: usize) -> String {
        match (self.source, self.number) {
            (Source::Se, Some(b)) => format!("se {b}:{i}"),
            (Source::Obj, Some(b)) => format!("se {b}:{i} ({})", self.file),
            (Source::Voice, _) => format!("voice {}:{i}", self.name),
            (Source::Bgm, _) => format!("music {i}"),
            (_, None) => format!("{}:{i}", self.name),
        }
    }

    fn filled(&self, i: usize) -> bool {
        self.entries.get(i).is_some_and(|e| e.filled)
    }
}

/// Every bank the viewer knows, in display order: the SE banks by number, the object banks, the
/// voice banks, the BGM.
#[derive(Default, Debug)]
pub struct Catalog {
    pub banks: Vec<Bank>,
}

/// Position in the catalog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub bank: usize,
    pub entry: usize,
}

/// Moves `from` by `delta` in a bank of `len` entries, wrapping at both ends, then walks on in the
/// same direction past empty entries (`filled` is false) until a filled one, wrapping again. When no
/// entry is filled `from` is returned. A `delta` of 0 only leaves an empty entry for the next filled one.
pub fn step_entry(len: usize, filled: impl Fn(usize) -> bool, from: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let wrap = |i: isize| i.rem_euclid(len as isize) as usize;
    let dir = if delta < 0 { -1 } else { 1 };
    let mut at = wrap(from.min(len - 1) as isize + delta);
    for _ in 0..len {
        if filled(at) {
            return at;
        }
        at = wrap(at as isize + dir);
    }
    from
}

/// Moves a bank index by `delta` among `count` banks, wrapping.
pub fn step_bank(count: usize, from: usize, delta: isize) -> usize {
    if count == 0 { 0 } else { (from as isize + delta).rem_euclid(count as isize) as usize }
}

impl Catalog {
    /// The cursor moved by `delta` ids in its bank (empty ids skipped, wrapping).
    pub fn step_entry(&self, c: Cursor, delta: isize) -> Cursor {
        let Some(b) = self.banks.get(c.bank) else { return c };
        Cursor { bank: c.bank, entry: step_entry(b.entries.len(), |i| b.filled(i), c.entry, delta) }
    }

    /// The cursor moved by `delta` banks (wrapping), landing on the new bank's first filled id.
    pub fn step_bank(&self, c: Cursor, delta: isize) -> Cursor {
        let bank = step_bank(self.banks.len(), c.bank, delta);
        let entry = self.banks.get(bank).map_or(0, |b| step_entry(b.entries.len(), |i| b.filled(i), 0, 0));
        Cursor { bank, entry }
    }

    /// Reads every bank from the disc. `lang` is the voice language folder and `speakers` the
    /// speaker names of the executable (`audio::VoiceInfo`).
    pub fn load(disc: &Disc, lang: &str, speakers: &[Option<String>]) -> Catalog {
        let mut banks = Vec::new();
        // The hash lookup of a missing name costs nothing; only the hits are read.
        let read = |name: &str| disc.0.lock().ok().and_then(|mut a| a.read(name).ok());
        for n in 0..1000u16 {
            let file = format!("se/se{n:03}.seb");
            let Some(raw) = read(&file) else { continue };
            let Some(block) = SeBlock::parse(&raw) else { continue };
            banks.push(Bank { source: Source::Se, name: format!("se{n:03}"), file, number: Some(block.bank), entries: se_entries(&block) });
        }
        let mut objects: Vec<String> = vec!["obj/P_EX100.apdx".into(), "obj/W_EX010.apdx".into()];
        objects.extend(crate::enemy::KINDS.iter().map(|k| format!("obj/{}.apdx", k.model)));
        let mut voices: Vec<Bank> = Vec::new();
        for file in objects {
            let Some(raw) = read(&file) else { continue };
            let Ok(bar) = kh2_bar::parse(&raw) else { continue };
            if let Some(block) = bar.iter().find(|e| e.tag.starts_with("se")).and_then(|e| SeBlock::parse(e.data)) {
                banks.push(Bank { source: Source::Obj, name: file.clone(), file: file.clone(), number: Some(block.bank), entries: se_entries(&block) });
            }
            if let Some(vsb) = bar.iter().find(|e| e.kind == crate::audio::VOICE_BAR_ENTRY).and_then(|e| Vsb::parse(e.data)) {
                voices.push(Bank { source: Source::Voice, name: format!("{file}#{:#x}", crate::audio::VOICE_BAR_ENTRY), file: file.clone(), number: None, entries: voice_entries(&vsb) });
            }
        }
        // The party banks: `voice/<lang>/battle/<world><n>_<speaker>.vsb` (`FUN_001ae180`); the
        // area's `arif` record picks `<n>`, so every small number is tried.
        for world in crate::audio::WORLDS {
            for n in 0..8u16 {
                for speaker in speakers.iter().flatten() {
                    let file = kh2_audio::voice::bank_path(lang, world, n, speaker);
                    let Some(raw) = read(&file) else { continue };
                    let Some(vsb) = Vsb::parse(&raw) else { continue };
                    banks.push(Bank { source: Source::Voice, name: file.clone(), file, number: None, entries: voice_entries(&vsb) });
                }
            }
        }
        banks.append(&mut voices);
        let mut bgm = Bank { source: Source::Bgm, name: "bgm".into(), file: "bgm/music<nnn>.bgm".into(), number: None, entries: Vec::new() };
        for n in 0..1000u32 {
            let entry = read(&format!("bgm/music{n:03}.bgm")).and_then(|raw| kh2_audio::Bgm::parse(&raw)).map(|b| Entry { filled: true, info: format!("wd {} tracks {}", b.wd, b.tracks.len()) });
            if let Some(e) = entry {
                bgm.entries.resize(n as usize, Entry::default());
                bgm.entries.push(e);
            }
        }
        if !bgm.entries.is_empty() {
            banks.push(bgm);
        }
        Catalog { banks }
    }
}

fn se_entries(block: &SeBlock) -> Vec<Entry> {
    block
        .entries
        .iter()
        .map(|e| Entry {
            filled: !e.tracks.is_empty(),
            info: if e.tracks.is_empty() { String::new() } else { format!("wd {} tracks {} group {} flags {:#04x}", e.wd, e.tracks.len(), e.group, e.flags) },
        })
        .collect()
}

fn voice_entries(vsb: &Vsb) -> Vec<Entry> {
    (0..vsb.len())
        .map(|i| match vsb.line_info(i) {
            Some((bytes, rate)) => Entry { filled: true, info: format!("{:.2} s at {rate} Hz", (bytes / 16 * 28) as f32 / rate.max(1) as f32) },
            None => Entry::default(),
        })
        .collect()
}

/// What is playing from the viewer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Playing {
    Se(Handle),
    Voice(Handle),
    Bgm,
}

#[derive(Resource, Default)]
pub struct SoundTest {
    pub open: bool,
    catalog: Option<Catalog>,
    cursor: Cursor,
    playing: Option<Playing>,
    /// The last play's log form, kept on screen.
    last: String,
}

#[derive(Component)]
struct SoundTestText;

fn setup(mut commands: Commands) {
    commands.spawn((
        SoundTestText,
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(15.0), ..default() },
        TextColor(Color::srgb(1.0, 0.95, 0.6)),
        Node { position_type: PositionType::Absolute, right: Val::Px(12.0), top: Val::Px(10.0), max_width: Val::Px(640.0), ..default() },
        Visibility::Hidden,
    ));
}

/// `KH2_SOUNDTEST_LIST=1`: every bank with its filled id count, at start.
fn list_banks(disc: Res<Disc>, info: Res<VoiceInfo>, mut st: ResMut<SoundTest>) {
    if !std::env::var("KH2_SOUNDTEST_LIST").is_ok_and(|v| v != "0") {
        return;
    }
    let cat = st.catalog.get_or_insert_with(|| Catalog::load(&disc, &info.lang, &info.names.0));
    let mut total = 0;
    for b in &cat.banks {
        let filled = b.entries.iter().filter(|e| e.filled).count();
        total += filled;
        let first = (0..b.entries.len()).find(|&i| b.filled(i));
        let last = (0..b.entries.len()).rev().find(|&i| b.filled(i));
        match (first, last) {
            (Some(f), Some(l)) => info!("soundtest list {:?} {} ({}): {filled} of {} ids filled, {} .. {}", b.source, b.name, b.file, b.entries.len(), b.log_form(f), b.log_form(l)),
            _ => info!("soundtest list {:?} {} ({}): no ids", b.source, b.name, b.file),
        }
    }
    info!("soundtest list: {} banks, {total} ids", cat.banks.len());
}

/// Keys the viewer takes while open (and swallows), plus the game's movement keys it holds back.
const TAKEN: [KeyCode; 24] = [
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::PageUp,
    KeyCode::PageDown,
    KeyCode::Enter,
    KeyCode::NumpadEnter,
    KeyCode::Space,
    KeyCode::Backspace,
    KeyCode::KeyW,
    KeyCode::KeyA,
    KeyCode::KeyS,
    KeyCode::KeyD,
    KeyCode::KeyJ,
    KeyCode::KeyK,
    KeyCode::KeyE,
    KeyCode::KeyQ,
    KeyCode::KeyC,
    KeyCode::KeyV,
    KeyCode::Tab,
    KeyCode::BracketLeft,
    KeyCode::BracketRight,
    KeyCode::ControlLeft,
];

fn keys(mut keys: ResMut<ButtonInput<KeyCode>>, mut st: ResMut<SoundTest>, disc: Res<Disc>, audio: Res<Audio>, cache: Res<SeCache>, info: Res<VoiceInfo>) {
    if keys.just_pressed(KeyCode::F10) {
        st.open = !st.open;
        info!("soundtest {}", if st.open { "open" } else { "closed" });
    }
    if !st.open {
        return;
    }
    if st.catalog.is_none() {
        let cat = Catalog::load(&disc, &info.lang, &info.names.0);
        info!("soundtest: {} banks", cat.banks.len());
        st.cursor = cat.step_bank(Cursor::default(), 0);
        st.catalog = Some(cat);
    }
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let one: isize = if shift { 100 } else { 1 };
    let ten: isize = if shift { 100 } else { 10 };
    let mut entry_delta = 0isize;
    let mut bank_delta = 0isize;
    if keys.just_pressed(KeyCode::ArrowRight) {
        entry_delta += one;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        entry_delta -= one;
    }
    if keys.just_pressed(KeyCode::PageDown) {
        entry_delta += ten;
    }
    if keys.just_pressed(KeyCode::PageUp) {
        entry_delta -= ten;
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        bank_delta += 1;
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        bank_delta -= 1;
    }
    let play = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) || keys.just_pressed(KeyCode::Space);
    let stop = keys.just_pressed(KeyCode::Backspace);
    for k in TAKEN {
        keys.reset(k);
    }
    let st = &mut *st;
    let Some(cat) = st.catalog.as_ref() else { return };
    if bank_delta != 0 {
        st.cursor = cat.step_bank(st.cursor, bank_delta);
    }
    if entry_delta != 0 {
        st.cursor = cat.step_entry(st.cursor, entry_delta);
    }
    if stop || play {
        stop_playing(&audio, &mut st.playing);
    }
    if play {
        let c = st.cursor;
        if let Some(b) = cat.banks.get(c.bank) {
            let (started, mix) = start(&disc, &audio, &cache, b, c.entry);
            st.playing = started;
            st.last = format!("{} {}", b.log_form(c.entry), if started.is_some() { "playing" } else { "not started" });
            info!(
                "soundtest bank {} entry {} id {} ({}) {} volume {:#x} pan {:#x} {}",
                b.name,
                c.entry,
                b.id(c.entry),
                b.log_form(c.entry),
                b.entries.get(c.entry).map_or("", |e| e.info.as_str()),
                mix.0,
                mix.1,
                if started.is_some() { "playing" } else { "not started" }
            );
        }
    }
}

/// Stops whatever the viewer started (a game sound is left alone).
fn stop_playing(audio: &Audio, playing: &mut Option<Playing>) {
    let Some(p) = playing.take() else { return };
    let Ok(mut s) = audio.0.lock() else { return };
    let Some(d) = s.driver.as_mut() else { return };
    match p {
        Playing::Se(h) => d.stop_se(h),
        Playing::Voice(h) => d.stop_voice(h),
        Playing::Bgm => d.bgm_stop(0, false),
    }
}

/// Plays entry `i` of `bank` the way the game would for a request without a position: an interface
/// sound (`FUN_002c3318`: volume 0x3fff, pan 0x40) for a sound effect, a voice line at the same
/// level (the game's voices are positional with a minimum volume; here the listener is at the
/// speaker), a BGM in the field slot at the full BGM level. Returns what started and the mix it was
/// asked for.
fn start(disc: &Disc, audio: &Audio, cache: &SeCache, bank: &Bank, i: usize) -> (Option<Playing>, (i32, i32)) {
    let mix = (FULL_VOLUME, CENTRE_PAN);
    if !bank.filled(i) {
        return (None, mix);
    }
    match bank.source {
        Source::Se | Source::Obj => {
            let Some(number) = bank.number else { return (None, mix) };
            let apdx = (bank.source == Source::Obj).then(|| bank.file.clone());
            let req = PlaySe { bank: number, index: i as u16, apdx, pos: None };
            (crate::audio::play_se(disc, audio, cache, &req, mix).map(|(h, _)| Playing::Se(h)), mix)
        }
        Source::Voice => {
            let in_bar = bank.name.contains('#');
            let Some(vsb) = crate::audio::voice_bank_file(disc, cache, &bank.file, in_bar) else { return (None, mix) };
            let Some((sample, rate)) = vsb.line(i) else { return (None, mix) };
            let Ok(mut s) = audio.0.lock() else { return (None, mix) };
            let Some(d) = s.driver.as_mut() else { return (None, mix) };
            (Some(Playing::Voice(d.play_voice_at(std::sync::Arc::new(sample), rate, mix.0, mix.1))), mix)
        }
        Source::Bgm => {
            let mix = (kh2_audio::ssl::BGM_FULL, CENTRE_PAN);
            (crate::audio::play_bgm(disc, audio, i as u32).then_some(Playing::Bgm), mix)
        }
    }
}

/// Drops the pad the frame sampled while the viewer is open: no key moves Sora, jumps or attacks.
fn hold_pad(st: Res<SoundTest>, mut pad: ResMut<crate::sora::Pad>) {
    if st.open {
        *pad = crate::sora::Pad::default();
    }
}

fn show(st: Res<SoundTest>, audio: Res<Audio>, mut text: Query<(&mut Text, &mut Visibility), With<SoundTestText>>) {
    let Ok((mut t, mut vis)) = text.single_mut() else { return };
    *vis = if st.open { Visibility::Inherited } else { Visibility::Hidden };
    if !st.open {
        return;
    }
    let Some(cat) = st.catalog.as_ref() else {
        t.0 = "[SOUND TEST]  loading banks...".into();
        return;
    };
    let c = st.cursor;
    let Some(b) = cat.banks.get(c.bank) else { return };
    let entry = b.entries.get(c.entry);
    let filled = b.entries.iter().filter(|e| e.filled).count();
    let still = st.playing.is_some_and(|p| {
        audio.0.lock().ok().and_then(|s| {
            let d = s.driver.as_ref()?;
            Some(match p {
                Playing::Se(h) => d.se_playing(h),
                Playing::Voice(h) => d.voice_playing(h),
                Playing::Bgm => d.bgm[0].playing(),
            })
        }) == Some(true)
    });
    let (volume, pan) = if b.source == Source::Bgm { (kh2_audio::ssl::BGM_FULL, CENTRE_PAN) } else { (FULL_VOLUME, CENTRE_PAN) };
    t.0 = format!(
        "[SOUND TEST]  F10 close | Left/Right id (Shift 100) | PgUp/PgDn 10 | Up/Down bank | Enter/Space play | Backspace stop\n\
         source: {}\n\
         bank {}/{}: {}  ({})\n\
         entry {}/{} ({} filled)  id {}  log: {}\n\
         data: {}\n\
         mix: volume {:#x} pan {:#x} (no position: the game's interface-sound mix)\n\
         {}",
        b.source.label(),
        c.bank + 1,
        cat.banks.len(),
        b.name,
        b.file,
        c.entry,
        b.entries.len(),
        filled,
        b.id(c.entry),
        b.log_form(c.entry),
        match entry {
            Some(e) if e.filled => e.info.as_str(),
            _ => "empty",
        },
        volume,
        pan,
        if st.last.is_empty() {
            String::new()
        } else {
            format!("last: {}{}", st.last, if still { " (sounding)" } else { "" })
        }
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bank(name: &str, filled: &[bool]) -> Bank {
        Bank {
            source: Source::Se,
            name: name.into(),
            file: format!("se/{name}.seb"),
            number: Some(2),
            entries: filled.iter().map(|&f| Entry { filled: f, info: String::new() }).collect(),
        }
    }

    /// Stepping one id passes over empty ids in the direction of travel.
    #[test]
    fn soundtest_step_skips_empty_ids() {
        // ids: 0 filled, 1 empty, 2 empty, 3 filled, 4 filled
        let cat = Catalog { banks: vec![bank("se002", &[true, false, false, true, true])] };
        let c = Cursor { bank: 0, entry: 0 };
        let c = cat.step_entry(c, 1);
        assert_eq!(c.entry, 3);
        let c = cat.step_entry(c, 1);
        assert_eq!(c.entry, 4);
        let c = cat.step_entry(c, -1);
        assert_eq!(c.entry, 3);
        let c = cat.step_entry(c, -1);
        assert_eq!(c.entry, 0, "backwards over two empties");
        // A bank of only empties keeps the cursor where it was.
        let empty = Catalog { banks: vec![bank("se003", &[false, false])] };
        assert_eq!(empty.step_entry(Cursor { bank: 0, entry: 1 }, 1).entry, 1);
    }

    /// Both ends wrap: past the last id comes the first, before the first the last (filled ones).
    #[test]
    fn soundtest_step_wraps_at_ends() {
        let cat = Catalog { banks: vec![bank("se002", &[false, true, true, false])] };
        let c = cat.step_entry(Cursor { bank: 0, entry: 2 }, 1);
        assert_eq!(c.entry, 1, "3 is empty, 0 is empty, 1 is the first filled");
        let c = cat.step_entry(Cursor { bank: 0, entry: 1 }, -1);
        assert_eq!(c.entry, 2, "0 is empty, wraps to 3 (empty), then 2");
        // A plain step on a full bank.
        let full = Catalog { banks: vec![bank("se004", &[true; 5])] };
        assert_eq!(full.step_entry(Cursor { bank: 0, entry: 4 }, 1).entry, 0);
        assert_eq!(full.step_entry(Cursor { bank: 0, entry: 0 }, -1).entry, 4);
    }

    /// Jumps of 10 and 100 land `delta` ids away (modulo the bank), then skip empties forward.
    #[test]
    fn soundtest_jumps_by_10_and_100() {
        let mut filled = vec![true; 250];
        filled[10] = false;
        filled[11] = false;
        let cat = Catalog { banks: vec![bank("se002", &filled)] };
        let c = Cursor { bank: 0, entry: 0 };
        assert_eq!(cat.step_entry(c, 10).entry, 12, "10 and 11 empty: lands on 12");
        assert_eq!(cat.step_entry(c, 100).entry, 100);
        assert_eq!(cat.step_entry(Cursor { bank: 0, entry: 200 }, 100).entry, 50, "wraps: 300 mod 250");
        assert_eq!(cat.step_entry(Cursor { bank: 0, entry: 5 }, -10).entry, 245, "wraps backwards");
        assert_eq!(cat.step_entry(Cursor { bank: 0, entry: 21 }, -10).entry, 9, "11 and 10 empty going down: lands on 9");
    }

    /// Up / Down wrap over the bank list and land on the new bank's first filled id.
    #[test]
    fn soundtest_bank_step_wraps_and_lands_on_first_filled() {
        let cat = Catalog { banks: vec![bank("a", &[true, true]), bank("b", &[false, false, true]), bank("c", &[true])] };
        let c = Cursor { bank: 0, entry: 1 };
        let c = cat.step_bank(c, 1);
        assert_eq!(c, Cursor { bank: 1, entry: 2 }, "first filled id of bank b is 2");
        let c = cat.step_bank(c, 1);
        assert_eq!(c, Cursor { bank: 2, entry: 0 });
        let c = cat.step_bank(c, 1);
        assert_eq!(c, Cursor { bank: 0, entry: 0 }, "wraps to the first bank");
        let c = cat.step_bank(c, -1);
        assert_eq!(c, Cursor { bank: 2, entry: 0 }, "wraps backwards to the last bank");
        // Ids and log forms follow the game's numbering: bank * 1000 + index, `se <bank>:<index>`.
        assert_eq!(cat.banks[0].id(1), 2001);
        assert_eq!(cat.banks[0].log_form(1), "se 2:1");
        let voice = Bank { source: Source::Voice, name: "voice/us/battle/tt0_sora.vsb".into(), file: "voice/us/battle/tt0_sora.vsb".into(), number: None, entries: vec![] };
        assert_eq!(voice.log_form(5), "voice voice/us/battle/tt0_sora.vsb:5");
        let bgm = Bank { source: Source::Bgm, name: "bgm".into(), file: String::new(), number: None, entries: vec![] };
        assert_eq!(bgm.log_form(50), "music 50");
        assert_eq!(bgm.id(50), 50);
    }

    /// The pure stepping function with an empty bank and a zero delta.
    #[test]
    fn soundtest_step_entry_edge_cases() {
        assert_eq!(step_entry(0, |_| true, 3, 1), 0);
        assert_eq!(step_entry(4, |i| i == 2, 0, 0), 2, "delta 0 moves an empty start to the next filled");
        assert_eq!(step_entry(4, |i| i == 2, 2, 0), 2);
        assert_eq!(step_bank(0, 0, 1), 0);
        assert_eq!(step_bank(3, 0, -1), 2);
    }
}
