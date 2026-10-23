//! The movie hook (boot contract section 6). Every movie goes through the MOVIE function-call
//! entry 0x2f5eb8 `(op, arg)`; the hook answers the way a movie that starts, finishes at once and
//! stops would, so the IPU and libmpeg are never reached.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ee::{Ee, reg};

/// MOVIE function-call entry of the USA executable [V: contract section 6].
pub const MOVIE_CALL: u32 = 0x2f5eb8;

/// What the overlay's player reports through op 5 [inferred from the callers]: 2 = prepared
/// (0x225860 waits for it after the start, op 0), 5 = finished (`isMovieEnd` 0x225790, polled by
/// 0x222668 every frame while the movie overlay is active, flag 0x371690).
pub const STATE_PREPARED: u32 = 2;
pub const STATE_FINISHED: u32 = 5;

/// The player the hook stands in for: a movie is prepared by op 0, and once op 1 has run (the
/// second loop of 0x225860, which waits for 0) it is finished at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Player {
    pub playing: bool,
}

impl Player {
    /// The answer for an op [inferred from the callers 0x225860, 0x225790, 0x2256e0, 0x222728 and
    /// the pause/resume wrapper: op 5 is the player state (2 after the start, 5 once played), op 1
    /// returns 0 when done, op 6 is the decoded picture (0: none, so 0x2256e0 draws nothing), op 7
    /// the movie's frame number (0); ops 2, 3, 4 answer 0].
    pub fn call(&mut self, op: u32) -> u32 {
        match op {
            0 => {
                self.playing = false;
                0
            }
            1 => {
                self.playing = true;
                0
            }
            5 if self.playing => STATE_FINISHED,
            5 => STATE_PREPARED,
            _ => 0,
        }
    }
}

/// Every call the hook answered, as (op, arg), oldest first.
pub type MovieLog = Rc<RefCell<Vec<(u32, u32)>>>;

/// Installs the hook and returns the log of calls.
pub fn install(ee: &mut Ee) -> MovieLog {
    let log: MovieLog = Rc::new(RefCell::new(Vec::new()));
    let l = log.clone();
    let mut player = Player::default();
    ee.hooks.insert(MOVIE_CALL, move |ee: &mut Ee| {
        let (op, arg) = (ee.gpr[reg::A0] as u32, ee.gpr[reg::A0 + 1] as u32);
        l.borrow_mut().push((op, arg));
        ee.gpr[reg::V0] = player.call(op) as u128;
    });
    log
}
