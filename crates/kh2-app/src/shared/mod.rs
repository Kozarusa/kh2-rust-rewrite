//! ECS data the feature modules exchange: components, resources, messages, constants and pure rules that a lower
//! crate reads from a higher one. Each source module re-exports its group (`pub use crate::shared::<name>::*;`).

pub mod sora;
pub mod abilities;
pub mod prize;
pub mod drive;
pub mod magic;
pub mod enemy;
pub mod enemy_obj;
pub mod struggle;
pub mod area_jump;
pub mod reaction;
pub mod summon;
pub mod party;
