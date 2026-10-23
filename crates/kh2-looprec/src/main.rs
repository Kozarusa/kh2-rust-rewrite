//! Loopback recorder: records what the default output device plays (WASAPI loopback through cpal) into a 32-bit
//! float wav, for loudness comparisons between the original (PCSX2) and kh2-game (`tools/wavstats.py` reads the
//! result).
//!
//! Usage: `cargo run -p kh2-looprec --release -- <out.wav> [seconds, default 10]`
//!
//! WASAPI delivers no loopback samples while nothing plays, so a recording of silence is shorter than asked (or empty).
//! The recording is the user's own machine output: it stays in `local/` (git-ignores every `*.wav`).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::io::{self, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};

/// A wav writer for interleaved 32-bit float samples (format tag 3, IEEE float). The two size fields of the header
/// are patched by [`FloatWav::finish`].
struct FloatWav<W: Write + Seek> {
    out: W,
    samples: u32,
}

impl<W: Write + Seek> FloatWav<W> {
    fn new(mut out: W, channels: u16, rate: u32) -> io::Result<Self> {
        let block = channels * 4;
        out.write_all(b"RIFF")?;
        out.write_all(&0u32.to_le_bytes())?;
        out.write_all(b"WAVEfmt ")?;
        out.write_all(&16u32.to_le_bytes())?;
        out.write_all(&3u16.to_le_bytes())?;
        out.write_all(&channels.to_le_bytes())?;
        out.write_all(&rate.to_le_bytes())?;
        out.write_all(&(rate * u32::from(block)).to_le_bytes())?;
        out.write_all(&block.to_le_bytes())?;
        out.write_all(&32u16.to_le_bytes())?;
        out.write_all(b"data")?;
        out.write_all(&0u32.to_le_bytes())?;
        Ok(Self { out, samples: 0 })
    }

    fn push(&mut self, data: &[f32]) -> io::Result<()> {
        for s in data {
            self.out.write_all(&s.to_le_bytes())?;
        }
        self.samples += data.len() as u32;
        Ok(())
    }

    fn finish(mut self) -> io::Result<W> {
        let data = self.samples * 4;
        self.out.seek(SeekFrom::Start(4))?;
        self.out.write_all(&(36 + data).to_le_bytes())?;
        self.out.seek(SeekFrom::Start(40))?;
        self.out.write_all(&data.to_le_bytes())?;
        self.out.flush()?;
        Ok(self.out)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(out) = args.get(1).cloned() else {
        eprintln!("usage: kh2-looprec <out.wav> [seconds]");
        std::process::exit(2);
    };
    let secs: f32 = args.get(2).map(|s| s.parse().expect("seconds")).unwrap_or(10.0);
    let host = cpal::default_host();
    let dev = host.default_output_device().expect("no output device");
    let cfg = dev.default_output_config().expect("output config");
    eprintln!("device {:?} {:?}", dev.description().map(|d| d.to_string()), cfg);
    if cfg.sample_format() != cpal::SampleFormat::F32 {
        eprintln!("the output device mixes in {:?}, only f32 is supported", cfg.sample_format());
        std::process::exit(1);
    }
    let file = std::io::BufWriter::new(std::fs::File::create(&out).expect("create the wav"));
    let wav = FloatWav::new(file, cfg.channels(), cfg.sample_rate()).expect("wav header");
    let wav = Arc::new(Mutex::new(Some(wav)));
    let sink = wav.clone();
    // On WASAPI an input stream on an output device records in loopback mode.
    let stream = dev
        .build_input_stream(
            &cfg.into(),
            move |d: &[f32], _| {
                if let Some(w) = sink.lock().unwrap().as_mut() {
                    w.push(d).expect("write samples");
                }
            },
            |e| eprintln!("stream error {e}"),
            None,
        )
        .expect("loopback stream");
    stream.play().expect("start the stream");
    std::thread::sleep(std::time::Duration::from_secs_f32(secs));
    drop(stream);
    let wav = wav.lock().unwrap().take().expect("writer");
    wav.finish().expect("finish the wav");
    eprintln!("wrote {out}");
}

#[cfg(test)]
mod tests {
    use super::FloatWav;
    use std::io::Cursor;

    #[test]
    fn float_wav_header_and_sizes() {
        let mut w = FloatWav::new(Cursor::new(Vec::new()), 2, 48_000).unwrap();
        w.push(&[0.5, -0.25, 1.0, 0.0]).unwrap();
        let b = w.finish().unwrap().into_inner();
        let u16_at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
        let u32_at = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        assert_eq!(b.len(), 44 + 16);
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(u32_at(4), 36 + 16);
        assert_eq!(&b[8..16], b"WAVEfmt ");
        assert_eq!(u16_at(20), 3, "IEEE float");
        assert_eq!(u16_at(22), 2);
        assert_eq!(u32_at(24), 48_000);
        assert_eq!(u32_at(28), 48_000 * 8);
        assert_eq!(u16_at(32), 8);
        assert_eq!(u16_at(34), 32);
        assert_eq!(&b[36..40], b"data");
        assert_eq!(u32_at(40), 16);
        assert_eq!(f32::from_le_bytes([b[44], b[45], b[46], b[47]]), 0.5);
        assert_eq!(f32::from_le_bytes([b[56], b[57], b[58], b[59]]), 0.0);
    }
}
