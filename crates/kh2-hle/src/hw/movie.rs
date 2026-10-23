//! The movie hook (boot contract section 6). Every movie goes through the MOVIE function-call
//! entry 0x2f5eb8 `(op, arg)`; the hook answers the way a movie that starts, finishes at once and
//! stops would, so the IPU and libmpeg are never reached.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ee::{Ee, reg};

/// MOVIE function-call entry of the USA executable [V: contract section 6].
pub const MOVIE_CALL: u32 = 0x2f5eb8;

/// The answer for an op [inferred from the callers 0x225860 and the pause/resume wrapper: start
/// (0) returns 0, the poll loop on op 5 ends when it returns 2, the stop loop on op 1 ends when
/// it returns 0; ops 2, 6, 7 are unread and answer 0].
pub fn movie_result(op: u32) -> u32 {
    if op == 5 { 2 } else { 0 }
}

/// Every call the hook answered, as (op, arg), oldest first.
pub type MovieLog = Rc<RefCell<Vec<(u32, u32)>>>;

/// Installs the hook and returns the log of calls.
pub fn install(ee: &mut Ee) -> MovieLog {
    let log: MovieLog = Rc::new(RefCell::new(Vec::new()));
    let l = log.clone();
    ee.hooks.insert(MOVIE_CALL, move |ee: &mut Ee| {
        let (op, arg) = (ee.gpr[reg::A0] as u32, ee.gpr[reg::A0 + 1] as u32);
        l.borrow_mut().push((op, arg));
        ee.gpr[reg::V0] = movie_result(op) as u128;
    });
    log
}
