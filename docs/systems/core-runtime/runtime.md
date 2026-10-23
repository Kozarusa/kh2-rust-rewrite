# Core runtime: boot, task system, frame step, RNG

Ladder step L1, spec WP-L1-S1 (2026-10-23). Ledger notes `L1:task`, `L1:frame`, `L1:rng`. Everything here is read from the decomp of
the USA executable; field names are ours.

## Boot thread (`FUN_00101d68`)
- Creates the root task manager: `FUN_001024f8(0x3890c0, 0x10000)` (an allocator over that region, then the manager), stored at
  `DAT_0033e69c`.
- Seeds the RNG: `FUN_001062f0(0x875b2f)` -> `FUN_00311790` stores the seed (low 32 bits) in the 64-bit state word at reent `+0xa8`
  (reent pointer `PTR_DAT_00373cf0`). The seed is the constant 0x875b2f on every boot.
- Runs the system initialisers in a fixed order (`FUN_00138860` frame timing, `FUN_001688a0` game init, sound, ..., `FUN_00105638`
  save block), starts the game mode `FUN_00102070(0x33e710)`, then loops forever: `FUN_001029d8(root)` (one pass over all tasks) and
  the pass counter `DAT_0033e6a0 = (DAT_0033e6a0 + 1) % 1000`.

## Task manager (0x120 bytes) and tasks (0x48 bytes)
- Manager (`FUN_001024a0`): +4 allocator, +8 head, +0xc tail, +0x110 the task the current pass resumes from; +0x10, +0x110 cleared.
- Task (`FUN_00102778` allocates 0x48 through the allocator's first vtable slot, `FUN_001022b0` fills it): +0 function (0 = finished),
  +0x24 owner manager, +0x28 class bits, +0x2c priority, +0x30 finalizer, +0x34 thread (0 = plain call), +0x38 next, +0x3c prev.
  `FUN_00102808` also allocates a thread of `size + 0x100` for coroutine tasks.
- Insert (`FUN_00102518`): walks from the head while `next.priority <= new.priority`, so equal priorities run in creation order and
  lower priorities run first.
- Pass (`FUN_001028a0(mgr, mask, value)`, `FUN_001029d8` = all): every task with `(bits & mask) == value` in list order; the running
  task is kept in `DAT_0033e6c0` (restored after). A task whose function became 0 is unlinked and freed (`FUN_001025e8` ->
  `FUN_001022d8`: finalizer, thread, then the task).
- Run one task (`FUN_00102340`): no thread: call the function. With a thread: depth counter `DAT_0033e858` down (`FUN_001055d8`),
  switch in (`FUN_00105440`, register save; returns 0 on entry, 1 on finish); on entry, if thread `+0xf0` is 0 call the function and
  finish, else stay suspended while the wait counter `+0xf4 > 0` or the wait time `+0xf8 > 0.0`; depth up (`FUN_00105600`).
  Yield (`FUN_00102438`, `FUN_00102460`) switches back through `FUN_00105510` (returns 2). The thread stack of the current depth is
  `0x3b21d8 + 4 * depth` (`FUN_00105618`).
- Helpers: count `FUN_001029f8`, contains-function `FUN_00102a28`, kill by bits `FUN_00102678` / all `FUN_00102720`, destroy manager
  `FUN_00102740`. Field-mode wrappers `FUN_00102bc8` / `FUN_00102bf8` create on the field manager `DAT_0033e6e4`.
- Game modes: `FUN_00102070(mode)` disables every running mode of the list at `DAT_003990c0` whose bits overlap (`FUN_001021c0`)
  and starts the mode as a coroutine task on the root manager.
- Port [inferred]: the register-switch coroutines are replaced by Rust state machines that keep the same resume points; the list,
  the insert order, the pass order and the finish rule are ported exactly (oracle: insert and pass orders over random priorities).

## Frame step (`dk::Vsync` unit, 0x14c6b0..0x14dc70)
- Init `FUN_00138860` -> `FUN_0014cd78`: `FUN_0014cd38` (frame type 1, limit 1, counters `DAT_00356f34`/`DAT_00356f38` = 0),
  installs the vblank handler `FUN_0014cc88` (`FUN_002f9db0`), and creates the frame task `FUN_0014ccf8` at priority 800000 (the last
  task of every pass).
- Vblank handler `FUN_0014cc88`: both counters +1 per vblank (60 Hz NTSC).
- Frame task `FUN_0014ccf8` -> `FUN_0014cdd0`: waits (`FUN_0014d1a8`) until the vblank counter reaches the limit `DAT_00356f4c + 1`;
  the frame step `DAT_00356f40` = vblanks elapsed (or the fixed constant `DAT_00379174` when `DAT_00356f50` is 0) times the game
  speed `DAT_00356f3c`, clamped at `DAT_00379178`; `DAT_00356f44` the unscaled step; `DAT_00356f48 = DAT_0037917c / step`; the counter
  resets. `FUN_0014cbf8` is the fixed-step variant used by the limit setter. Game speed is set by `FUN_0014cb50` (event opcode).
- Rig deviation "frame step" (DEVIATIONS 2026-10-23) belongs here: the port's step must be this value, not a wall-clock delta.

### Read in WP-L1-P3 (port: `crates/kh2-game-core/src/frame.rs`, oracle `tests/orig_frame.rs`)
- Block: the words `0x356f18..0x356f54` form one run (offsets from `0x356f18`): +0 timer ticks, +4 pause bits, +8 timer mark, +0xc
  count-down length, +0x10 timer value, +0x14 timer running, +0x18 count-down flag (byte), +0x1c vblanks since init, +0x20 vblanks
  since the last step, +0x24 game speed, +0x28 step, +0x2c unscaled step, +0x30 rate, +0x34 limit, +0x38 frame type (byte).
- The wait `FUN_0014d1a8`: `WaitSema` on the vblank semaphore `DAT_00356f78`, then, when the semaphore `DAT_00357fd8` has waiting
  threads (`ReferSemaStatus`, the wait-thread count), signals it once. The handler side `FUN_0014d1f8` (called by `FUN_0014cc88`)
  signals `DAT_00356f78` only when a thread waits on it, so one wait returns at the next vblank and missed vblanks do not queue up;
  they show in the counter instead. `DAT_00357fd8` belongs to a helper thread created by `FUN_0014d0a0` (priority 0x28, entry
  `0x14d008`; the frame thread itself is set to priority 0x1e) that runs queued callbacks `FUN_0014d138` after each vblank
  [inferred: from `FUN_0014d03c`, which the decomp shows only partly].
- The step `FUN_0014cdd0`: one wait; then for frame type 0 the step is the constant 2.0 (`DAT_00379174`), else the counter as a
  float. When the counter is below `limit + 1` (unsigned) and the limit is not 0, it waits exactly `limit` more times (the count is
  not rechecked), re-reading the counter after each wait for type 1. Then unscaled = step, step = step * speed, each clamped at 6.0
  (`DAT_00379178`), the counter cleared, rate = 1.0 (`DAT_0037917c`) / step.
- Who sets the cadence: init `FUN_0014cd38` (type 1, limit 1, speed 1.0 `DAT_0037916c`): the field runs 30 Hz, step 2.0. The menus
  (`FUN_00239e78`) save type and limit in `DAT_0036ca40`/`DAT_0036ca44`, set type 1 and limit 0 (60 Hz, step 1.0); `FUN_00239ec0`
  restores them. The musical event mode (`FUN_002251f0`) sets type 0 (fixed 2.0). The only reader of the getter `FUN_0014cbe8` found
  by call site is `FUN_0023a920` [inferred: most movers read `0x356f40` directly].
- `FUN_0014cbf8` (from `setLimit` `FUN_0014cba0`): step = `limit + 1` times 1.0 (signed count, none for a negative), times the
  speed, unclamped, rate 1.0 / step. Game speed: `FUN_0014cbc0` stores f12, `FUN_0014cbd0` resets it; the event opcode
  `setGameSpeed` `FUN_0014cb50` starts a fade object at `0x4843b8` (`FUN_0014eca0`) and a task `FUN_0014cb08` that stores its value
  through `FUN_0014cbc0` each pass [inferred: the fade object `FUN_0014ed20`/`FUN_0014eca0` was not read].
- Vblank handler `FUN_0014cc88`: play time `FUN_0014ca28` (save block +0x1604 and a per-world counter, each capped below
  0xcdfe5c4), timer ticks `FUN_0014c7b8` (unless a pause bit is set), both counters +1, the semaphore signal.
- Timer `dk::Timer`: `FUN_0014c6b0` count-down of n * 60, `FUN_0014c6e0` count-up, `FUN_0014c710` is-down, `FUN_0014c728` start
  (mark = ticks), `FUN_0014c748`/`FUN_0014c760` pause bits, `FUN_0014c780` paused, `FUN_0014c790` stop (returns the value),
  `FUN_0014c7a8` value. No writer of the timer value (+0x10) other than these was found in the unit [inferred: by reading the unit
  only, not a store scan]. `FUN_0014c7f0` splits a vblank count into hours (/216000), minutes, seconds and hundredths (leftover
  sixtieths times 1.666667, `DAT_00379168`, cut by `FUN_00326940`); `FUN_0014c950`/`FUN_0014c9b0` apply it to the play times.
- Not frame logic: `0x14d240..0x14dc70` is the vblank thread setup and the GS clear and image upload helpers (`FUN_0014d378`, the
  0x1d0-byte draw-buffer objects `FUN_0014d8a0`..`FUN_0014dbe8`) [inferred: hardware drawing, render.gs].
- The rig deviation "frame step": explained. The original reports 1.0 in menus and 2.0 in the field because the field runs limit 1.
  The app runs every game frame on a 60 Hz tick with step 1.0, the original's limit 0, everywhere. The `frame.step` getter now comes
  from the port's `Vsync` at the app's cadence (still 1.0); reporting 2.0 would only be true once the field runs at 30 Hz with step
  2.0, a behaviour change left to the frame-order package (L1-P4).

## RNG
- `rand` `FUN_003117a8`: state = state * M + 1 (64-bit, `FUN_00324820` is the 64-bit multiply, M the word at `DAT_00385f90`, read
  from the executable), result `(state >> 32) & 0x7fffffff`.
- Range helpers: `FUN_00106308(n)` = int of `n * rand / DAT_003780e0` (unsigned n), `FUN_00106380(lo, hi)` = lo + that of hi - lo;
  `FUN_001063b8(x)` = `x * rand / DAT_003780e4`, `FUN_00106408(lo, hi)` its range form. Callers include friend AI (`FUN_0010f420`) and
  more than ten object routines; no other RNG was found [inferred: search by the `rand` call sites only].
- For the rig: the state word is in RAM (reent `+0xa8`), so a save state carries it; the port reads it from the state file.

## Decompile index
0x101d68, 0x102070, 0x1022b0, 0x1022d8, 0x102340, 0x102438, 0x102460, 0x1024a0, 0x1024f8, 0x102518, 0x1025e8, 0x102660, 0x102678,
0x102720, 0x102740, 0x102778, 0x102808, 0x1028a0, 0x1029d8, 0x1029f8, 0x102a28, 0x102bc8, 0x102bf8, 0x105440, 0x1054a8, 0x105510,
0x1055d8, 0x105600, 0x105618, 0x1062f0, 0x106308, 0x106380, 0x1063b8, 0x106408, 0x138860, 0x14cbf8, 0x14cc88, 0x14ccf8, 0x14cd38,
0x14cd78, 0x14cdd0, 0x311790, 0x3117a8, 0x324820. WP-L1-P3: every function of 0x14c6b0..0x14dc70 (0x14c6b0, 0x14c6e0, 0x14c710,
0x14c728, 0x14c748, 0x14c760, 0x14c780, 0x14c790, 0x14c7a8, 0x14c7b8, 0x14c7e8, 0x14c7f0, 0x14c8c8, 0x14c910, 0x14c930, 0x14c950,
0x14c9b0, 0x14ca28, 0x14cb08, 0x14cb50, 0x14cb90, 0x14cba0, 0x14cbc0, 0x14cbd0, 0x14cbe8, 0x14ccf0, 0x14cfa8, 0x14cfb0, 0x14cfe8,
0x14d03c, 0x14d078, 0x14d0a0, 0x14d138, 0x14d188, 0x14d1a8, 0x14d1f8, 0x14d240, 0x14d338, 0x14d378, 0x14d5a0, 0x14d5c8, 0x14d608,
0x14d670, 0x14d768, 0x14d8a0, 0x14d8f0, 0x14d930, 0x14d970, 0x14d9d0, 0x14da38, 0x14da40, 0x14da88, 0x14daa8, 0x14dac8, 0x14dad8,
0x14db58, 0x14dbe8, 0x14dc70), and 0x2251f0, 0x239e78, 0x239ec0.

## Gaps
- `FUN_0014ddb0`/`FUN_0014e318` (after the step), `FUN_001b6048`/`FUN_002f5ee0` (before it, in the frame task), the fade object of
  the game speed (`FUN_0014eca0`, `FUN_0014ed20`), the helper thread entry `0x14d008`.
- Thread `+0xf0..+0xf8` writers (the sleep and wait calls) not read.
- Game-mode list layout at `DAT_003990c0` (`+0xc` next, `+0x14` bits, `+0x1c` state) only seen from `FUN_00102070`.
