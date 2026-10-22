//! The game's IOP sound driver (Square-Enix "SSL"), reimplemented from its behaviour: sequence
//! players for BGM and sound effects, per-track slides and LFOs, and the voice pitch/volume/pan
//! math the driver writes to the SPU2. Studied in a local disassembly of the user's own
//! `LIBSSL.IRX`; nothing copied. The driver's lookup tables (semitone pitches, fine tune, pan laws)
//! are read from that file on the user's disc at runtime ([`Tables::from_irx`]).
//!
//! Timing: the driver's main loop runs at 240 Hz; each player subtracts its tempo from an
//! accumulator and runs one sequence tick for every 300 it goes below zero (BGM PPQN 48 ⇒ tempo is
//! in beats per minute). LFOs advance once per 240 Hz frame.

use std::collections::HashMap;
use std::sync::Arc;

use crate::adpcm::{self, Sample};
use crate::synth::{Envelope, Mixer, Voice};
use crate::wd::Wd;

/// Driver frames per second.
pub const FRAME_HZ: u32 = 240;

/// The game's sound-bank number of the system bank `se/se000.seb` (with `se/wave0000.wd`): the
/// interface sounds, the HUD alarm, item and pick-up sounds. The boot routine `FUN_002c0eb0` loads
/// it into driver slot 2 (`FUN_002c1378(2, ..)` at 0x2c0f10; the wave bank by `FUN_002c1348(2, ..)`
/// at 0x2c0f74 [read from the decompilation of 0x2c0eb0]) and `FUN_002c0da8` registers slot 2 under
/// bank number 0 (`FUN_002c3a90(0x1e33b50, 2, 0)`).
pub const SYSTEM_BANK: u16 = 0;

/// The second boot bank, `se/se999.seb` (with `se/wave0001.wd`): driver slot 3, bank number 999
/// (`FUN_002c0da8`: `FUN_002c3a90(0x1e33b50, 3, 999)`).
pub const SYSTEM_BANK_2: u16 = 999;

/// The driver slot a boot bank sits in (the first argument of `FUN_002be49c`): 2 for bank 0, 3 for
/// bank 999. Other banks (the area's own, e.g. 697) are loaded per area into other slots.
pub fn boot_slot(bank: u16) -> Option<u8> {
    match bank {
        SYSTEM_BANK => Some(2),
        SYSTEM_BANK_2 => Some(3),
        _ => None,
    }
}

/// Lookup tables from the driver binary.
#[derive(Clone)]
pub struct Tables {
    /// SPU pitch of the 12 semitones in the top octave.
    pub semitone: [u32; 12],
    /// 256 fine-tune multipliers (16.16).
    pub fine: [u32; 256],
    /// Pan laws: (left, right) for normal and "phase" output, 256 entries each.
    pub pan_l: [i16; 256],
    pub pan_r: [i16; 256],
}

impl Tables {
    /// Reads the tables from `LIBSSL.IRX` (USA build, 2006-01-13). Offsets are the driver's data
    /// addresses relative to its load address, plus the module's segment file offset.
    pub fn from_irx(irx: &[u8]) -> Option<Tables> {
        if irx.get(..4) != Some(b"\x7fELF") || !irx.windows(5).any(|w| w == b"Ohori") {
            return None;
        }
        // Second program header (the load segment): p_offset.
        let seg = u32::from_le_bytes(irx.get(0x58..0x5c)?.try_into().ok()?) as usize;
        let at = |va: usize| seg + va;
        let u32_at = |o: usize| irx.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()));
        let i16_at = |o: usize| irx.get(o..o + 2).map(|s| i16::from_le_bytes([s[0], s[1]]));
        let mut t = Tables { semitone: [0; 12], fine: [0; 256], pan_l: [0; 256], pan_r: [0; 256] };
        for i in 0..12 {
            t.semitone[i] = u32_at(at(0xff60 + 4 * i))?;
        }
        for i in 0..256 {
            t.fine[i] = u32_at(at(0xff90 + 4 * i))?;
            t.pan_l[i] = i16_at(at(0x10590 + 2 * i))?;
            t.pan_r[i] = i16_at(at(0x10390 + 2 * i))?;
        }
        (t.semitone[0] != 0).then_some(t)
    }

    /// SPU pitch (0x1000 = 48 kHz) from the driver's 8.24 pitch sum.
    pub fn pitch(&self, sum: i64) -> u32 {
        let sum = sum.max(0) as u32;
        let note = (sum >> 24) & 0x7f;
        let fine = self.fine[((sum >> 16) & 0xff) as usize];
        let base = self.semitone[(note % 12) as usize] >> (10i32 - (note / 12) as i32).clamp(0, 31);
        ((base as u64 * fine as u64) >> 16).min(0x3fff) as u32
    }
}

/// A linear slide (the driver's 16-byte slide unit): value moves to target over `count` ticks.
#[derive(Clone, Copy, Debug, Default)]
struct Slide {
    active: bool,
    count: i32,
    value: i32,
    target: i32,
    step: i32,
}

impl Slide {
    /// Sets `target` (shifted) reached after `time` ticks (0 = at once). Shift 0 uses the LFO
    /// depth encoding (0..0x7f ⇒ ×2^17, above ⇒ (v−0x80)·0x180000).
    fn set(&mut self, time: i32, target: i32, shift: u32) {
        let t = if shift == 0 { if target < 0x80 { target << 17 } else { (target - 0x80) * 0x180000 } } else { target << shift };
        if time == 0 {
            *self = Slide { value: t, ..Default::default() };
            return;
        }
        self.active = true;
        self.count = time;
        self.target = t;
        self.step = (t - self.value) / time;
    }

    /// Relative pitch slide: target = current target + semitones (×0x10000).
    fn relative(&mut self, time: i32, delta: i32) {
        if time == 0 || delta == 0 {
            *self = Slide::default();
            return;
        }
        let t = self.target + delta * 0x10000;
        self.active = true;
        self.count = time;
        self.step = (t - self.value) / time;
        self.target = t;
    }

    /// Portamento from `delta` semitones away back to 0 over `time` ticks.
    fn glide(&mut self, time: i32, delta: i32) {
        if time == 0 {
            *self = Slide::default();
            return;
        }
        let v = delta << 16;
        *self = Slide { active: true, count: time, value: v, target: 0, step: -(v / time) };
    }

    fn tick(&mut self) {
        if self.active {
            self.count -= 1;
            self.value += self.step;
            if self.count == 0 {
                self.active = false;
                self.value = self.target;
            }
        }
    }
}

/// An LFO unit (vibrato, tremolo, auto-pan): delay, then a waveform with depth and period.
#[derive(Clone, Copy, Debug, Default)]
struct Lfo {
    state: i32,
    phase: i32,
    out: i32,
    depth_cur: i32,
    depth: Slide,
    rate: Slide,
    wave: i32,
    delay: i32,
    fade: i32,
    target: i32,
}

impl Lfo {
    #[allow(clippy::too_many_arguments)] // mirrors the seven LFO parameters of the sequence command
    fn set(&mut self, on: bool, depth: i32, rate: i32, wave: i32, delay: i32, fade: i32, target: i32) {
        let rate = if rate == 0 { 0x100 } else { rate };
        if !on {
            *self = Lfo { fade: if fade == 0 { 0x100 } else { fade }, ..Default::default() };
            self.depth.set(0, depth.max(0), 0);
            self.rate.set(0, rate.max(0), 16);
            return;
        }
        self.state = 1;
        self.phase = 0;
        self.out = 0;
        self.depth_cur = 0;
        if depth != -1 {
            self.depth.set(0, depth, 0);
        }
        if rate != -1 {
            self.rate.set(0, rate, 16);
        }
        if wave != -1 {
            self.wave = wave;
        }
        if delay != -1 {
            self.delay = delay << 2;
        }
        if fade != -1 {
            self.fade = fade << 2;
        }
        if target != -1 {
            self.target = target;
        }
    }

    /// Restart at note-on (keeps settings).
    fn retrigger(&mut self) {
        self.state = if self.delay == 0 { 2 } else { 1 };
        self.phase = 0;
        self.out = 0;
        self.depth_cur = 0;
    }

    /// One driver frame (240 Hz).
    fn step(&mut self, rng: &mut u32, tables: &Tables) {
        if self.state == 0 {
            return;
        }
        let depth_full = self.depth.value;
        if depth_full == 0 {
            return;
        }
        if self.state == 1 {
            if self.phase < self.delay {
                self.phase += 1;
                return;
            }
            self.phase = 0;
            self.state = 2;
            return;
        }
        if self.fade == 0 {
            self.depth_cur = depth_full;
        } else {
            self.depth_cur = (self.depth_cur + depth_full / self.fade).min(depth_full);
        }
        let amp = (self.depth_cur >> 16) as i16 as i32; // the driver reads the high half
        let period = (self.rate.value >> 16) as i16 as i32;
        let period = period.max(1);
        match self.wave & 7 {
            0 => {
                // Sine (from the pan-law table, which is a sine quarter).
                let idx = (self.phase >> 16).clamp(0, 0xff) as usize;
                let s = tables.pan_r[idx] as i32 * amp / 0x3fff;
                self.out = if self.state == 2 { s } else { -s };
                self.phase += 0x80_0000 / period;
                if self.phase > 0x100_0000 {
                    self.phase -= 0x100_0000;
                    self.state = if self.state + 1 < 4 { self.state + 1 } else { 2 };
                }
            }
            1 => {
                // Triangle.
                self.out = match self.state {
                    2 => amp * self.phase / period,
                    3 => amp * (period - self.phase) / period,
                    4 => -(amp * self.phase / period),
                    _ => -(amp * (period - self.phase) / period),
                };
                self.phase += 1;
                if self.phase >= period {
                    self.phase = 0;
                    self.state = if self.state + 1 < 6 { self.state + 1 } else { 2 };
                }
            }
            2 => {
                // Saw.
                self.out = if self.wave & 8 == 0 { amp * self.phase / (period * 2) - amp } else { amp - amp * self.phase / (period * 2) };
                self.phase += 1;
                if period * 4 < self.phase {
                    self.phase = 0;
                }
            }
            3 => {
                // Square.
                self.out = if self.state == 2 { amp } else { -amp };
                self.phase += 1;
                if self.phase >= period * 2 {
                    self.phase = 0;
                    self.state = if self.state + 1 < 4 { self.state + 1 } else { 2 };
                }
            }
            _ => {
                // Random.
                if self.phase == 0 {
                    self.out = if amp == 0 { 0 } else { rand(rng, amp * 2) - amp };
                }
                self.phase += 1;
                if period <= self.phase {
                    self.phase = 0;
                }
            }
        }
    }
}

fn rand(state: &mut u32, n: i32) -> i32 {
    let x = *state ^ (*state << 17);
    *state = x ^ ((x as i32 >> 15) as u32);
    (*state as i32).rem_euclid(n + 1)
}

/// One sequence track.
#[derive(Clone, Debug)]
struct Track {
    data: Arc<[u8]>,
    pos: usize,
    wait: i32,
    done: bool,
    loop_ret: [usize; 4],
    loop_cnt: [u8; 4],
    loop_depth: usize,
    inf_loop: Option<usize>,
    program: usize,
    key: u8,
    last_key: u8,
    velocity: u8,
    sustain: bool,
    porta_time: i32,
    porta: bool,
    transpose: i32,
    detune: i32,
    pitch_add: i32,
    pan_add: i32,
    vol_factor: i32,
    /// Per-note random ranges set by command 0x78 (pitch, volume, pan); 0 = off.
    rand_pitch: i32,
    rand_vol: i32,
    rand_pan: i32,
    adsr_override: Option<(u16, u16)>,
    volume: Slide,
    expression: Slide,
    pan: Slide,
    bend: Slide,
    glide: Slide,
    vibrato: Lfo,
    tremolo: Lfo,
    autopan: Lfo,
    extra: [Lfo; 3],
    /// Mixer voice tags of this track's sounding notes.
    voices: Vec<(u32, u8)>,
}

impl Track {
    fn new(data: Arc<[u8]>) -> Track {
        let mut t = Track {
            data,
            pos: 0,
            wait: 0,
            done: false,
            loop_ret: [0; 4],
            loop_cnt: [0; 4],
            loop_depth: 0,
            inf_loop: None,
            program: 0,
            key: 0xff,
            last_key: 0xfe,
            velocity: 0x7f,
            sustain: false,
            porta_time: 0,
            porta: false,
            transpose: 0,
            detune: 0,
            pitch_add: 0,
            pan_add: 0,
            vol_factor: 0x3fff,
            rand_pitch: 0,
            rand_vol: 0,
            rand_pan: 0,
            adsr_override: None,
            volume: Slide::default(),
            expression: Slide::default(),
            pan: Slide::default(),
            bend: Slide::default(),
            glide: Slide::default(),
            vibrato: Lfo::default(),
            tremolo: Lfo::default(),
            autopan: Lfo::default(),
            extra: [Lfo::default(); 3],
            voices: Vec::new(),
        };
        t.volume.set(0, 0x7f, 16);
        t.expression.set(0, 0x7f, 16);
        t.pan.set(0, 0x40, 16);
        t.wait = t.varlen() as i32;
        t
    }

    fn byte(&mut self) -> u8 {
        let b = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        b
    }

    fn varlen(&mut self) -> u32 {
        let mut v = self.byte() as u32;
        if v & 0x80 != 0 {
            v &= 0x7f;
            loop {
                let b = self.byte();
                v = v * 0x80 + (b & 0x7f) as u32;
                if b & 0x80 == 0 {
                    break;
                }
            }
        }
        v
    }

    fn slides_tick(&mut self) {
        self.volume.tick();
        self.expression.tick();
        self.pan.tick();
        self.bend.tick();
        self.glide.tick();
        for l in [&mut self.vibrato, &mut self.tremolo, &mut self.autopan] {
            l.depth.tick();
            l.rate.tick();
        }
        for l in &mut self.extra {
            l.depth.tick();
            l.rate.tick();
        }
    }
}

/// A sequence player: one BGM or one sound effect.
pub struct Sequence {
    tracks: Vec<Track>,
    tempo: i32,
    acc: i32,
    wd: Arc<Wd>,
    /// Player volume and pan (0x3fff full, 0x40 centre).
    pub volume: i32,
    pub pan: i32,
    /// The slot volume the BGM fades move (the IOP sequence struct's +0x44; 0x3000 is the
    /// game's full BGM level, 0x3fff for a sound effect). Inferred to be the second fixed factor
    /// of the volume chain; its exact position in the chain was not traced, which can change the
    /// result by one 14-bit step.
    pub slot_volume: i32,
    /// Base voice tag.
    tag: u32,
    pub finished: bool,
    samples: HashMap<usize, Arc<Sample>>,
}

impl Sequence {
    /// A BGM: header `"BGM "`, tracks from 0x20.
    pub fn bgm(b: &[u8], wd: Arc<Wd>, tag: u32) -> Option<Sequence> {
        let bgm = crate::bgm::Bgm::parse(b)?;
        let vol = ((b[0xc] as i32) << 7).clamp(0, 0x3fff);
        Some(Sequence::new(bgm.tracks.into_iter().map(Arc::from).collect(), wd, vol, 0x40, tag))
    }

    pub fn new(tracks: Vec<Arc<[u8]>>, wd: Arc<Wd>, volume: i32, pan: i32, tag: u32) -> Sequence {
        Sequence {
            tracks: tracks.into_iter().map(Track::new).collect(),
            tempo: 0x78,
            acc: 0,
            wd,
            volume,
            pan,
            slot_volume: 0x3fff,
            tag,
            finished: false,
            samples: HashMap::new(),
        }
    }

    /// Releases every sounding note of this player (the driver keys its voices off when a
    /// sequence stops, FUN_108900 in the IOP module).
    pub fn release_all(&mut self, mixer: &mut Mixer) {
        for (ti, t) in self.tracks.iter_mut().enumerate() {
            mixer.release(self.tag + ti as u32, None);
            t.voices.clear();
        }
    }

    /// One 240 Hz driver frame: sequence ticks per tempo, then LFOs, then voice parameters.
    pub fn frame(&mut self, mixer: &mut Mixer, tables: &Tables, rng: &mut u32) {
        if self.finished {
            return;
        }
        self.acc -= self.tempo;
        while self.acc < 0 {
            self.acc += 300;
            for ti in 0..self.tracks.len() {
                self.track_tick(ti, mixer, rng);
            }
        }
        for t in &mut self.tracks {
            t.vibrato.step(rng, tables);
            t.tremolo.step(rng, tables);
            t.autopan.step(rng, tables);
            for l in &mut t.extra {
                l.step(rng, tables);
            }
        }
        self.update_voices(mixer, tables);
        if self.tracks.iter().all(|t| t.done) {
            self.finished = mixer.voices.iter().all(|v| v.tag / 64 != self.tag / 64);
        }
    }

    fn track_tick(&mut self, ti: usize, mixer: &mut Mixer, rng: &mut u32) {
        if self.tracks[ti].done {
            return;
        }
        let mut guard = 0;
        while self.tracks[ti].wait == 0 && !self.tracks[ti].done && guard < 512 {
            guard += 1;
            self.event(ti, mixer, rng);
            if self.tracks[ti].done {
                break;
            }
            let d = self.tracks[ti].varlen() as i32;
            self.tracks[ti].wait = d;
        }
        if self.tracks[ti].wait > 0 {
            self.tracks[ti].wait -= 1;
        }
        self.tracks[ti].slides_tick();
    }

    fn note_on(&mut self, ti: usize, key: u8, vel: u8, mixer: &mut Mixer, rng: &mut u32) {
        let tag = self.tag + ti as u32;
        let t = &mut self.tracks[ti];
        // Per-note randomisation first, in the driver's order (FUN_00106fd0 calls 0x1091d8,
        // 0x109228, 0x109270): pitch offset, volume factor, pan offset.
        if t.rand_pitch != 0 {
            t.pitch_add = rand(rng, t.rand_pitch * 0x18_0000) - t.rand_pitch * 0xc_0000;
        }
        if t.rand_vol != 0 {
            t.vol_factor = 0x3fff - rand(rng, t.rand_vol * 0x40 - 1);
        }
        if t.rand_pan != 0 {
            t.pan_add = (rand(rng, t.rand_pan * 2) - t.rand_pan) * 0x40;
        }
        if t.porta && t.porta_time != 0 && t.last_key != 0xfe {
            let d = t.last_key as i32 - key as i32;
            t.glide.glide(t.porta_time, d);
        }
        // Region by key, then by velocity (byte 0x15 of each region is its velocity ceiling).
        let wd = self.wd.clone();
        let Some(inst) = wd.instruments.get(t.program) else { return };
        let regs = &inst.regions;
        let Some(ri) = regs
            .iter()
            .position(|r| key <= r.key_high && vel <= r.velocity_high)
            .or_else(|| regs.iter().position(|r| key <= r.key_high))
            .or(regs.len().checked_sub(1))
        else {
            return;
        };
        let stereo = regs[ri].stereo && ri + 1 < regs.len();
        // Release this track's previous note unless sustained.
        if !t.sustain {
            for &(vt, k) in &t.voices {
                mixer.release(vt, Some(k));
            }
            t.voices.clear();
        }
        t.vibrato.retrigger();
        t.tremolo.retrigger();
        t.autopan.retrigger();
        for l in &mut t.extra {
            if l.target < 2 {
                l.retrigger();
            }
        }
        t.key = key;
        t.last_key = key;
        t.velocity = vel;
        let ovr = t.adsr_override;
        for side in 0..if stereo { 2 } else { 1 } {
            let r = regs[ri + side];
            let (a1, a2) = ovr.unwrap_or((r.adsr1, r.adsr2));
            let sample = self.samples.entry(r.sample).or_insert_with(|| Arc::new(adpcm::decode(&wd.samples, r.sample))).clone();
            let mut v = Voice::new(sample, 1.0, Envelope::new(a1, a2), 0.0, 0.0, tag, key);
            v.region = ri + side;
            v.side = if stereo { side as i8 * 2 - 1 } else { 0 };
            mixer.play(v);
            self.tracks[ti].voices.push((tag, key));
        }
    }

    fn note_off(&mut self, ti: usize, key: u8, mixer: &mut Mixer) {
        let tag = self.tag + ti as u32;
        let t = &mut self.tracks[ti];
        if t.sustain {
            return;
        }
        mixer.release(tag, Some(key));
        t.voices.retain(|&(_, k)| k != key);
    }

    /// Pitch, volume and pan of every voice of this player (the driver's per-frame voice update).
    fn update_voices(&mut self, mixer: &mut Mixer, tables: &Tables) {
        for v in mixer.voices.iter_mut() {
            if v.tag < self.tag || v.tag >= self.tag + self.tracks.len() as u32 {
                continue;
            }
            let t = &self.tracks[(v.tag - self.tag) as usize];
            let Some(r) = self.wd.instruments.get(t.program).and_then(|i| i.regions.get(v.region)) else { continue };
            // Pitch sum (8.24): region tune word, key, transpose/detune, slides and LFOs.
            let mut lfo_pitch = 0i64;
            for l in &t.extra {
                if l.target == 0 {
                    lfo_pitch += l.out as i64;
                }
            }
            let sum = r.tune as i64
                + ((v.key as i64) << 24)
                + ((t.detune as i64) << 17)
                + ((t.transpose as i64) << 24)
                + ((t.bend.value as i64) << 8)
                + ((t.glide.value as i64) << 8)
                + ((t.vibrato.out as i64) << 16)
                + t.pitch_add as i64
                + (lfo_pitch << 16);
            let pitch = tables.pitch(sum);
            v.step = pitch as f64 / 4096.0;
            // Volume chain (14-bit fixed point at each step).
            let mut trem = 0i32;
            let mut lfo_pan = 0i32;
            for l in &t.extra {
                match l.target {
                    1 => trem += l.out,
                    2 => lfo_pan += l.out,
                    _ => {}
                }
            }
            let mut vol = (0x3fff * 0x3fff) >> 14;
            vol = (vol * (t.volume.value >> 9)) >> 14;
            vol = (vol * (t.expression.value >> 9)) >> 14;
            vol = (vol * v.velocity as i32) >> 7;
            vol = (vol * (r.volume_raw & 0x7f) as i32) >> 7;
            vol = (vol * self.slot_volume) >> 14;
            vol = (vol * (0xff - t.tremolo.out)) >> 8;
            vol = (vol * (0xff - trem)) >> 8;
            vol = ((vol * t.vol_factor) >> 14).clamp(0, 0x3fff);
            vol = (vol * self.volume) >> 14;
            let region_pan = if r.pan_raw & 0x80 != 0 { (r.pan_raw & 0x7f) as i32 } else { 0x40 };
            let pan = ((((self.pan & 0x1ff) + region_pan + ((t.pan.value >> 16) as i16 as i32) - 0xc0) * 0x80
                + t.autopan.out * 0x20
                + t.pan_add
                + lfo_pan * 0x20)
                >> 7)
                + 0x40;
            let pan = (pan & 0x1ff) as usize;
            let (l, rr) = if pan < 0x100 {
                ((vol * tables.pan_l[pan] as i32) >> 14, (vol * tables.pan_r[pan] as i32) >> 14)
            } else {
                ((vol * -(tables.pan_l[pan & 0xff] as i32)) >> 14, (vol * -(tables.pan_r[pan & 0xff] as i32)) >> 14)
            };
            // The SPU takes a 15-bit signed volume: a negative law value (a source behind the
            // listener) inverts the phase of that channel.
            let spu = |v: i32| {
                let v = v & 0x7fff;
                (if v & 0x4000 != 0 { v - 0x8000 } else { v }) as f32 / 16384.0
            };
            let (l, rr) = (spu(l), spu(rr));
            match v.side {
                -1 => v.set_lr(l + rr, 0.0),
                1 => v.set_lr(0.0, l + rr),
                _ => v.set_lr(l, rr),
            }
        }
    }

    fn event(&mut self, ti: usize, mixer: &mut Mixer, rng: &mut u32) {
        let t = &mut self.tracks[ti];
        let cmd = t.byte();
        match cmd {
            0x00 => {
                t.done = true;
                let tag = self.tag + ti as u32;
                mixer.release(tag, None);
            }
            0x02 => t.inf_loop = Some(t.pos),
            0x03 => {
                if let Some(l) = t.inf_loop {
                    t.pos = l;
                }
            }
            0x04 => {
                if t.loop_depth < 4 {
                    t.loop_cnt[t.loop_depth] = 0;
                    t.loop_ret[t.loop_depth] = t.pos;
                    t.loop_depth += 1;
                }
            }
            0x05 => {
                let n = t.byte();
                if t.loop_depth > 0 {
                    let d = t.loop_depth - 1;
                    t.loop_cnt[d] = t.loop_cnt[d].wrapping_add(1);
                    if t.loop_cnt[d] < n {
                        t.pos = t.loop_ret[d];
                    } else {
                        t.loop_depth -= 1;
                    }
                }
            }
            0x08 => self.tempo = t.byte() as i32,
            0x10 => {
                let (k, v) = (t.key, t.velocity);
                self.note_on(ti, k, v, mixer, rng);
            }
            0x11 => {
                let k = t.byte();
                let v = t.byte();
                self.note_on(ti, k, v, mixer, rng);
            }
            0x12 => {
                let k = t.byte();
                let v = t.velocity;
                self.note_on(ti, k, v, mixer, rng);
            }
            0x13 => {
                let v = t.byte();
                let k = t.key;
                self.note_on(ti, k, v, mixer, rng);
            }
            0x18 => {
                let k = t.key;
                self.note_off(ti, k, mixer);
            }
            0x19 => {
                let k = t.byte();
                let _v = t.byte();
                self.note_off(ti, k, mixer);
            }
            0x1a => {
                let k = t.byte();
                self.note_off(ti, k, mixer);
            }
            0x1b => {
                let _v = t.byte();
                let k = t.key;
                self.note_off(ti, k, mixer);
            }
            0x20 => {
                let p = t.byte() as usize;
                t.program = if p < self.wd.instruments.len() { p } else { 0 };
            }
            0x21 => {
                let _bank = t.byte();
                let p = t.byte() as usize;
                t.program = if p < self.wd.instruments.len() { p } else { 0 };
            }
            0x22 => {
                let v = t.byte() as i8 as i32;
                t.volume.set(0, v, 16);
            }
            0x23 => {
                let time = t.varlen() as i32;
                let v = t.byte() as i8 as i32;
                t.volume.set(time, v, 16);
            }
            0x24 => {
                let v = t.byte() as i32;
                t.expression.set(0, v, 16);
            }
            0x25 => {
                let time = t.varlen() as i32;
                let v = t.byte() as i32;
                t.expression.set(time, v, 16);
            }
            0x26 => {
                let v = t.byte() as i32;
                t.pan.set(0, v, 16);
            }
            0x27 => {
                let time = t.varlen() as i32;
                let v = t.byte() as i32;
                t.pan.set(time, v, 16);
            }
            0x28 => {
                t.porta_time = t.varlen() as i32;
                t.porta = true;
            }
            0x29 => {
                t.glide.glide(0, 0);
                t.porta = false;
            }
            0x2c => {
                let time = t.varlen() as i32;
                let d = t.byte() as i8 as i32;
                t.bend.relative(time, d);
            }
            0x3c => t.sustain = t.byte() != 0,
            0x40 => {
                let (d, r, w) = (t.byte() as i32, t.byte() as i32, t.byte() as i32);
                t.vibrato.set(true, d, if r == 0 { 0x100 } else { r }, w, -1, -1, -1);
            }
            0x41 => t.vibrato.set(false, 0, 0, 0, 0, 1, 0),
            0x42 => {
                let d = t.byte() as i32;
                t.vibrato.set(true, d, -1, -1, -1, -1, -1);
            }
            0x43 => {
                let time = t.varlen() as i32;
                let d = t.byte() as i32;
                t.vibrato.depth.set(time, d, 0);
            }
            0x44 => {
                let r = t.byte() as i32;
                t.vibrato.set(true, -1, if r == 0 { 0x100 } else { r }, -1, -1, -1, -1);
            }
            0x45 => {
                let time = t.varlen() as i32;
                let r = t.byte() as i32;
                t.vibrato.rate.set(time, if r == 0 { 0x100 } else { r }, 16);
            }
            0x46 => {
                let w = t.byte() as i32;
                t.vibrato.set(true, -1, -1, w, -1, -1, -1);
            }
            0x47 => {
                let (dl, f) = (t.byte() as i32, t.byte() as i32);
                t.vibrato.set(true, -1, -1, -1, dl, f, -1);
            }
            0x48..=0x56 => {
                let lfo = if cmd < 0x50 { &mut t.tremolo } else { &mut t.autopan };
                match cmd & 7 {
                    0 => {
                        let (d, r, w) = (t.data.get(t.pos).copied().unwrap_or(0) as i32, t.data.get(t.pos + 1).copied().unwrap_or(0) as i32, t.data.get(t.pos + 2).copied().unwrap_or(0) as i32);
                        t.pos += 3;
                        let lfo = if cmd < 0x50 { &mut t.tremolo } else { &mut t.autopan };
                        lfo.set(true, d, if r == 0 { 0x100 } else { r }, w, -1, -1, -1);
                    }
                    1 => lfo.set(false, 0, 0, 0, 0, 1, 0),
                    2 => {
                        let d = t.byte() as i32;
                        let lfo = if cmd < 0x50 { &mut t.tremolo } else { &mut t.autopan };
                        lfo.set(true, d, -1, -1, -1, -1, -1);
                    }
                    3 => {
                        let time = t.varlen() as i32;
                        let d = t.byte() as i32;
                        let lfo = if cmd < 0x50 { &mut t.tremolo } else { &mut t.autopan };
                        lfo.depth.set(time, d, 0);
                    }
                    4 => {
                        let r = t.byte() as i32;
                        let lfo = if cmd < 0x50 { &mut t.tremolo } else { &mut t.autopan };
                        lfo.set(true, -1, if r == 0 { 0x100 } else { r }, -1, -1, -1, -1);
                    }
                    5 => {
                        let time = t.varlen() as i32;
                        let r = t.byte() as i32;
                        let lfo = if cmd < 0x50 { &mut t.tremolo } else { &mut t.autopan };
                        lfo.rate.set(time, if r == 0 { 0x100 } else { r }, 16);
                    }
                    6 => {
                        let w = t.byte() as i32;
                        let lfo = if cmd < 0x50 { &mut t.tremolo } else { &mut t.autopan };
                        lfo.set(true, -1, -1, w, -1, -1, -1);
                    }
                    _ => {
                        let (dl, f) = (t.byte() as i32, t.byte() as i32);
                        let lfo = if cmd < 0x50 { &mut t.tremolo } else { &mut t.autopan };
                        lfo.set(true, -1, -1, -1, dl, f, -1);
                    }
                }
            }
            0x72 => {
                let i = (t.byte() as usize).min(2);
                t.extra[i].set(false, 0, 0, 0, 0, 1, 0);
            }
            0x73 => {
                let i = (t.byte() as usize).min(2);
                let (dl, f) = (t.byte() as i32, t.byte() as i32);
                t.extra[i].set(true, -1, -1, -1, dl, f, -1);
            }
            0x74 => {
                let i = (t.byte() as usize).min(2);
                let (tg, w) = (t.byte() as i32, t.byte() as i32);
                t.extra[i].set(true, -1, -1, w, -1, -1, tg);
            }
            0x75 => {
                let i = (t.byte() as usize).min(2);
                let (d, r) = (t.byte() as i32, t.byte() as i32);
                t.extra[i].set(true, d, if r == 0 { 0x100 } else { r }, -1, -1, -1, -1);
            }
            0x76 => {
                let i = (t.byte() as usize).min(2);
                let time = t.varlen() as i32;
                let d = t.byte() as i32;
                t.extra[i].depth.set(time, d, 0);
            }
            0x77 => {
                let i = (t.byte() as usize).min(2);
                let time = t.varlen() as i32;
                let r = t.byte() as i32;
                t.extra[i].rate.set(time, if r == 0 { 0x100 } else { r }, 16);
            }
            // Per-track random ranges (handlers 0x10a66c / 0x10a6e0 in LIBSSL): 0x78 sets slot
            // `index` to `value` (0 reads as 256), 0x79 clears it. Slots 0, 1, 2 are the random
            // pitch, volume and pan ranges applied at every note on. Slot 3 (random delay time),
            // 4 (random attack rate) and 5 (random program) are read and ignored here.
            0x78 => {
                let i = t.byte();
                let v = match t.byte() {
                    0 => 256,
                    v => v as i32,
                };
                match i {
                    0 => t.rand_pitch = v,
                    1 => t.rand_vol = v,
                    2 => t.rand_pan = v,
                    _ => {}
                }
            }
            0x79 => match t.byte() {
                0 => {
                    t.rand_pitch = 0;
                    t.pitch_add = 0;
                }
                1 => {
                    t.rand_vol = 0;
                    t.vol_factor = 0x3fff;
                }
                2 => {
                    t.rand_pan = 0;
                    t.pan_add = 0;
                }
                _ => {}
            },
            0x7c | 0x7d => {
                // Jumps into other sound effects: treated as the end of this track.
                t.byte();
                t.done = true;
                let tag = self.tag + ti as u32;
                mixer.release(tag, None);
            }
            c => {
                for _ in 0..arg_len(c) {
                    t.byte();
                }
            }
        }
    }
}

/// Argument bytes per command for commands without a dedicated handler above (fixed lengths as
/// the driver's handlers consume them; varlen-taking ones are all handled explicitly).
fn arg_len(cmd: u8) -> usize {
    match cmd {
        0x09 | 0x0b | 0x0c | 0x39 | 0x62 => 2,
        0x0a | 0x0d | 0x0e | 0x14 | 0x15 | 0x16 | 0x31..=0x38 | 0x3e | 0x58..=0x5d | 0x66 | 0x67 | 0x6a | 0x6b | 0x6c | 0x71 => 1,
        _ => 0,
    }
}

/// An SE block (`"SeBlock"`): u16 bank id at 8, u16 count at 0xA, offsets from 0x10 (relative to the
/// end of the table). Each entry: u8 tracks, u16 WD bank, u8 group, u8 flags, u16 size per track
/// (bit 15: shares the previous track's voice), then the tracks.
#[derive(Clone, Debug)]
pub struct SeBlock {
    pub bank: u16,
    pub entries: Vec<SeEntry>,
}

#[derive(Clone, Debug)]
pub struct SeEntry {
    pub wd: u16,
    pub group: u8,
    pub flags: u8,
    pub tracks: Vec<Arc<[u8]>>,
}

impl SeBlock {
    pub fn parse(b: &[u8]) -> Option<SeBlock> {
        if b.get(..7) != Some(b"SeBlock") {
            return None;
        }
        let bank = u16::from_le_bytes([b[8], b[9]]);
        let count = u16::from_le_bytes([b[10], b[11]]) as usize;
        let base = 0x10 + count * 4;
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let off = u32::from_le_bytes(b.get(0x10 + 4 * i..0x14 + 4 * i)?.try_into().ok()?);
            if off == 0xffff_ffff {
                entries.push(SeEntry { wd: 0, group: 0, flags: 0, tracks: Vec::new() });
                continue;
            }
            let e = base + off as usize;
            let Some(h) = b.get(e..e + 5) else {
                entries.push(SeEntry { wd: 0, group: 0, flags: 0, tracks: Vec::new() });
                continue;
            };
            let n = h[0] as usize;
            let mut p = e + 5 + 2 * n;
            let mut tracks = Vec::with_capacity(n);
            for k in 0..n {
                let size = (u16::from_le_bytes([b[e + 5 + 2 * k], b[e + 6 + 2 * k]]) & 0x7fff) as usize;
                tracks.push(Arc::from(b.get(p..(p + size).min(b.len())).unwrap_or(&[])));
                p += size;
            }
            entries.push(SeEntry { wd: u16::from_le_bytes([h[1], h[2]]), group: h[3], flags: h[4], tracks });
        }
        Some(SeBlock { bank, entries })
    }
}

/// The game's full BGM slot volume: every start and fade-in of the area music goes to 0x3000
/// (FUN_00136ec0, FUN_00137458, FUN_001374c0), half is 0x1800 (the event scripts' 0x101).
pub const BGM_FULL: i32 = 0x3000;

/// Mixer tag bases of the two BGM slots (64 tags each, one per track); sound effects and voice
/// lines take the blocks from 64 up to 60,000.
const BGM_TAG: [u32; 2] = [0, 61_440];

/// A BGM slot of the driver: the IOP module's sequence struct for slot 0 (field) or 1 (battle),
/// with its volume ramp (+0x44 volume, +0x46 target, +0x48 frames left, +0x4a step per frame,
/// +0x68 delay, +0x6a delay counter). Read in the disassembly of the user's `LIBSSL.IRX`:
/// command 0xf (`FUN_103cbc`) starts the sequence with a ramp from a start volume to a target
/// over `time` frames after `delay` frames, command 0x11 (`FUN_104964`) ramps the playing
/// sequence to a target over `time` frames. The ramp runs in the 240 Hz driver frame
/// (`FUN_1081ac` from the main loop at 0x101908), before the slot's sequencer tick, which does
/// not run while the delay counts: `volume += step; time -= 1`, and at 0 `volume = target`; a
/// target of 0 stops the sequence (`FUN_108900(slot, 1)`). The step is the integer quotient
/// `(target - volume) / time` (a `time` of 0 or less counts as 1), so a long fade-out ends on a
/// small positive remainder before the final snap to 0.
#[derive(Default)]
pub struct BgmSlot {
    pub seq: Option<Sequence>,
    /// A sequence stopped by a fade to 0, kept for a "resume" start. The game restores the state
    /// its sequencer last checkpointed (0x1e35c0, written at 0x108140 of routine 0x108068 on a
    /// counter event that was not traced); the port resumes at the exact stop point.
    saved: Option<Sequence>,
    pub volume: i32,
    target: i32,
    time: i32,
    step: i32,
    delay: i32,
    counter: i32,
}

impl BgmSlot {
    fn set_fade(&mut self, target: i32, time: i32) {
        let time = time.max(1);
        self.step = (target - self.volume) / time;
        self.target = target;
        self.time = time;
    }

    /// The ramp of one driver frame; true when the sequencer may run this frame.
    fn frame(&mut self, mixer: &mut Mixer) -> bool {
        if self.counter < self.delay {
            self.counter += 1;
            return false;
        }
        if self.time != 0 {
            self.volume += self.step;
            self.time -= 1;
            if self.time <= 0 {
                self.volume = self.target;
                self.step = 0;
                self.time = 0;
                if self.target == 0 {
                    self.stop(mixer, true);
                }
            }
        }
        true
    }

    /// `FUN_108900(slot, save)`: keys the slot's notes off and drops the sequence, keeping it for
    /// a resume when `save`.
    fn stop(&mut self, mixer: &mut Mixer, save: bool) {
        if let Some(mut s) = self.seq.take() {
            s.release_all(mixer);
            if save {
                self.saved = Some(s);
            }
        }
    }

    pub fn playing(&self) -> bool {
        self.seq.is_some()
    }
}

/// A playing sound effect or voice line, for later volume and pan updates (the game sends
/// command 0x15 every tick for a positional sound).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handle(pub u32);

/// The whole driver: two BGM slots, sound effects, voices.
pub struct Driver {
    pub tables: Arc<Tables>,
    pub mixer: Mixer,
    pub bgm: [BgmSlot; 2],
    pub se: Vec<Sequence>,
    rng: u32,
    frame_acc: u32,
    next_tag: u32,
}

impl Driver {
    pub fn new(tables: Arc<Tables>) -> Driver {
        Driver { tables, mixer: Mixer::new(), bgm: Default::default(), se: Vec::new(), rng: 0x1234_5678, frame_acc: 0, next_tag: 64 }
    }

    fn take_tag(&mut self) -> u32 {
        let tag = self.next_tag;
        self.next_tag = if self.next_tag > 60_000 { 64 } else { self.next_tag + 64 };
        tag
    }

    /// Plays `seq` in slot 0 at the full BGM level at once (the area start of FUN_00136ec0:
    /// command 0xf from 0x3000 to 0x3000 in no time).
    pub fn play_bgm(&mut self, seq: Sequence) {
        self.bgm_start(0, Some(seq), BGM_FULL, BGM_FULL, 0, false, 0);
    }

    /// Command 0xf: starts a BGM in `slot` with a ramp from `from` to `to` over `time` frames
    /// after `delay` frames. With `resume`, a sequence the slot kept from a fade-out is restarted
    /// where it stopped instead of `seq` (the game's mode 1 restores its checkpoint); `seq` must
    /// be given otherwise. The slot's playing sequence is dropped first, without keeping it.
    #[allow(clippy::too_many_arguments)]
    pub fn bgm_start(&mut self, slot: usize, seq: Option<Sequence>, from: i32, to: i32, time: i32, resume: bool, delay: i32) -> bool {
        let Some(s) = self.bgm.get_mut(slot) else { return false };
        if resume && s.saved.is_none() && s.seq.is_some() {
            // Not yet faded out: the sequence is still the one to continue (inferred; the game's
            // checkpoint restore was not traced).
            s.volume = from;
            s.set_fade(to, time);
            s.delay = delay;
            s.counter = 0;
            return true;
        }
        s.stop(&mut self.mixer, false);
        let mut next = match (resume, s.saved.take(), seq) {
            (true, Some(kept), _) => kept,
            (_, _, Some(mut fresh)) => {
                fresh.tag = BGM_TAG[slot];
                fresh
            }
            _ => return false,
        };
        next.tag = BGM_TAG[slot];
        next.finished = false;
        s.volume = from;
        s.set_fade(to, time);
        s.delay = delay;
        s.counter = 0;
        s.seq = Some(next);
        true
    }

    /// Command 0x11: ramps the playing BGM of `slot` to `target` over `time` frames; a target of
    /// 0 stops it at the end, keeping it for a resume. Nothing when the slot is silent.
    pub fn bgm_fade(&mut self, slot: usize, target: i32, time: i32) {
        if let Some(s) = self.bgm.get_mut(slot)
            && s.seq.is_some()
        {
            s.set_fade(target, time);
        }
    }

    /// Stops the BGM of `slot` now.
    pub fn bgm_stop(&mut self, slot: usize, save: bool) {
        if let Some(s) = self.bgm.get_mut(slot) {
            s.stop(&mut self.mixer, save);
        }
    }

    /// Starts sound effect `index` of `block` with volume (0..0x3fff) and pan (0..0x7f).
    pub fn play_se(&mut self, block: &SeBlock, index: usize, wd: Arc<Wd>, volume: i32, pan: i32) -> bool {
        self.play_se_handle(block, index, wd, volume, pan).is_some()
    }

    /// `play_se` with a handle for later `set_se_mix` calls.
    pub fn play_se_handle(&mut self, block: &SeBlock, index: usize, wd: Arc<Wd>, volume: i32, pan: i32) -> Option<Handle> {
        let e = block.entries.get(index)?;
        if e.tracks.is_empty() {
            return None;
        }
        let vol = if e.flags & 0x10 != 0 { 0x3fff } else { volume };
        let pan = if e.flags & 0x20 != 0 { 0x40 } else { pan };
        let tag = self.take_tag();
        self.se.push(Sequence::new(e.tracks.clone(), wd, vol, pan, tag));
        if self.se.len() > 24 {
            self.se.remove(0);
        }
        Some(Handle(tag))
    }

    /// Command 0x15 for a sound effect: new volume and pan, applied by the next driver frame.
    pub fn set_se_mix(&mut self, h: Handle, volume: i32, pan: i32) {
        if let Some(s) = self.se.iter_mut().find(|s| s.tag == h.0) {
            s.volume = volume;
            s.pan = pan;
        }
    }

    pub fn se_playing(&self, h: Handle) -> bool {
        self.se.iter().any(|s| s.tag == h.0)
    }

    /// Stops a sound effect now: its sounding notes are keyed off (the release of their envelopes
    /// runs out in the mixer) and the sequence player is dropped. Nothing for a finished handle.
    pub fn stop_se(&mut self, h: Handle) {
        if let Some(i) = self.se.iter().position(|s| s.tag == h.0) {
            let mut s = self.se.remove(i);
            s.release_all(&mut self.mixer);
        }
    }

    /// Plays a one-shot sample (a voice line) at `rate` Hz, centred.
    pub fn play_voice(&mut self, sample: Arc<Sample>, rate: u32, volume: f32) {
        let tag = self.take_tag();
        let step = rate as f64 / crate::synth::RATE as f64;
        self.mixer.play(Voice::new(sample, step, Envelope::new(0x000f, 0x0000), volume, 0.0, tag, 0));
    }

    /// Plays a voice line at `rate` Hz with the driver's volume (0..0x3fff) and pan circle
    /// position, through the pan law tables like a sequence note.
    pub fn play_voice_at(&mut self, sample: Arc<Sample>, rate: u32, volume: i32, pan: i32) -> Handle {
        let tag = self.take_tag();
        let step = rate as f64 / crate::synth::RATE as f64;
        let mut v = Voice::new(sample, step, Envelope::new(0x000f, 0x0000), 0.0, 0.0, tag, 0);
        let (l, r) = self.law(volume, pan);
        v.set_lr(l, r);
        self.mixer.play(v);
        Handle(tag)
    }

    /// Command 0x15 for a voice line.
    pub fn set_voice_mix(&mut self, h: Handle, volume: i32, pan: i32) {
        let (l, r) = self.law(volume, pan);
        if let Some(v) = self.mixer.voices.iter_mut().find(|v| v.tag == h.0) {
            v.set_lr(l, r);
        }
    }

    pub fn voice_playing(&self, h: Handle) -> bool {
        self.mixer.voices.iter().any(|v| v.tag == h.0 && !v.env.off())
    }

    pub fn stop_voice(&mut self, h: Handle) {
        self.mixer.release(h.0, None);
    }

    /// Left and right gains of a volume on the pan circle (the law tables; a position past 0xff
    /// inverts the phase, as for sequence notes).
    fn law(&self, volume: i32, pan: i32) -> (f32, f32) {
        let vol = volume.clamp(0, 0x3fff);
        let pan = (pan & 0x1ff) as usize;
        let (l, r) = if pan < 0x100 {
            ((vol * self.tables.pan_l[pan] as i32) >> 14, (vol * self.tables.pan_r[pan] as i32) >> 14)
        } else {
            ((vol * -(self.tables.pan_l[pan & 0xff] as i32)) >> 14, (vol * -(self.tables.pan_r[pan & 0xff] as i32)) >> 14)
        };
        let spu = |v: i32| {
            let v = v & 0x7fff;
            (if v & 0x4000 != 0 { v - 0x8000 } else { v }) as f32 / 16384.0
        };
        (spu(l), spu(r))
    }

    /// Renders `out.len() / 2` stereo frames at 48 kHz, running driver frames every 200 samples.
    pub fn render(&mut self, out: &mut [f32]) {
        const PER_FRAME: u32 = crate::synth::RATE / FRAME_HZ;
        let mut done = 0;
        let frames = out.len() / 2;
        while done < frames {
            if self.frame_acc == 0 {
                for slot in &mut self.bgm {
                    if slot.frame(&mut self.mixer)
                        && let Some(b) = slot.seq.as_mut()
                    {
                        b.slot_volume = slot.volume.clamp(0, 0x3fff);
                        b.frame(&mut self.mixer, &self.tables, &mut self.rng);
                    }
                }
                for s in &mut self.se {
                    s.frame(&mut self.mixer, &self.tables, &mut self.rng);
                }
                self.se.retain(|s| !s.finished);
                self.frame_acc = PER_FRAME;
            }
            let n = (self.frame_acc as usize).min(frames - done);
            self.mixer.render(&mut out[done * 2..(done + n) * 2]);
            done += n;
            self.frame_acc -= n as u32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wd::{Instrument, Region};

    fn one_note_wd() -> Arc<Wd> {
        let region = Region { key_high: 127, velocity_high: 127, ..Region::default() };
        Arc::new(Wd { id: 0, instruments: vec![Instrument { regions: vec![region] }], samples: Vec::new() })
    }

    fn idle_seq() -> Sequence {
        // One track that waits for a very long time and plays nothing.
        Sequence::new(vec![Arc::from(vec![0x8f, 0x7f])], one_note_wd(), BGM_FULL, 0x40, 0)
    }

    fn driver() -> Driver {
        let tables = Tables { semitone: [0; 12], fine: [0; 256], pan_l: [0; 256], pan_r: [0; 256] };
        Driver::new(Arc::new(tables))
    }

    /// The slot volume ramps linearly with the integer step, waits out its delay, and a fade to 0
    /// stops the sequence and keeps it for a resume (IOP commands 0xf and 0x11).
    #[test]
    fn bgm_slot_ramps_delays_and_resumes() {
        let mut d = driver();
        assert!(d.bgm_start(1, Some(idle_seq()), 0, 0x3000, 240, false, 240));
        let mut out = vec![0.0f32; 2 * 200];
        for _ in 0..240 {
            d.render(&mut out);
        }
        // Delay still counting: volume untouched.
        assert_eq!(d.bgm[1].volume, 0);
        for _ in 0..121 {
            d.render(&mut out);
        }
        assert_eq!(d.bgm[1].volume, 0x3000 / 240 * 121);
        assert!(d.bgm[1].volume > 0 && d.bgm[1].volume < 0x3000);
        for _ in 0..200 {
            d.render(&mut out);
        }
        assert_eq!(d.bgm[1].volume, 0x3000);
        d.bgm_fade(1, 0, 100);
        for _ in 0..101 {
            d.render(&mut out);
        }
        assert!(!d.bgm[1].playing());
        assert!(d.bgm[1].saved.is_some());
        // Resume without a fresh sequence.
        assert!(d.bgm_start(1, None, 0, 0x3000, 10, true, 0));
        assert!(d.bgm[1].playing());
    }

    /// Command 0x78 takes (index, value) and 0x79 takes (index): a sequence that sets the random
    /// volume range, plays a note and clears the range must keep its tempo (the old reading took
    /// the arguments as commands, one of which was a tempo change to 0) and finish.
    #[test]
    fn random_range_commands_consume_their_arguments() {
        let track: Arc<[u8]> = Arc::from(vec![0x00, 0x78, 0x01, 0x14, 0x00, 0x12, 0x3d, 0x10, 0x79, 0x01, 0x00, 0x00]);
        let mut seq = Sequence::new(vec![track], one_note_wd(), 0x3fff, 0x40, 64);
        let (mut mixer, mut rng) = (Mixer::new(), 0x1234_5678u32);
        let mut seen = None;
        for _ in 0..4 {
            seq.track_tick(0, &mut mixer, &mut rng);
            if seq.tracks[0].rand_vol != 0 {
                seen = Some(seq.tracks[0].rand_vol);
            }
        }
        assert_eq!(seq.tempo, 0x78);
        assert_eq!(seen, Some(0x14));
        // The note drew a volume factor inside the range (0x3fff minus up to 0x14 * 0x40 - 1).
        assert!((0x3fff - 0x4ff..=0x3fff).contains(&seq.tracks[0].vol_factor) || seq.tracks[0].vol_factor == 0x3fff);
        for _ in 0..40 {
            seq.track_tick(0, &mut mixer, &mut rng);
        }
        assert!(seq.tracks[0].done);
        assert_eq!(seq.tracks[0].rand_vol, 0);
        assert_eq!(seq.tracks[0].vol_factor, 0x3fff);
    }

    /// Command 0x39 takes two argument bytes (found in the boot bank's entries 14 and 17: read with one
    /// argument the track ends before its notes and the sound, such as the prize bounce, was silent).
    #[test]
    fn command_39_consumes_two_arguments() {
        assert_eq!(arg_len(0x39), 2);
        let track: Arc<[u8]> = Arc::from(vec![0x00, 0x39, 0x07, 0x00, 0x00, 0x12, 0x3d, 0x05, 0x00]);
        let mut seq = Sequence::new(vec![track], one_note_wd(), 0x3fff, 0x40, 64);
        let (mut mixer, mut rng) = (Mixer::new(), 1u32);
        seq.track_tick(0, &mut mixer, &mut rng);
        assert_eq!(mixer.voices.len(), 1, "the note after the command sounds");
    }
}
