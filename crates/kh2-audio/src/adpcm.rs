//! Sony SPU/SPU2 ADPCM ("VAG"): 16-byte blocks of 28 samples. Byte 0: shift (low nibble) and
//! filter (high nibble); byte 1: flags (bit 0 loop end, bit 1 repeat, bit 2 loop start); 14 bytes of
//! 4-bit deltas. The filter coefficients are part of the hardware definition (SPU documentation).

const FILTERS: [[i32; 2]; 5] = [[0, 0], [60, 0], [115, -52], [98, -55], [122, -60]];

pub const BLOCK: usize = 16;
pub const SAMPLES_PER_BLOCK: usize = 28;

pub mod flag {
    pub const END: u8 = 1;
    pub const REPEAT: u8 = 2;
    pub const LOOP_START: u8 = 4;
}

/// Decodes one block. `hist` carries the two previous samples between blocks.
pub fn decode_block(b: &[u8], hist: &mut [i32; 2], out: &mut [i16; SAMPLES_PER_BLOCK]) {
    let shift = (b[0] & 0xf) as i32;
    let filter = ((b[0] >> 4) as usize).min(4);
    let [f0, f1] = FILTERS[filter];
    for i in 0..SAMPLES_PER_BLOCK {
        let byte = b[2 + i / 2];
        let nib = if i & 1 == 0 { byte & 0xf } else { byte >> 4 };
        let s = (((nib as i32) << 28) >> 16) >> shift.min(12);
        let s = s + ((hist[0] * f0 + hist[1] * f1 + 32) >> 6);
        let s = s.clamp(-32768, 32767);
        hist[1] = hist[0];
        hist[0] = s;
        out[i] = s as i16;
    }
}

/// A decoded sample with its loop point (in samples), if the data loops.
#[derive(Clone, Debug, Default)]
pub struct Sample {
    pub pcm: Vec<i16>,
    pub loop_start: Option<usize>,
}

/// Decodes from `data[offset..]` until a block with the end flag (or the end of the data).
pub fn decode(data: &[u8], offset: usize) -> Sample {
    let mut hist = [0i32; 2];
    let mut pcm = Vec::new();
    let mut loop_start = None;
    let mut o = offset;
    let mut buf = [0i16; SAMPLES_PER_BLOCK];
    // The first block of a sound is often all zero with no flags; decode it like any other.
    while o + BLOCK <= data.len() {
        let b = &data[o..o + BLOCK];
        let flags = b[1];
        if flags & flag::LOOP_START != 0 && loop_start.is_none() {
            loop_start = Some(pcm.len());
        }
        decode_block(b, &mut hist, &mut buf);
        pcm.extend_from_slice(&buf);
        o += BLOCK;
        if flags & flag::END != 0 {
            if flags & flag::REPEAT == 0 {
                loop_start = None;
            }
            break;
        }
        if pcm.len() > 48000 * 120 {
            break;
        }
    }
    Sample { pcm, loop_start }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_block_and_end_flag() {
        let mut d = vec![0u8; 32];
        d[16 + 1] = flag::END;
        let s = decode(&d, 0);
        assert_eq!(s.pcm.len(), 56);
        assert!(s.pcm.iter().all(|&x| x == 0));
        assert!(s.loop_start.is_none());
    }

    #[test]
    fn filter_one_integrates() {
        // Shift 12 keeps the raw nibble; filter 1 adds 60/64 of the previous sample.
        let mut b = [0u8; 16];
        b[0] = 0x1c;
        b[2] = 0x07;
        let mut h = [0; 2];
        let mut out = [0i16; 28];
        decode_block(&b, &mut h, &mut out);
        assert_eq!(out[0], 7);
        assert_eq!(out[1], (7 * 60 + 32) >> 6);
    }
}
