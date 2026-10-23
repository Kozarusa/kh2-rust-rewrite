# BDX: enemy AI bytecode

[Contract](contract.md) | [Index](../../README.md)

Findings for work packages 1E and 1E2. Everything here was read from the USA executable (`SLUS_210.05`) and from the BDX files on the
user's disc. Nothing from the disc is stored in the repository. Function addresses are virtual addresses in the executable.
"Verified" means the claim is checked bit for bit against the game's own interpreter (see Verification); "inferred" means read
from code or from how scripts use a function, but not independently checked.

## Where the files are

- Object scripts: BAR entry of **type 3** inside `obj/*.mdlx` (885 files). The entry tag is the first four characters of the
  object name (`m_ex`), the 16-byte name inside the entry is the full name (`m_ex020`). Shadow is `obj/M_EX020.mdlx`, Soldier
  `obj/M_EX010.mdlx`.
- Other type 3 entries: 283 in `*.bar` (mission and limit files), 79 in `magic/*.mag`, 43 in `ard/*.ard`.
- The standalone `00common.bdx` in the archive. The game loads it at start (FUN_00194d50).
- 1291 BDX entries on the disc: 1244 parse and analyse without error, 47 are empty (0 bytes: the object has no script), 0 fail
  (`kh2 bdx --scan`).

## File format (verified)

All values little endian. Code is 16-bit words; **word offsets count from file byte 0x10** (the interpreter reads
`base + 0x10 + 2 * pc`).

| bytes | meaning |
| --- | --- |
| 0x00 | 16-byte name |
| 0x10 | u32 global memory size (shared by all threads of the object) |
| 0x14 | u32 frame (call) stack size; the game adds 0x100 |
| 0x18 | u32 operand stack size; the game adds 0x100 |
| 0x1c | entry table: `{u32 event id, u32 word offset}` pairs, ended by a pair whose offset is 0 (FUN_001d7b78) |
| after | code and data (string pool, constants) in the same word space |

Entry offsets index the word array from file byte 0x10 (header words included), so the first code word is at word 6 + 4 * entries + 4.
Constants and strings are addressed from code through the "code" addressing mode and sit after the code.

## Instruction set (verified, FUN_001d7cb0)

Instruction word: bits 0-3 major opcode, bits 4-5 type (0 int, 1 float, 2/3 address forms), bits 6-15 sub-operation, or addressing mode, or a count.
All stack values are 32-bit words; floats are IEEE bit patterns. Operand words follow the instruction word.

Operand addressing (bits 6+ of the word, then one operand word `off`): 0 `frame + off`, 1 `globals + off`, 2 `*(frame[0]) + off`, 3 `code + 2*off`,
anything else is the plain address `off`. Frame = the running routine's locals; the callee's locals start right after the caller's frame words.

| major | meaning | stack effect |
| --- | --- | --- |
| 0, type 0/1 | push 32-bit constant (2 words follow) | +1 |
| 0, type 2 | push address of operand | +1 |
| 0, type 3 | push the word at the operand address | +1 |
| 1 | pop into the word at the operand address | -1 |
| 2 | `size` word, operand: pop a source address, copy `size` bytes to the operand address | -1 |
| 3 | `off` word: pop address, push the word at `address + off` | 0 |
| 4 | pop value, pop address; sub = 0: store word; sub = n: copy `n` bytes from `value` to `address` | -2 |
| 5 | unary. int: 0 to float, 2 neg, 3 not, 4/8 is-zero, 5 abs, 6 <0, 7 <=0, 9 !=0, 10 >=0, 11 >0. float: 1 to int, 2 neg, 5 abs, 6 <0, 7 <=0, 8 ==0, 9 !=0, 10 >=0, 11 >0 | 0 |
| 6 | binary on the two top values (second = left). int: 0 add, 1 sub, 2 mul, 3 div, 4 rem, 5 and, 6 or, 7 xor, 8 shl, 9 sar, 10 logical and, 11 logical or. float: 0 add, 1 sub, 2 mul, 3 div, 4 fmod | -1 |
| 7 | jump, one word `rel` (signed, relative to the word after the operand): sub 0 always, 1 if popped value is zero, 2 if non-zero | 0 / -1 |
| 8 | call, count in sub, one word `rel`: frame pointer += count words, the last two words of that frame hold return pc and count | 0 |
| 9 | sub 0 yield (run result 1), 1 exit (2), 2 return (pop frame; return pc 0 ends the thread, result 3), 3 pop, 5 dup, 6 sin, 7 cos (radians wrapped to -pi..pi), 8 degrees to wrapped radians, 9 radians to degrees | |
| 10 | host call: group in sub, index word. Pops the function's argument count (first pushed = first argument), pushes the result if the function has one | |
| 11-15 | invalid (the game ends the thread with result 5) | |

Comparisons are done by subtracting and applying a unary test. Threads are cooperative: `yield` ends the slice and the thread resumes after it
on the next update. Starting a thread zeroes four words of frame and pushes the event arguments on the operand stack in order (the handler pops
them last-first). **Opcodes not understood: none.** Over the 1244 scripts no invalid instruction word occurs on reachable code. Instruction counts on
the disc (reachable code, including the routines registered with mode records): push 332501, load 222466, lea 130874, store 121638, host call 159072,
jump 73671, call 66263, binary 78634, unary 35835, loadfield 33051, copyto 45112, dup 24162, ret 47156, yield 13972, store-indirect 14649, pop 11216,
sin 81, cos 42, deg-to-rad 690, rad-to-deg 28, exit 36, copy-indirect 534. (Routines passed to the thread-spawn host calls are followed too since 1E2, so the counts of a fresh `--scan` are slightly higher.)

The type bits of `ret` say what the routine returns: 0x89 nothing, 0xa9 an address, 0xb9 a word, 0x99 a float. They are not always honest (a routine that returns a host call's
result directly ends in 0x89); the signature inference below looks at the instruction before the `ret` too.

Float semantics: the console's FPU has no infinities or NaNs (overflow saturates to the largest normal number); this crate applies that clamp.
The interpreter's own pi is 0x40490fda and 2 pi is 0x40c90fda (one unit below the nearest floats); sin/cos use the SunPro-style kernels with the game's constants (`trig.rs`).

## How the game runs a script (verified unless noted)

- Per object: `FUN_00169a90` creates one thread, runs **event 0** (arg: the object pointer) as a short handler, then makes **event 1** the persistent main thread (obj+0x484). A script without event 1 has a finished main thread from the start.
- Every frame `FUN_0016a288` runs the main thread one slice (and its child threads, below); while it has not ended the **mode manager** (obj+0x300, `FUN_001869b8`) runs. The main thread counts as ended when its code has returned *and* it has no children left.
- Events are started by the game with `FUN_0016c620(thread, id, args, n)` and run to their first `yield`, then are discarded. Known ids from call sites: 0 init, 1 main,
  6 damage received (arg: the hit record; `FUN_001b21f0`), 0xd / 0xe event begin / end (`FUN_00182f68`, `FUN_00183080`), 0xb (3 args: the **reaction command** delivered to its target, `FUN_0016b420` from `FUN_00177c20` and `FUN_00170b50`, see `docs/limits-reactions.md`), 0xf (2 args), 0x10 (3 args), 0x13 (2 args: enemy clash, `FUN_001b2478`, own volume and other volume),
  0x17, 0x18, 0x19, 0x1a (`FUN_001b3d18`, `FUN_001b3cc8`, `FUN_001b3c70`), 0x1b (3 args). Meanings other than 0, 1, 6 are inferred from the names of the call sites only.
  Ids 2, 3 occur in `00common.bdx` (`FUN_001954f8`).
- **Modes.** A script registers named modes in a 0x80-record table in its globals (host 1.6; record = name pointer, priority u16, flags u16, nine routine words) and attaches the table and a context
  structure (host 1.7). Record routine slots: 0 main routine (runs as the mode's thread, argument = context), 1 on exit, 2 event begin, 3 event end, 4 damage hook (args context, hit record),
  6 message hook (args context, sender context), 7 on enter (run once at mode start); slot 8 is the clash hook (`FUN_00187130` from `FUN_001b2478`; args context, own volume, other volume); slot 5 is not used by anything found. Host 1.8 switches now (kills the running thread, runs exit hook, starts the new thread, runs the enter hook),
  1.9 queues a mode to start after the running one ends (only when its priority is back to 0), 1.10 tests "queue not empty or a mode running", 1.127 resets, 1.81 interrupts (stops the running mode and puts the named one at the front of the queue),
  1.167 switches only when the new mode's priority is at least the running one's. The manager runs the mode's thread each frame;
  a switch made inside a running mode thread lets the new thread run in the same frame. Record flag bit 1 of the high half disables switching while the mode runs; bit 0 clear means "ask the host before running" (`ModeMayStart`).
  Priority below 300 lets the host interrupt (`ModeInterrupted`). Implemented in `runtime.rs`.
- Context structure pointer: host functions of "type A" take the script context (word at +4 = the game object); "type B" take the object directly (scripts pass `ctx->obj` with `ldf +4`). `runtime.rs` resolves both and the `Host` only sees objects.

## Threads, messages and queues (1E2; read from FUN_0016c8d8, FUN_0016c680, FUN_0016c3f0, FUN_00187170 to FUN_00187250, FUN_00186f50)

- **Thread objects** (0x40 bytes, FUN_0016c230): +0 program, +4 pc, +8 frame pointer, +0xc operand stack pointer, +0x10 globals, +0x1c flags (bit 1 = ended), +0x20 tag, +0x24 cleanup routine, +0x28 next sibling, +0x2c first child, +0x30 last child, +0x38 owning object (whose live-thread count at +0x488 goes up and down with them).
- **Child threads.** Host 0.9 (FUN_0016d458; arguments tag, routine, cleanup routine, argument) and host 1.82 (FUN_00172df0; context, tag, routine, cleanup routine, argument, always on the object's main thread) start a thread
  under the thread that is running (0.9) or under the main thread (1.82) and return its handle. When the tag is non-zero every child that has all the tag's bits is killed first (FUN_0016c928). Children run right after their parent in every
  frame (FUN_0016c680 runs the thread, then each child, and removes children that have ended and have no children); threads a child starts attach to its parent. A thread that is destroyed (ended, killed, or its mode stopped) first runs its cleanup routine
  once, then its children are destroyed the same way (FUN_0016c3f0). A mode ends only when its thread and all its children have. Event handlers run as throw-away threads whose children attach to the main thread.
  Mode threads that are stopped are destroyed at the end of the mode manager's pass (FUN_0016ca48), not at once.
- **Request threads.** Host 2.20 (`FUN_0018e898` -> `FUN_001a4738`) starts a detached thread at a routine with one argument and links it into the game's global list at 0x1c95518; the task body `FUN_001a4428` runs every listed thread once per frame (`FUN_0016c680`) and destroys the ones that have ended (it does nothing while the pause flag 0x20 of 0x35d268 is set). Here they belong to the runtime of the object whose script started them and run in `Runtime::update_requests` (called by `update`). The routine and its argument are plain constants only when the script passes them so: Soldier's event 11 passes the routine number through two calls, so `analyze_with` (and the `listing` example) take extra routine starts by hand.
- **A thread whose code is an event (WP-7A).** FUN_0016c500(thread, event, args, n) looks the event's entry up (FUN_0016c4c0 -> FUN_001d7b78: the program's entry table) and hands it to FUN_0016c4d8, which clears the thread's flags (+0x1c, so an ended thread runs again) and calls FUN_001d7bb0 (an entry of 0 or less ends the thread through FUN_0016c7c8 instead). FUN_001d7bb0 sets the program counter, pushes four zero words on the frame stack (returning from the routine then ends the thread) and pushes the arguments (FUN_001d7b30, one word each, the first first). It does **not** reset the frame or the operand stack: restarting a live thread runs the new routine above whatever the old one left. The thread's children stay. The cast start FUN_001a4798 makes a thread on a spell's program (FUN_0016c9b8: FUN_0016c230 with no globals block, so the thread allocates its own, FUN_0016cad8) and points it at event 20 with four words (caster, the slot's resource record at table + 0xc, the cast's kind, the `magi` row); the cast trigger FUN_001a4a60 points it at event 21 with none. `Runtime::start_thread` / `restart_thread` / `thread_done` and `vm::Thread::restart` are these; `tests/runtime.rs` checks them on a hand-made script and, in `restart_oracle::spell_thread_restart_matches_the_games`, the four level-1 spell scripts against the game's FUN_001d7bb0 and FUN_001d7cb0 in kh2-ee (event 20 for 1, 2 and 5 slices, the restart, event 21 to its end: 3592 interpreter steps and 199 host calls, 0 differences).
- **Script heap.** Host 0.47 (`alloc`) and 0.48 (`free`, `FUN_0016ded8`) are served by the runtime itself (`runtime::HEAP_BASE`, a 64 KiB bump allocator that never reuses): the game hands out real heap memory, scripts only keep and fill it (Soldier's reaction thread builds its context there). The runtime's own reads of script memory (contexts, vectors) and the interpreter's copies see it; the host is never asked about heap addresses.
- **Action thread.** Host 1.112 (FUN_001764a8) starts a thread that is not a child of anything but belongs to the object (+0xaec): routine and an optional argument after the object. It replaces the earlier one (which is destroyed) and runs once per frame in the object class's own update (FUN_00176528) unless the object is frozen (+0x160 bit 0x40000);
  host 1.214 drops it. Where the class runs it relative to the script update (FUN_0016a288) is not known; `Runtime::update` runs it after the script and the two halves can be called separately.
- **Messages.** Host 1.90 (FUN_00187170: message name, delay, the sender's mode machine) and 1.241 (FUN_00187190: name, delay, no sender) append to a global 64-entry queue. A per-frame task (FUN_00187250, registered at 0x187398) hands every queued
  entry to *every* object: with a delay of zero or less the object's mode of that name (any mode of its table, running or not) gets its **message hook** (slot 6, arguments: its own context and the sender's context) at once; otherwise the entry goes into the object's single pending slot (mode machine +0xa0 name,
  +0xa4 sender, +0xa8 time; a newer message replaces an older one) with the time scaled by a random fraction from the game's second generator (`seed = seed * 0x10dcd` at 0x3716a0, fraction `seed * 2^-32`). The mode machine counts the slot down by one frame per update (FUN_001869b8) and delivers at zero. Objects without a mode of that name drop the message.
  `Runtime::receive_message` is the receiving end; the host collects `Call::SendMessage`, picks each receiver's delay with `message_delay` and calls it on every runtime. The sender's context is a pointer into the *sender's* script memory in the game; here the script gets a small stand-in whose word 1 is the sender's object, which is what scripts read from it (other fields are a gap).
- **Remote queue entries.** Host 6.58 (FUN_00186f50) queues a mode like 1.9 and sets bit 0 on the queue entry just written. When the entry comes up, FUN_00186900 does not start the mode but switches *another* object (FUN_002f4398(1): the object with entity id 0x81f that FUN_0016b940(object, 1) does not mark, not worked out further) to the mode of that name (FUN_00186db0) and tries the next entry. Here that is `Call::RemoteSetMode`, answered with `Runtime::set_mode_named` on the other object's runtime.
- **Object lookups.** Hosts 1.114, 1.143 and 1.210 write the object they find (by entity id, by the spawn record's byte +0x1e, by NeoStatus) into the context at +4; the interpreter does that write and asks the host with `Call::FindObject`.

## Host interface

Host functions are `group.index`. The tables are in the executable at 0x35bce8 (group pointers) with `{function, flags}` entries; flags low 16 bits = argument count, bit 30 = returns a value.
Groups 0, 1, 2, 4 to 8 and 10 are in the file; **group 3 (the Gummi ship scripts), group 9 (the Gummi menu scripts) and entries 0 and 1 of group 4 are empty in it**: the group pointer slots for 3 and 9 are zero and the two entries of group 4 are null. The only code that writes a group pointer is the two-instruction setter FUN_0016ca10 and its wrapper
FUN_0016ca28 (group 3), and nothing calls either: no jump or call to them exists in the executable, and none in any file on the disc (the whole archive was scanned for the call instruction, also inside BARs; every other file is data). The code that fills these tables is therefore not on the disc under any form this project can read.
`signatures.rs` recovers their signatures from the scripts instead (below). The calling convention
(FUN_001d7cb0): arguments go into a 16-word buffer, the native function receives a pointer to it and writes its result into word 0.
`kh2_bdx::HostTable::from_exe` reads the table from the user's executable; `names.rs` carries the argument kinds of the analysed functions and the tests check both agree.

`kh2_bdx::Host` has one method, `call(&Call) -> Value`, plus `read_mem`/`write_mem` for game memory a script holds pointers into (hit records, targets, buffers from 2.21). `Call` (host.rs) has a variant with typed fields for the functions Shadow and Soldier use (and the thread, message and mode ones), and
`Call::Func { group, index, args }` for every other named function: the arguments are decoded by the kind letters of `names::HostFn::kinds` (`O` context to object, `o` object, `i`, `f`, `v` vector, `s` string, `p` raw word) and `names::lookup(group, index)` gives the name, return kind and notes. A function not analysed arrives as `Call::Unknown { group, index, args }`.
Group 0 vector math, `fabs` and the random numbers are implemented inside the crate and need no host: `frame_time` returns 1.0 (the game's per-frame unit),
`vec_add/sub/scale` work on all four lanes, `vec_length`/`vec_normalize` on xyz (normalize stores the length in w and returns it), `vec_rotate_y`, and the generator `seed = seed * 0x10dcd` with
`rand_int(n) = seed / ceil(2^32 / n)`, `rand_float(max) = seed * 2^-32 * max`, `rand_range(a, b) = a + seed * 2^-32 * (b - a)` (all verified against the game's own routines). The functions the retail executable implements as an empty return (the debug prints 0.0, 0.1, 0.2, 0.8, 0.24 to 0.26, 0.60, 1.103, 1.122, 1.163, 1.240, 1.246, 2.36) do nothing here.
`atan2`, the angle wrap (an `fmodf`) and the functions that touch game state go to the host.

