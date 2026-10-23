//! The enemy kind `M_EX020`: its `Kind` (one file per kind; `build.rs` lists the files into `enemy::KINDS`).

use crate::shared::enemy::Kind;

pub const KIND: Kind = Kind { name: "Shadow", enmp: 34, model: "M_EX020", death_effect: true, status: true };
