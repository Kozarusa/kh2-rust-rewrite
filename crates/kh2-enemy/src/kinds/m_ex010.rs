//! The enemy kind `M_EX010`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

pub const KIND: Kind = Kind { name: "Soldier", enmp: 33, model: "M_EX010", death_effect: true, status: true };
