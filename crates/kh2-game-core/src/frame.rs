//! The game's frame step, vblank counters and the count-down timer, ported on the original's byte layout (core runtime, WP-L1-P3;
//! docs/systems/core-runtime/runtime.md "Frame step").
//!
//! The original keeps these words in one run of globals at `0x356f18..0x356f54` (the `dk::Vsync` and `dk::Timer` units).
//! [`Vsync`] is that run as a byte block with accessors at the original's offsets (RULES C7), so an oracle test copies it into kh2-ee
//! and back unchanged. Offsets below are from `0x356f18`.
//!
//! How a frame works in the original: the vblank handler (`FUN_0014cc88`, 60 Hz) adds 1 to two counters; the frame task
//! (`FUN_0014ccf8`, priority 800000, the last task of every pass) calls [`Vsync::step_frame`], which waits for vblanks until the
//! counter reaches `limit + 1`, then stores the step: the vblanks elapsed (frame type 1) or the fixed 2.0 (frame type 0), times the
//! game speed, clamped at 6.0. With the field's limit 1 two vblanks pass per game frame and the step is 2.0; the menus set limit 0
//! (`FUN_00239e78`), one vblank per frame, step 1.0.
//!
//! The waits are the one part that is not data: [`Vsync::step_frame`] takes the wait (`FUN_0014d1a8`, block until the next vblank)
//! as a closure. A host that drives the port at 60 Hz passes [`Vsync::vblank`] (one vblank per wait), which gives the original's
//! counts when no vblank is missed. The app drives it once per fixed step through `schedule::FrameClock` (WP-L1-P4).

/// Address of the block in the original.
pub const BASE: u32 = 0x0035_6f18;
/// Size of the block (the frame type byte at +0x38 and its padding).
pub const SIZE: usize = 0x3c;

/// `DAT_0037916c`: the game speed after init (1.0).
pub const SPEED_INIT: f32 = 1.0;
/// `DAT_00379170`: one vblank's step (1.0), the fixed-step variant's unit and the numerator of its rate.
pub const ONE: f32 = 1.0;
/// `DAT_00379174`: the step of frame type 0 (2.0).
pub const FIXED_STEP: f32 = 2.0;
/// `DAT_00379178`: the largest step (6.0).
pub const MAX_STEP: f32 = 6.0;
/// `DAT_0037917c`: the numerator of the rate (1.0).
pub const RATE_NUM: f32 = 1.0;
/// `DAT_00379168`: sixtieths to hundredths (1.666667).
pub const HUNDREDTHS: f32 = 1.666_666_7;
/// Vblanks per second (NTSC).
pub const VBLANKS_PER_SECOND: u32 = 60;

/// The EE's float unit has no infinities: an overflow saturates to the largest finite number.
fn finite(x: f32) -> f32 {
    if x.is_nan() {
        f32::MAX
    } else if x.is_infinite() {
        f32::MAX.copysign(x)
    } else {
        x
    }
}

/// The block at `0x356f18`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Vsync(pub [u8; SIZE]);

impl Default for Vsync {
    fn default() -> Self {
        Vsync([0; SIZE])
    }
}

macro_rules! word {
    ($get:ident, $set:ident, $off:expr, u32, $doc:literal) => {
        #[doc = $doc]
        pub fn $get(&self) -> u32 {
            u32::from_le_bytes(self.0[$off..$off + 4].try_into().unwrap())
        }
        pub fn $set(&mut self, v: u32) {
            self.0[$off..$off + 4].copy_from_slice(&v.to_le_bytes())
        }
    };
    ($get:ident, $set:ident, $off:expr, f32, $doc:literal) => {
        #[doc = $doc]
        pub fn $get(&self) -> f32 {
            f32::from_le_bytes(self.0[$off..$off + 4].try_into().unwrap())
        }
        pub fn $set(&mut self, v: f32) {
            self.0[$off..$off + 4].copy_from_slice(&v.to_le_bytes())
        }
    };
}

impl Vsync {
    word!(
        timer_ticks,
        set_timer_ticks,
        0x00,
        u32,
        "+0x00 (`0x356f18`): vblanks counted while no pause bit is set (`FUN_0014c7b8`)."
    );
    word!(
        pause_bits,
        set_pause_bits,
        0x04,
        u32,
        "+0x04: the timer's pause bits; any set bit stops `timer_ticks`."
    );
    word!(
        timer_mark,
        set_timer_mark,
        0x08,
        u32,
        "+0x08: `timer_ticks` when the timer was started (`FUN_0014c728`)."
    );
    word!(
        countdown_start,
        set_countdown_start,
        0x0c,
        u32,
        "+0x0c: the count-down's length in vblanks (seconds * 60)."
    );
    word!(
        timer_value,
        set_timer_value,
        0x10,
        u32,
        "+0x10: the timer's value (the count-down's remaining vblanks)."
    );
    word!(
        timer_running,
        set_timer_running,
        0x14,
        u32,
        "+0x14: 1 while the timer runs."
    );
    word!(
        vblanks,
        set_vblanks,
        0x1c,
        u32,
        "+0x1c (`0x356f34`): vblanks since init."
    );
    word!(
        since_step,
        set_since_step,
        0x20,
        u32,
        "+0x20 (`0x356f38`): vblanks since the last frame step; the step resets it."
    );
    word!(
        speed,
        set_speed,
        0x24,
        f32,
        "+0x24 (`0x356f3c`): the game speed the step is scaled by."
    );
    word!(
        step,
        set_step,
        0x28,
        f32,
        "+0x28 (`0x356f40`): the frame step (scaled, clamped); the value every mover multiplies by."
    );
    word!(
        raw_step,
        set_raw_step,
        0x2c,
        f32,
        "+0x2c (`0x356f44`): the frame step before the game speed (clamped)."
    );
    word!(rate, set_rate, 0x30, f32, "+0x30 (`0x356f48`): `1 / step`.");
    word!(
        limit,
        set_limit,
        0x34,
        u32,
        "+0x34 (`0x356f4c`): extra vblanks per frame (0: 60 Hz, 1: 30 Hz)."
    );

    /// +0x18 (`0x356f30`, a byte): 1 while the timer counts down.
    pub fn counting_down(&self) -> u8 {
        self.0[0x18]
    }
    pub fn set_counting_down(&mut self, v: u8) {
        self.0[0x18] = v
    }
    /// +0x38 (`0x356f50`, a byte): the frame type, 0 for the fixed step, otherwise the step follows the vblanks elapsed.
    pub fn frame_type(&self) -> u8 {
        self.0[0x38]
    }

    /// The state after the init `FUN_0014cd38`: frame type 1, limit 1 (and its fixed step), game speed 1.0, both counters 0.
    ///
    /// orig 0x14cd38
    pub fn init(&mut self) {
        self.set_frame_type(1);
        self.set_frame_limit(1);
        self.reset_speed();
        self.set_vblanks(0);
        self.set_since_step(0);
    }

    /// `dk::Vsync::setFrameType`.
    ///
    /// orig 0x14cb90
    pub fn set_frame_type(&mut self, t: u8) {
        self.0[0x38] = t
    }

    /// `dk::Vsync::setLimit`: stores the limit and recomputes the step for it ([`Vsync::fixed_step`]).
    ///
    /// orig 0x14cba0
    pub fn set_frame_limit(&mut self, limit: u32) {
        self.set_limit(limit);
        self.fixed_step();
    }

    /// Sets the game speed (the task `FUN_0014cb08` that the event opcode `FUN_0014cb50` queues ends here).
    ///
    /// orig 0x14cbc0
    pub fn set_game_speed(&mut self, s: f32) {
        self.set_speed(s)
    }

    /// Game speed back to 1.0.
    ///
    /// orig 0x14cbd0
    pub fn reset_speed(&mut self) {
        self.set_speed(SPEED_INIT)
    }

    /// The frame step getter.
    ///
    /// orig 0x14cbe8
    pub fn frame_step(&self) -> f32 {
        self.step()
    }

    /// The step a limit gives without waiting: `limit + 1` vblanks (summed one at a time, nothing for a limit below 0 as a signed
    /// number), times the game speed, unclamped; the rate is `1 / step`.
    ///
    /// orig 0x14cbf8
    pub fn fixed_step(&mut self) {
        let mut s = 0.0f32;
        let n = (self.limit() as i32).wrapping_add(1);
        for _ in 0..n.max(0) {
            s = finite(s + ONE);
        }
        self.set_raw_step(s);
        let s = finite(s * self.speed());
        self.set_step(s);
        self.set_rate(finite(ONE / s));
    }

    /// One vblank as the handler `FUN_0014cc88` counts it: both frame counters and the pause-gated timer ticks. (The handler also
    /// adds to the play time in the save block, `FUN_0014ca28`, ported as `save_block::play_time_tick` and not called from here, and signals the
    /// vblank semaphore, which is not here.)
    pub fn vblank(&mut self) {
        self.tick_timer();
        self.set_vblanks(self.vblanks().wrapping_add(1));
        self.set_since_step(self.since_step().wrapping_add(1));
    }

    /// The frame step `FUN_0014cdd0`. `wait` stands for `FUN_0014d1a8` (block until the next vblank); pass [`Vsync::vblank`] to
    /// model one vblank per wait.
    ///
    /// The counter converts to a float as an unsigned number.
    ///
    /// orig 0x14cdd0
    pub fn step_frame(&mut self, mut wait: impl FnMut(&mut Vsync)) {
        wait(self);
        let fixed = self.frame_type() == 0;
        if fixed {
            self.set_step(FIXED_STEP);
        } else {
            self.set_step(self.since_step() as f32);
        }
        // Waits `limit` more vblanks when the counter is short of `limit + 1` (unsigned compares; the count is not rechecked).
        if self.since_step() < self.limit().wrapping_add(1) && self.limit() != 0 {
            let mut n = 0u32;
            while n < self.limit() {
                wait(self);
                if !fixed {
                    self.set_step(self.since_step() as f32);
                }
                n += 1;
            }
        }
        let raw = self.step();
        let scaled = finite(raw * self.speed());
        self.set_raw_step(raw);
        self.set_step(scaled);
        if MAX_STEP < scaled {
            self.set_step(MAX_STEP);
        }
        if MAX_STEP < raw {
            self.set_raw_step(MAX_STEP);
        }
        self.set_since_step(0);
        self.set_rate(finite(RATE_NUM / self.step()));
    }

    /// `dk::Timer::setCountDown(seconds)`: a count-down of `seconds * 60` vblanks, stopped.
    ///
    /// orig 0x14c6b0
    pub fn set_count_down(&mut self, seconds: i32) {
        let v = seconds.wrapping_mul(VBLANKS_PER_SECOND as i32) as u32;
        self.set_countdown_start(v);
        self.set_counting_down(1);
        self.set_timer_value(v);
        self.set_timer_running(0);
    }

    /// The count-up form: length `seconds * 60`, value 0, stopped.
    ///
    /// orig 0x14c6e0
    pub fn set_count_up(&mut self, seconds: i32) {
        self.set_counting_down(0);
        self.set_countdown_start(seconds.wrapping_mul(VBLANKS_PER_SECOND as i32) as u32);
        self.set_timer_running(0);
        self.set_timer_value(0);
    }

    /// `dk::Timer::isDown`.
    ///
    /// orig 0x14c710
    pub fn is_count_down(&self) -> bool {
        self.counting_down() == 1
    }

    /// `dk::Timer::start`: marks the current ticks and runs.
    ///
    /// orig 0x14c728
    pub fn start_timer(&mut self) {
        self.set_timer_mark(self.timer_ticks());
        self.set_timer_running(1);
    }

    /// `dk::Timer::pauseOn(bits)`.
    ///
    /// orig 0x14c748
    pub fn pause_on(&mut self, bits: u32) {
        self.set_pause_bits(self.pause_bits() | bits)
    }

    /// `dk::Timer::pauseOff(bits)`.
    ///
    /// orig 0x14c760
    pub fn pause_off(&mut self, bits: u32) {
        self.set_pause_bits(self.pause_bits() & !bits)
    }

    /// Whether any pause bit is set.
    ///
    /// orig 0x14c780
    pub fn paused(&self) -> bool {
        self.pause_bits() != 0
    }

    /// Stops the timer and returns its value.
    ///
    /// orig 0x14c790
    pub fn stop_timer(&mut self) -> u32 {
        self.set_timer_running(0);
        self.timer_value()
    }

    /// The timer's value.
    ///
    /// orig 0x14c7a8
    pub fn timer(&self) -> u32 {
        self.timer_value()
    }

    /// The vblank handler's tick of the timer: `timer_ticks + 1` unless paused.
    ///
    /// orig 0x14c7b8
    pub fn tick_timer(&mut self) {
        if !self.paused() {
            self.set_timer_ticks(self.timer_ticks().wrapping_add(1));
        }
    }
}

/// A vblank count (unsigned) as hours, minutes, seconds and hundredths (the play-time display): `FUN_0014c7f0`. The hundredths are the leftover
/// sixtieths times 1.666667, cut to an integer.
///
/// orig 0x14c7f0
pub fn to_hms(frames: u32) -> (u32, u32, u32, u32) {
    let hours = frames / 216_000;
    let rest = frames % 216_000;
    let minutes = rest / 3600;
    let sec = rest % 3600;
    let hundredths = kh2_model::texanim::fixuns(finite((sec % 60) as f32 * HUNDREDTHS));
    (hours, minutes, sec / 60, hundredths)
}

/// `DAT_00379290`: the step a ramp uses when it is given 0.
pub const RAMP_DEFAULT_STEP: f32 = 1.0;
/// `DAT_00379180`: the end value of the game-speed ramp after its static init.
pub const SPEED_RAMP_INIT_END: f32 = 1.0;
/// Address of the game-speed ramp block.
pub const SPEED_RAMP_BASE: u32 = 0x0048_43b8;

/// A linear ramp of three floats: the start, the change per step and the end. The game-speed request (the event opcode
/// `FUN_0014cb50`) keeps one at `0x4843b8`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Ramp(pub [f32; 3]);

impl Ramp {
    /// Sets the ramp from `start` to `end` in `steps` steps (a step count of 0 counts as [`RAMP_DEFAULT_STEP`]): the start at +0, the
    /// end at +8 and `(end - start) / steps` at +4.
    ///
    /// orig 0x14eca0
    pub fn set(&mut self, start: f32, end: f32, steps: f32) {
        let steps = if steps != 0.0 { steps } else { RAMP_DEFAULT_STEP };
        self.0[0] = start;
        self.0[2] = end;
        self.0[1] = crate::pad::ee::div(crate::pad::ee::sub(end, start), steps);
    }

    /// The end test of a ramp: false while the value has not reached the end (it moves up for a change of 0 or more, down below 0);
    /// true after it has, and then the value is set to the end.
    ///
    /// orig 0x14ecd8
    pub fn check(&mut self) -> bool {
        let reached = if 0.0 <= self.0[1] { self.0[2] <= self.0[0] } else { self.0[0] <= self.0[2] };
        if reached {
            self.0[0] = self.0[2];
        }
        reached
    }

    /// One advance of a ramp: the value goes up by twice the change (the change doubled first, then added), then [`Ramp::check`] answers.
    ///
    /// orig 0x14ed20
    pub fn advance(&mut self) -> bool {
        use crate::pad::ee::add;
        self.0[0] = add(self.0[0], add(self.0[1], self.0[1]));
        self.check()
    }

    /// The static init of the game-speed ramp: only the constructor-list call `(1, 0xffff)` does anything; it zeroes the start and
    /// the change and sets the end to 1.0.
    ///
    /// orig 0x14cfb0
    pub fn static_init(&mut self, a: u64, b: u64) {
        if b == 0xffff && a == 1 {
            self.0[0] = 0.0;
            self.0[1] = 0.0;
            self.0[2] = SPEED_RAMP_INIT_END;
        }
    }

    /// The constructor-list wrapper: `static_init(1, 0xffff)`.
    ///
    /// orig 0x14cfe8
    pub fn static_init_wrapper(&mut self) {
        self.static_init(1, 0xffff)
    }
}

/// The game-speed task (priority 800000 on the root manager): the ramp advances; when it answers that it has ended `FUN_00102498` is
/// called with the task's argument `arg` (a stand-in for the host); then the game speed is set to the ramp's value.
///
/// orig 0x14cb08
pub fn game_speed_task(ramp: &mut Ramp, v: &mut Vsync, host: &mut impl crate::area::ctl::AreaHost, arg: u64) {
    if ramp.advance() {
        host.call(0x102498, &[arg]);
    }
    v.set_game_speed(ramp.0[0]);
}

/// The game-speed request: the ramp at [`SPEED_RAMP_BASE`] is set from `start` to `end` in `steps`, then [`game_speed_task`] is made a
/// task of the root manager `root` (bits 0, priority 800000).
///
/// orig 0x14cb50
pub fn game_speed_request(ramp: &mut Ramp, tasks: &mut crate::task::Tasks, root: u32, start: f32, end: f32, steps: f32) -> u32 {
    ramp.set(start, end, steps);
    tasks.create_task(root, 0, FRAME_PRIORITY_SPEED, GAME_SPEED_TASK)
}

/// The vblank handler: one tick of the play time in the save block `save` (`counting`: the word at `0x33e6fc` is not 0; `world`: the byte
/// at `0x33e830`), the pause-gated timer tick and the two frame counters ([`Vsync::vblank`]), then `FUN_0014d1f8` (signals the vblank
/// semaphore, a stand-in for the host). The cache flushes and the interrupt enable are hardware. Returns 0.
///
/// orig 0x14cc88
pub fn vblank_handler(v: &mut Vsync, save: &mut [u8], counting: bool, world: u8, host: &mut impl crate::area::ctl::AreaHost) -> u64 {
    crate::save_block::play_time_tick(save, counting, world);
    v.vblank();
    host.call(0x14d1f8, &[]);
    0
}

/// The priority of the game-speed task.
pub const FRAME_PRIORITY_SPEED: i32 = 800_000;
/// The game-speed task body.
pub const GAME_SPEED_TASK: u32 = 0x14cb08;

/// The identity helper of the clock unit: returns its argument (the play-time splitter passes the count through it).
///
/// orig 0x14c7e8
pub fn clock_pass(x: u64) -> u64 {
    x
}

fn save_word(save: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(save[off..off + 4].try_into().unwrap())
}

/// The play time in vblanks: the save block's word at +0x1604 (`save` starts at the block, `0x33e860`).
///
/// orig 0x14c910
pub fn play_time(save: &[u8]) -> u32 {
    save_word(save, 0x1604)
}

/// The second clock word of the save block (+0x1608), which the vblank handler copies onto itself.
///
/// orig 0x14c930
pub fn second_time(save: &[u8]) -> u32 {
    save_word(save, 0x1608)
}

/// The play time as hours, minutes, seconds and hundredths.
///
/// orig 0x14c950
pub fn play_time_hms(save: &[u8]) -> (u32, u32, u32, u32) {
    to_hms(clock_pass(play_time(save) as u64) as u32)
}

/// The time spent in one world (the words from save +0x160c, one per world) as hours, minutes, seconds and hundredths.
///
/// orig 0x14c9b0
pub fn world_time_hms(save: &[u8], world: i32) -> (u32, u32, u32, u32) {
    let off = 0x160cusize.wrapping_add((world as i64 * 4) as usize);
    to_hms(clock_pass(save_word(save, off) as u64) as u32)
}

/// The first call of the frame task: it does nothing.
///
/// orig 0x14ccf0
pub fn frame_task_prelude() {}

/// The frame task, the last task of every pass of the root manager: the empty prelude, `FUN_001b6048`, `FUN_002f5ee0`, the frame
/// step (`wait` as in [`Vsync::step_frame`]), then `FUN_0014ddb0` and its result passed to `FUN_0014e318`.
///
/// orig 0x14ccf8
pub fn frame_task(v: &mut Vsync, host: &mut impl crate::area::ctl::AreaHost, wait: impl FnMut(&mut Vsync)) {
    frame_task_prelude();
    host.call(0x1b6048, &[]);
    host.call(0x2f5ee0, &[]);
    v.step_frame(wait);
    let r = host.call(0x14ddb0, &[]);
    host.call(0x14e318, &[r]);
}

/// The frame unit's set-up: the frame state (`FUN_0014cd38`, [`Vsync::init`]), the timer task (`FUN_0014c678`), the vblank handler
/// `FUN_0014cc88` installed (`FUN_002f9db0`, hardware), the vblank thread (`FUN_0014d0a0`, hardware), and the frame task made on the
/// root manager `root` at priority 800000.
///
/// orig 0x14cd78
pub fn frame_unit_start(
    v: &mut Vsync,
    host: &mut impl crate::area::ctl::AreaHost,
    tasks: &mut crate::task::Tasks,
    root: u32,
) -> u32 {
    v.init();
    host.call(0x14c678, &[]);
    host.call(0x2f9db0, &[0x14cc88]);
    host.call(0x14d0a0, &[]);
    tasks.create_task(root, 0, crate::task::FRAME_PRIORITY, crate::task::FRAME_TASK)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(limit: u32, frame_type: u8, speed: f32) -> f32 {
        let mut v = Vsync::default();
        v.init();
        v.set_frame_type(frame_type);
        v.set_frame_limit(limit);
        v.set_game_speed(speed);
        v.step_frame(Vsync::vblank);
        v.step()
    }

    #[test]
    fn the_field_steps_two_and_the_menu_one() {
        assert_eq!(run(1, 1, 1.0), 2.0, "limit 1: two vblanks per frame");
        assert_eq!(run(0, 1, 1.0), 1.0, "limit 0: one vblank per frame");
        assert_eq!(run(0, 0, 1.0), 2.0, "frame type 0: the fixed step");
        assert_eq!(run(1, 1, 0.5), 1.0, "game speed scales the step");
        assert_eq!(run(5, 1, 2.0), MAX_STEP, "clamped");
    }

    #[test]
    fn a_step_resets_the_count_and_sets_the_rate() {
        let mut v = Vsync::default();
        v.init();
        v.step_frame(Vsync::vblank);
        assert_eq!(
            (v.since_step(), v.vblanks(), v.rate(), v.raw_step()),
            (0, 2, 0.5, 2.0)
        );
    }

    #[test]
    fn the_timer_counts_unless_paused() {
        let mut v = Vsync::default();
        v.set_count_down(3);
        assert!(v.is_count_down());
        assert_eq!(v.timer(), 180);
        v.start_timer();
        v.pause_on(4);
        v.vblank();
        assert_eq!(v.timer_ticks(), 0);
        v.pause_off(4);
        v.vblank();
        assert_eq!(v.timer_ticks(), 1);
        assert_eq!(v.stop_timer(), 180);
        assert_eq!(to_hms(216_000 + 3600 * 2 + 60 * 5 + 30), (1, 2, 5, 50));
    }
}
