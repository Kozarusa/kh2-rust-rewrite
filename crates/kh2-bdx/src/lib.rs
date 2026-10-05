//! BDX: the bytecode scripts KH2 uses for enemy AI (one per enemy object, in the object's MDLX) and
//! for area logic (`00common.bdx`, area files). Format and instruction set were read from the game's
//! interpreter on the user's disc; see docs/bdx.md. Nothing here depends on a game engine: the
//! interpreter talks to the world through the [`Host`] trait.

pub mod disasm;
pub mod exe;
pub mod host;
pub mod insn;
pub mod math;
pub mod mock;
pub mod names;
pub mod program;
pub mod runtime;
pub mod signatures;
pub mod trig;
pub mod vm;

pub use disasm::{Analysis, analyze, listing};
pub use exe::HostTable;
pub use host::{Call, Host, Value, Vec4};
pub use insn::{BinaryOp, Insn, UnaryOp, decode};
pub use program::{EntryPoint, Error, Program};
pub use runtime::{Runtime, message_delay};
pub use vm::VmError;
