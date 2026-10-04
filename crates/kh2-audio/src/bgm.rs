//! BGM: Square's PS2 sequence format. Header `"BGM "`, u16 sequence id, u16 WD bank id, u8 track
//! count, volume at 0xC, PPQN at 0xE; tracks from 0x20, each a u32 size and MIDI-like events
//! (variable-length delta time, then a command byte and its arguments).
//!
//! Layout per OpenKH `docs/kh2/file/type/bgm.md` (Apache-2.0) and VGMTrans
//! `src/main/formats/SquarePS2/SquarePS2Seq.cpp` (zlib); see CREDITS.md.

use std::collections::HashMap;
use std::sync::Arc;

use crate::adpcm::Sample;
use crate::synth::{Envelope, Mixer, RATE, Voice, pitch_step};
use crate::wd::Wd;

#[derive(Clone, Debug)]
pub struct Bgm {
    pub id: u16,
    pub wd: u16,
    pub volume: i8,
    pub ppqn: u16,
    pub tracks: Vec<Vec<u8>>,
}

impl Bgm {
    pub fn parse(b: &[u8]) -> Option<Bgm> {
        if b.get(..4) != Some(b"BGM ") {
            return None;
        }
        let n = b[8] as usize;
        let mut tracks = Vec::with_capacity(n);
        let mut p = 0x20;
        for _ in 0..n {
            let size = u32::from_le_bytes(b.get(p..p + 4)?.try_into().ok()?) as usize;
            tracks.push(b.get(p + 4..(p + 4 + size).min(b.len()))?.to_vec());
            p += 4 + size;
        }
        Some(Bgm {
            id: u16::from_le_bytes([b[4], b[5]]),
            wd: u16::from_le_bytes([b[6], b[7]]),
            volume: b[0xc] as i8,
            ppqn: u16::from_le_bytes([b[0xe], b[0xf]]).max(1),
            tracks,
        })
    }
}

/// Argument byte count per command (after the command byte); None ends the track.
pub fn arg_len(cmd: u8) -> Option<usize> {
    Some(match cmd {
        0x00 => return None,
        0x02 | 0x03 | 0x04 | 0x10 | 0x18 | 0x60 | 0x61 | 0x7f => 0,
        0x08 | 0x0a | 0x0d | 0x12 | 0x13 | 0x1a | 0x20 | 0x22 | 0x24 | 0x26 | 0x28 | 0x31 | 0x34 | 0x35 | 0x3c | 0x3e
        | 0x58 | 0x5d => 1,
        0x0c | 0x11 | 0x19 | 0x47 | 0x5c => 2,
        0x40 | 0x48 | 0x50 => 3,
        _ => 0,
    })
}

struct Track {
    data: Vec<u8>,
    pos: usize,
    /// Ticks until the next event.
    wait: u32,
    loop_at: Option<usize>,
    done: bool,
    program: usize,
    volume: f32,
    expression: f32,
    pan: f32,
    key: u8,
    velocity: u8,
    bend: f32,
}

impl Track {
    fn read_delta(&mut self) -> u32 {
        let mut v = 0u32;
        for _ in 0..4 {
            let Some(&b) = self.data.get(self.pos) else { break };
            self.pos += 1;
            v = (v << 7) | (b & 0x7f) as u32;
            if b & 0x80 == 0 {
                break;
            }
        }
        v
    }

    fn byte(&mut self) -> u8 {
        let b = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        b
    }
}

/// Plays one BGM against its WD bank into a mixer.
pub struct Player {
    wd: Arc<Wd>,
    tracks: Vec<Track>,
    ppqn: u32,
    bpm: f64,
    /// Fractional ticks carried between frames.
    tick_acc: f64,
    samples: HashMap<usize, Arc<Sample>>,
    pub volume: f32,
    /// Voice tag base (tag = base + track index).
    tag: u32,
}

impl Player {
    pub fn new(bgm: &Bgm, wd: Arc<Wd>, tag: u32) -> Player {
        let tracks = bgm
            .tracks
            .iter()
            .map(|d| Track {
                data: d.clone(),
                pos: 0,
                wait: 0,
                loop_at: None,
                done: false,
                program: 0,
                volume: 1.0,
                expression: 1.0,
                pan: 0.0,
                key: 60,
                velocity: 100,
                bend: 0.0,
            })
            .collect();
        let mut p = Player {
            wd,
            tracks,
            ppqn: bgm.ppqn as u32,
            bpm: 120.0,
            tick_acc: 0.0,
            samples: HashMap::new(),
            volume: (bgm.volume as f32 / 127.0).clamp(0.0, 1.0).max(0.5),
            tag,
        };
        for t in &mut p.tracks {
            t.wait = t.read_delta();
        }
        p
    }

    pub fn finished(&self) -> bool {
        self.tracks.iter().all(|t| t.done)
    }

    fn sample(&mut self, r: &crate::wd::Region) -> Arc<Sample> {
        let wd = self.wd.clone();
        self.samples.entry(r.sample).or_insert_with(|| Arc::new(wd.sample(r))).clone()
    }

    /// Advances by `frames` output frames, running every event that falls due.
    pub fn advance(&mut self, mixer: &mut Mixer, frames: usize) {
        let ticks_per_frame = self.bpm * self.ppqn as f64 / 60.0 / RATE as f64;
        self.tick_acc += ticks_per_frame * frames as f64;
        while self.tick_acc >= 1.0 {
            self.tick_acc -= 1.0;
            self.tick(mixer);
        }
    }

    fn tick(&mut self, mixer: &mut Mixer) {
        for ti in 0..self.tracks.len() {
            let mut guard = 0;
            while !self.tracks[ti].done && self.tracks[ti].wait == 0 && guard < 256 {
                guard += 1;
                self.event(ti, mixer);
                if !self.tracks[ti].done {
                    let d = self.tracks[ti].read_delta();
                    self.tracks[ti].wait = d;
                }
            }
            if self.tracks[ti].wait > 0 {
                self.tracks[ti].wait -= 1;
            }
        }
    }

    fn note_on(&mut self, ti: usize, key: u8, vel: u8, mixer: &mut Mixer) {
        let tag = self.tag + ti as u32;
        let t = &self.tracks[ti];
        let Some(r) = self.wd.region(t.program, key).copied() else { return };
        let vol = self.volume * t.volume * t.expression * (vel as f32 / 127.0) * r.volume;
        let pan = (t.pan + r.pan).clamp(-1.0, 1.0);
        let bend = t.bend;
        let sample = self.sample(&r);
        let step = pitch_step(key as i32, r.unity_key, r.fine_cents + bend * 100.0, 44_100.0);
        mixer.play(Voice::new(sample, step, Envelope::new(r.adsr1, r.adsr2), vol, pan, tag, key));
    }

    fn event(&mut self, ti: usize, mixer: &mut Mixer) {
        let tag = self.tag + ti as u32;
        let cmd = self.tracks[ti].byte();
        let t = &mut self.tracks[ti];
        match cmd {
            0x00 => {
                t.done = true;
                mixer.release(tag, None);
            }
            0x02 => t.loop_at = Some(t.pos),
            0x03 => {
                if let Some(l) = t.loop_at {
                    t.pos = l;
                }
            }
            0x08 => {
                let bpm = t.byte();
                self.bpm = bpm.max(1) as f64;
            }
            0x10 => {
                let (k, v) = (t.key, t.velocity);
                self.note_on(ti, k, v, mixer);
            }
            0x11 => {
                let k = t.byte();
                let v = t.byte();
                t.key = k;
                t.velocity = v;
                self.note_on(ti, k, v, mixer);
            }
            0x12 => {
                let k = t.byte();
                t.key = k;
                let v = t.velocity;
                self.note_on(ti, k, v, mixer);
            }
            0x13 => {
                let v = t.byte();
                t.velocity = v;
                let k = t.key;
                self.note_on(ti, k, v, mixer);
            }
            0x18 => {
                let k = t.key;
                mixer.release(tag, Some(k));
            }
            0x1a => {
                let k = t.byte();
                t.key = k;
                mixer.release(tag, Some(k));
            }
            0x20 => t.program = t.byte() as usize,
            0x22 => t.volume = t.byte() as f32 / 127.0,
            0x24 => t.expression = t.byte() as f32 / 127.0,
            0x26 => t.pan = (t.byte() as f32 - 64.0) / 64.0,
            0x5c => {
                let lsb = t.byte() as i32;
                let msb = t.byte() as i32;
                // 14-bit bend around 0x2000, ±2 semitones.
                t.bend = (((msb << 7) | lsb) - 0x2000) as f32 / 8192.0 * 2.0;
            }
            c => {
                for _ in 0..arg_len(c).unwrap_or(0) {
                    t.byte();
                }
            }
        }
    }
}
