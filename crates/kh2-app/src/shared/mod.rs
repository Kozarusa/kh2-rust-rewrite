//! ECS data the feature modules exchange: components, resources, messages, constants and pure rules that a lower
//! crate reads from a higher one. Each source module re-exports its group (`pub use crate::shared::<name>::*;`).

pub mod sora;
pub mod abilities;
