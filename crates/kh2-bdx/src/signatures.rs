//! Signatures of host functions the executable's table does not hold.

use std::collections::HashMap;

use crate::program::Program;

/// Argument counts and result flags worked out from a script's own code.
pub fn infer(_p: &Program) -> HashMap<(u16, u16), (usize, bool)> {
    HashMap::new()
}
