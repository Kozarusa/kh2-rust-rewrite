//! A software SPU2-style voice mixer at 48 kHz: ADPCM samples with loop points, pitch, the SPU
//! ADSR envelope (rates and exponential modes as the SPU hardware defines them), volume and pan.

use std::sync::Arc;

use crate::adpcm::Sample;

pub const RATE: u32 = 48_000;

/// The SPU envelope generator: attack, decay, sustain, release from the ADSR1/ADSR2 registers.
#[derive(Clone, Copy, Debug)]
pub struct Envelope {
    adsr1: u16,
    adsr2: u16,
    phase: Phase,
    /// 0..0x7fff
    level: i32,
    counter: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Attack,
    Decay,
    Sustain,
    Release,
    Off,
}

impl Envelope {
    pub fn new(adsr1: u16, adsr2: u16) -> Envelope {
        Envelope { adsr1, adsr2, phase: Phase::Attack, level: 0, counter: 0 }
    }

    pub fn release(&mut self) {
        if self.phase != Phase::Off {
            self.phase = Phase::Release;
            self.counter = 0;
        }
    }

    pub fn off(&self) -> bool {
        self.phase == Phase::Off
    }

    pub fn level(&self) -> f32 {
        self.level as f32 / 32767.0
    }

    /// One envelope step with rate `rate` (0..127), exponential or linear, increasing or not.
    fn step(&mut self, rate: i32, exp: bool, decrease: bool) {
        let shift = (11 - (rate >> 2)).max(0);
        let mut step = if decrease { -8 + (rate & 3) } else { 7 - (rate & 3) } << shift;
        let mut cycles = 1 << ((rate >> 2) - 11).max(0);
        if exp {
            if decrease {
                step = (step * self.level) >> 15;
            } else if self.level > 0x6000 {
                cycles *= 4;
            }
        }
        self.counter += 1;
        if self.counter >= cycles {
            self.counter = 0;
            self.level = (self.level + step).clamp(0, 0x7fff);
        }
    }

    pub fn tick(&mut self) {
        let a1 = self.adsr1 as i32;
        let a2 = self.adsr2 as i32;
        match self.phase {
            Phase::Attack => {
                self.step((a1 >> 8) & 0x7f, a1 & 0x8000 != 0, false);
                if self.level >= 0x7fff {
                    self.phase = Phase::Decay;
                    self.counter = 0;
                }
            }
            Phase::Decay => {
                self.step(((a1 >> 4) & 0xf) << 2, true, true);
                let sustain = (((a1 & 0xf) + 1) * 0x800).min(0x7fff);
                if self.level <= sustain {
                    self.phase = Phase::Sustain;
                    self.counter = 0;
                }
            }
            Phase::Sustain => self.step((a2 >> 6) & 0x7f, a2 & 0x8000 != 0, a2 & 0x4000 != 0),
            Phase::Release => {
                self.step((a2 & 0x1f) << 2, a2 & 0x20 != 0, true);
                if self.level == 0 {
                    self.phase = Phase::Off;
                }
            }
            Phase::Off => {}
        }
    }
}

/// One playing voice.
pub struct Voice {
    pub sample: Arc<Sample>,
    /// Position in samples (fractional).
    pos: f64,
    /// Samples advanced per output sample.
    pub step: f64,
    pub env: Envelope,
    pub left: f32,
    pub right: f32,
    /// Owner tag (channel/track) so a sequencer can release its notes.
    pub tag: u32,
    pub key: u8,
    /// Index of the WD region this voice plays (for the sequencer's per-frame updates).
    pub region: usize,
    /// −1 left half / 1 right half of a stereo pair, 0 mono.
    pub side: i8,
    pub velocity: u8,
}

impl Voice {
    pub fn new(sample: Arc<Sample>, step: f64, env: Envelope, volume: f32, pan: f32, tag: u32, key: u8) -> Voice {
        let p = pan.clamp(-1.0, 1.0);
        let left = volume * (1.0 - p).min(1.0);
        let right = volume * (1.0 + p).min(1.0);
        Voice { sample, pos: 0.0, step, env, left, right, tag, key, region: 0, side: 0, velocity: 0x7f }
    }

    pub fn set_mix(&mut self, volume: f32, pan: f32) {
        let p = pan.clamp(-1.0, 1.0);
        self.left = volume * (1.0 - p).min(1.0);
        self.right = volume * (1.0 + p).min(1.0);
    }

    /// Sets the channel gains directly (0..1 each).
    pub fn set_lr(&mut self, l: f32, r: f32) {
        self.left = l;
        self.right = r;
    }

    /// Next stereo sample; None once the voice has finished.
    fn next(&mut self) -> Option<(f32, f32)> {
        if self.env.off() {
            return None;
        }
        let n = self.sample.pcm.len();
        if n == 0 {
            return None;
        }
        let i = self.pos as usize;
        if i >= n {
            match self.sample.loop_start {
                Some(l) if l < n => self.pos = l as f64 + (self.pos - n as f64) % ((n - l) as f64).max(1.0),
                _ => return None,
            }
        }
        let i = self.pos as usize;
        let frac = (self.pos - i as f64) as f32;
        let a = self.sample.pcm[i] as f32;
        let j = if i + 1 < n { i + 1 } else { self.sample.loop_start.unwrap_or(i) };
        let b = self.sample.pcm.get(j).copied().unwrap_or(0) as f32;
        let s = (a + (b - a) * frac) / 32768.0;
        self.pos += self.step;
        self.env.tick();
        let e = self.env.level();
        Some((s * e * self.left, s * e * self.right))
    }
}

/// Voice pool and mixer.
#[derive(Default)]
pub struct Mixer {
    pub voices: Vec<Voice>,
    pub master: f32,
}

impl Mixer {
    pub fn new() -> Mixer {
        Mixer { voices: Vec::new(), master: 1.0 }
    }

    pub fn play(&mut self, v: Voice) {
        if self.voices.len() >= 48 {
            // The SPU2 has 48 voices; drop the quietest.
            if let Some((i, _)) = self.voices.iter().enumerate().min_by(|a, b| a.1.env.level().total_cmp(&b.1.env.level())) {
                self.voices.swap_remove(i);
            }
        }
        self.voices.push(v);
    }

    pub fn release(&mut self, tag: u32, key: Option<u8>) {
        for v in &mut self.voices {
            if v.tag == tag && key.is_none_or(|k| v.key == k) {
                v.env.release();
            }
        }
    }

    /// Mixes `out.len() / 2` stereo frames.
    pub fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        for f in out.as_chunks_mut::<2>().0 {
            self.voices.retain_mut(|v| match v.next() {
                Some((l, r)) => {
                    f[0] += l;
                    f[1] += r;
                    true
                }
                None => false,
            });
        }
        for s in out.iter_mut() {
            *s = (*s * self.master).clamp(-1.0, 1.0);
        }
    }
}

/// Output-rate step for a note: the sample plays at `base_rate` on its unity key.
pub fn pitch_step(key: i32, unity: i32, cents: f32, base_rate: f64) -> f64 {
    let semis = (key - unity) as f64 + cents as f64 / 100.0;
    base_rate / RATE as f64 * 2f64.powf(semis / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_attacks_and_releases() {
        // Fast linear attack (rate 0), no decay to speak of, sustain level 15, fast release.
        let mut e = Envelope::new(0x000f, 0x0000);
        for _ in 0..200 {
            e.tick();
        }
        assert!(e.level() > 0.99);
        e.release();
        for _ in 0..100_000 {
            e.tick();
        }
        assert!(e.off());
    }

    #[test]
    fn unity_key_plays_at_base_rate() {
        assert!((pitch_step(60, 60, 0.0, 48_000.0) - 1.0).abs() < 1e-9);
        assert!((pitch_step(72, 60, 0.0, 48_000.0) - 2.0).abs() < 1e-9);
    }
}
