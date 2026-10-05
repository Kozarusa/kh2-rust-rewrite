//! KH2 audio: SPU ADPCM samples, WD instrument banks, BGM sequences, and a software mixer that
//! plays them the way the PS2's SPU2 would (48 voices, ADSR envelopes, 48 kHz).

pub mod adpcm;
pub mod bgm;
pub mod footstep;
pub mod positional;
pub mod ssl;
pub mod synth;
pub mod vsb;
pub mod wd;

pub use bgm::{Bgm, Player};
pub use synth::{Mixer, RATE};
pub use wd::Wd;
