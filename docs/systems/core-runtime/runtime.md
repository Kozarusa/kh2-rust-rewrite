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
0x14cd78, 0x14cdd0, 0x311790, 0x3117a8, 0x324820.

## Gaps
- `FUN_0014d1a8` (the wait itself), `FUN_0014ddb0`/`FUN_0014e318` (after the step), the 41 unread functions of the Vsync unit.
- Thread `+0xf0..+0xf8` writers (the sleep and wait calls) not read.
- Game-mode list layout at `DAT_003990c0` (`+0xc` next, `+0x14` bits, `+0x1c` state) only seen from `FUN_00102070`.
