//! KH2 effects: the PAX/DPX/DPD containers and the particle program ("ppp") runtime.
//!
//! An effect is a manager running groups of objects; every object runs its group's programs once
//! per 60 Hz tick (move, rotate, scale, color, build a matrix, draw). See [`pax`] for the data
//! and [`ppp`] for the program table.

pub mod omd;
pub mod pax;
pub mod ppp;
pub(crate) mod prog;
mod rd;
pub mod vm;

pub use pax::{Dpd, Element, Pax, ParticleData};
pub use vm::{Draw, Effect, Env, Strip};
