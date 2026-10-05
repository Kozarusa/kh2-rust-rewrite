# BDX: enemy AI bytecode

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
  6 damage received (arg: the hit record; `FUN_001b21f0`), 0xd / 0xe event begin / end (`FUN_00182f68`, `FUN_00183080`), 0xb (3 args), 0xf (2 args), 0x10 (3 args), 0x13 (2 args),
  0x17, 0x18, 0x19, 0x1a (`FUN_001b3d18`, `FUN_001b3cc8`, `FUN_001b3c70`), 0x1b (3 args). Meanings other than 0, 1, 6 are inferred from the names of the call sites only.
  Ids 2, 3 occur in `00common.bdx` (`FUN_001954f8`).
- **Modes.** A script registers named modes in a 0x80-record table in its globals (host 1.6; record = name pointer, priority u16, flags u16, nine routine words) and attaches the table and a context
  structure (host 1.7). Record routine slots: 0 main routine (runs as the mode's thread, argument = context), 1 on exit, 2 event begin, 3 event end, 4 damage hook (args context, hit record),
  6 message hook (args context, sender context), 7 on enter (run once at mode start); slots 5 and 8 are not used by anything found. Host 1.8 switches now (kills the running thread, runs exit hook, starts the new thread, runs the enter hook),
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

## Named host functions

272 of the 812 distinct host functions called by scripts on the disc are named (73 before work package 1E2); the table has 298 entries counting 26 that no script on the disc calls. The named ones account for 86 percent of all host calls in scripts on the disc and
98.7 percent of the calls in the enemy and boss scripts (`obj/M_*`, `B_*`, `H_*`). Argument counts and return flags of every row are checked against the executable's table by a test (`signatures_match_the_executable`). Rows marked "role inferred" have their effect on the object read from the game code but
their game-level meaning guessed from how scripts use them; rows without that mark that only describe a field ("word at +0xaa8") say what the wrapper does, not what the field is for. "Native" is the address of the host function's wrapper in the executable, "uses" the number of call sites in reachable script code on the disc. Object fields are offsets in the game object.
Group 0 is the library (math, effect handles, flags), group 1 the scripted object, group 2 attacks and hit records (the layout of the hit record is in `docs/hit-reaction.md`), group 6 bosses and NPCs.

| id | signature | native | uses | what it does |
| --- | --- | --- | --- | --- |
| 0.0 | `debug_print_0(pp)` | 0x16cbd8 | 252 | debug print; the retail build's function is an empty return |
| 0.1 | `debug_print_1(pp)` | 0x16cbe0 | 193 | debug print stub (empty return) |
| 0.2 | `debug_print(p)` | 0x16cbe8 | 2484 | debug print stub (empty return), called from many scripts |
| 0.3 | `frame_time() -> f` | 0x16cc08 | 4926 | the per-frame unit, 1.0 (FUN_0016cc08 loads it from the executable's constants) |
| 0.4 | `vec_add(vv) -> v` | 0x16cc78 | 611 | add the four lanes |
| 0.5 | `vec_sub(vv) -> v` | 0x16cd00 | 6263 | subtract the four lanes |
| 0.6 | `vec_length(v) -> f` | 0x16ce90 | 917 | length of the xyz part |
| 0.7 | `vec_normalize(v) -> f` | 0x16cec0 | 2109 | normalise xyz in place and store the length in w; returns the length |
| 0.8 | `debug_print_8(pp)` | 0x16cfc8 | 19 | debug print stub (empty return) |
| 0.9 | `spawn_thread(iiii) -> h` | 0x16d458 | 75 | start a child thread of the running thread: tag, routine, cleanup routine, argument (FUN_0016d458; see runtime.rs) |
| 0.13 | `vec_rotate_y(vf) -> v` | 0x16cf08 | 997 | rotate around the y axis by an angle in radians |
| 0.14 | `progress_flag_set(i)` | 0x16d5a0 | 99 | set bit `id` of the progress flag array at 0x33f6b0 (id >> 10 selects a 1024-bit page) and run the page's side-effect list (FUN_0017ff60); 0xffff is invalid |
| 0.15 | `progress_flag_test(i) -> b` | 0x16d5b8 | 177 | test a progress flag (FUN_001802e0); 0xffff is false |
| 0.16 | `rand_int(i) -> i` | 0x16d638 | 234 | random integer below n from the script generator (seed * 0x10dcd) |
| 0.17 | `rand_float(f) -> f` | 0x16d6b8 | 1411 | random float in [0, max) |
| 0.18 | `rand_range(ff) -> f` | 0x16d718 | 1755 | random float in [a, b) |
| 0.21 | `angle_between(vv) -> f` | 0x16cfd0 | 724 | wrapped difference atan2(b.x, b.z) - atan2(a.x, a.z): the turn from direction a to direction b (FUN_0017bbc0 wraps) |
| 0.23 | `fabs(f) -> f` | 0x16cc48 | 222 | absolute value (clears the sign bit) |
| 0.24 | `debug_print_24(pp)` | 0x16cbf0 | 9 | debug print stub (empty return) |
| 0.25 | `debug_print_25(pp)` | 0x16cbf8 | 1 | debug print stub (empty return) |
| 0.26 | `debug_print_26(p)` | 0x16cc00 | 5 | debug print stub (empty return) |
| 0.31 | `fader_in(f)` | 0x16d838 | 8 | screen fader at 0x3564d0: mode 3 over `seconds * DAT_0037a24c` frames (FUN_0014a7c8); role inferred |
| 0.32 | `fader_out(f)` | 0x16d880 | 36 | screen fader at 0x3564d0: mode 1 over `seconds * DAT_0037a250` frames, flag 1 (FUN_0014a7f0); role inferred |
| 0.35 | `vec_scale_in_place(vf)` | 0x16d038 | 105 | multiply all four lanes of the vector by the scalar, in place |
| 0.36 | `vec_scale(vf) -> v` | 0x16cd88 | 569 | multiply all four lanes by a scalar into a new vector |
| 0.38 | `effect_set_position(pv)` | 0x16da90 | 78 | copy a vector into the effect's position (the 16 bytes behind FUN_001d9d00(handle)) |
| 0.41 | `effect_set_vec2(pv)` | 0x16db50 | 32 | copy a vector (through FUN_0017be00) into the second vector of the effect instance (the 16 bytes behind FUN_001d9d50(handle)); role unknown |
| 0.42 | `angle_of(v) -> f` | 0x16d060 | 219 | atan2(v.x, v.z), the yaw of a direction |
| 0.43 | `wrap_angle(f) -> f` | 0x16de78 | 140 | wrap an angle in radians into -pi..pi (FUN_0017bbc0) |
| 0.44 | `effect_stop(p)` | 0x16dba0 | 114 | stop the effect instance of an effect handle (FUN_001e0298 on handle+0x1c, if it has one) |
| 0.47 | `alloc(i) -> p` | 0x16dea8 | 212 | allocate `size` bytes from the game heap (vtable slot 0 of the allocator at 0x33e850) |
| 0.49 | `effect_alive(p) -> b` | 0x16dbd0 | 45 | bit 0 of the handle's halfword at +10 is clear (FUN_001da0d8) |
| 0.50 | `effect_valid(p) -> b` | 0x16dc00 | 74 | the handle has an effect instance (handle+0x1c != 0) |
| 0.51 | `effect_destroy(p)` | 0x16dc30 | 73 | mark the handle released (flags |= 0x10001), destroy its instance (FUN_001e0218, FUN_001e4218) and clear the pointer |
| 0.52 | `effect_fade(pfi)` | 0x16dc48 | 34 | fade the effect out over `frames` (rate = -(level at +0x20) / frames in +0x24, flag bits 0x20000, plus 1 when the last argument is non-zero, which also destroys it at the end); zero or fewer frames acts at once (FUN_001da080, FUN_001d9fd0) |
| 0.53 | `effect_position(p) -> v` | 0x16dc88 | 110 | pointer to the position vector of an effect handle (FUN_001d9d00) |
| 0.57 | `bits_33fe20_set(i)` | 0x16df30 | 15 | set bit `n` of the bit array at 0x33fe20 (FUN_001a7750) |
| 0.59 | `bits_33fe20_test(i) -> b` | 0x16df60 | 15 | test bit `n` of the bit array at 0x33fe20 (FUN_001a77b8) |
| 0.60 | `nop_60(p)` | 0x16df90 | 102 | empty function |
| 0.70 | `progress_flag_102b() -> b` | 0x16d5e8 | 8 | progress flag 0x102b (FUN_00180330) |
| 0.78 | `vec_lerp(vvf) -> v` | 0x16d180 | 29 | a * (1 - t) + b * t on all four lanes, result in a static buffer |
| 0.79 | `release_effect(p)` | 0x16de50 | 395 | hand an effect handle back to the game (queued for deletion) |
| 0.85 | `effect_stop_mark(p)` | 0x16dbb8 | 87 | like effect_stop and also sets bit 0 of the handle's flags (+8) (FUN_001d9f48) |
| 0.86 | `set_word_4(pp)` | 0x16de68 | 0 | store the second argument at +4 of the first |
| 0.88 | `fader_88(f)` | 0x16d958 | 1 | screen fader at 0x3564d0 with flag 0 over `seconds * DAT_0037a25c` frames (FUN_0014a7f0); role inferred |
| 1.0 | `create_object(i) -> h` | 0x170190 | 51 | construct a new object (0x9c0 bytes) for entity id `id` at the origin (FUN_0016e8c0); returns it |
| 1.1 | `set_yaw(Of)` | 0x170ac8 | 222 | rebuild the facing vector at +0x550 from an angle (FUN_0016a790), set the yaw at +0x66c, clear flag 8 of +0x7f8; role inferred |
| 1.3 | `player() -> o` | 0x170260 | 775 | the player's object (Sora) |
| 1.4 | `face_direction(Ov)` | 0x170ae8 | 1826 | face along the direction at once (yaw = atan2(x, z) at +0x66c) |
| 1.5 | `modes_clear(p)` | 0x172050 | 716 | clear the mode record table |
| 1.6 | `modes_register(psippppppppp)` | 0x172068 | 13941 | write a mode record: table, name, priority and flags, nine routine words |
| 1.7 | `modes_attach(pp)` | 0x170e60 | 716 | attach the mode table and the context structure to the object's mode machine |
| 1.8 | `set_mode(ps)` | 0x170ea0 | 15744 | switch to the named mode now |
| 1.9 | `queue_mode(ps)` | 0x170ec8 | 4202 | queue the named mode to start after the running one ends |
| 1.10 | `mode_pending(p) -> b` | 0x170f18 | 368 | a mode is queued or running |
| 1.11 | `play_motion(oif)` | 0x170348 | 4450 | start a motion with a blend (motion controller at +0x140, FUN_0017e220) |
| 1.12 | `play_motion_checked(oif)` | 0x1703a8 | 5605 | start a motion unless the game rejects the change (FUN_0017e2d8); role inferred |
| 1.13 | `queue_motion(oif)` | 0x1703d8 | 449 | append a motion to the object's motion queue (4 entries at motion controller +0x3c) with the given blend (FUN_0017e170) |
| 1.14 | `motion_done(o) -> b` | 0x170408 | 2085 | the current motion has finished (bit 1 of +0x160) |
| 1.15 | `motion_state(o) -> i` | 0x170450 | 3272 | the word at +0x154 |
| 1.18 | `link_start(OOiipp)` | 0x170dd8 | 401 | bind the object to another one (FUN_001beb90): other object at +0x570, two shorts at +0x574 and +0x576, clears the movement vector, links into the other's list at +0x760; when a routine is given it is registered with its argument on the new link record (+0x10, +0x14) |
| 1.19 | `fade_out(of)` | 0x170460 | 1090 | tween the alpha from 1 to 0 over the given frames |
| 1.20 | `fade_in(of)` | 0x1704a8 | 626 | tween the alpha from 0 to 1 over the given frames |
| 1.21 | `spawn_effect(Oiii) -> h` | 0x171780 | 25 | spawn an effect `id` on the object's effect manager (object +0x80) with two parameters, owner = the object (FUN_0016a530 -> FUN_001db868); returns the effect handle, 0 when the object has no manager |
| 1.22 | `spawn_effect_at(Oivii) -> h` | 0x1717c0 | 132 | as spawn_effect, then copy a vector into the new effect's position (FUN_0016a5b8) |
| 1.23 | `global_byte_33e830() -> i` | 0x1720b8 | 273 | byte at 0x33e830 |
| 1.24 | `global_byte_33e831() -> i` | 0x1720c8 | 68 | byte at 0x33e831 (the area number, read by FUN_001ad170) |
| 1.25 | `global_byte_33e833() -> i` | 0x1720d8 | 0 | byte at 0x33e833 |
| 1.26 | `global_byte_33e834() -> i` | 0x1720e8 | 45 | byte at 0x33e834 |
| 1.27 | `global_byte_33e835() -> i` | 0x1720f8 | 0 | byte at 0x33e835 |
| 1.28 | `hide(O)` | 0x170f78 | 1120 | hide the object at once |
| 1.29 | `play_motion_with(OOif) -> i` | 0x1715b8 | 206 | start a motion on the object in step with a second object (FUN_0017f160 on the motion controller at +0x140: motion id stored at +0x154); role inferred |
| 1.32 | `task_start(pi) -> h` | 0x172360 | 105 | allocate a task object (0xb0c bytes, vtable 0x35eb80) and a 0x1c byte listener, return the task (FUN_001ab6a8, FUN_001aad78); role inferred |
| 1.33 | `task_free(p)` | 0x1723f0 | 105 | call the task's virtual method +0x20 |
| 1.34 | `event_active() -> b` | 0x173888 | 517 | the word at 0x35e3a4 is non-zero: an event or pause is in progress |
| 1.35 | `set_bytes_33e86c(iii)` | 0x1721f8 | 0 | store three bytes at 0x33e86c, 0x33e86d, 0x33e86e |
| 1.36 | `mask_35a810_clear(i)` | 0x172418 | 315 | clear bit n of the 64-bit mask at 0x35a810 and run the slot-off hooks (FUN_0015e928) |
| 1.37 | `mask_35a810_set(i)` | 0x172430 | 222 | set bit n of the 64-bit mask at 0x35a810 and run the slot-on hooks (FUN_0015e9a8) |
| 1.38 | `set_team(Oi)` | 0x170950 | 12 | store the word at object +0x3b0 (the attack object constructor reads it as the owner's team, FUN_0018bd08) |
| 1.39 | `spawn_param(Oi) -> i` | 0x171b50 | 869 | word i of the array at object +0x81c (the spawn record); Shadow tests the value 99 |
| 1.42 | `bits_1d5d7b8_set(i)` | 0x170da8 | 17 | set bit n of the bit array at 0x1d5d7b8 (FUN_001b7ce8) |
| 1.43 | `bits_1d5d7b8_clear(i)` | 0x170dc0 | 15 | clear bit n of the bit array at 0x1d5d7b8 (FUN_001b7d18) |
| 1.48 | `task_done(p) -> b` | 0x1723d8 | 105 | the task's state word (+0x70) equals 3 |
| 1.52 | `bits_47c_set(Oi)` | 0x1728c0 | 177 | set bit n of the bit array at object +0x47c (FUN_0019dd88) |
| 1.53 | `bits_480_set(Oi)` | 0x1728e8 | 334 | set bit n of the bit array at object +0x480 (FUN_0019ddb0) |
| 1.54 | `bits_47c_480_clear(Oi)` | 0x172910 | 293 | clear bit n in both bit arrays (FUN_0019dde0) |
| 1.55 | `start_move(Ovifi)` | 0x1712d0 | 257 | set the direction, start the motion and, with the last flag, record motion + 1 and + 2 as the following motions |
| 1.56 | `on_screen(O) -> b` | 0x172938 | 409 | the object is inside the camera frustum |
| 1.57 | `airborne(O) -> b` | 0x172968 | 342 | bit 0 of the mover's flag word; role inferred |
| 1.58 | `set_dash(Ovf)` | 0x172988 | 527 | store the direction scaled by its w component at object +0x20 and the argument at +0xf0, then run the mover call with state 5 (FUN_0019e6b8, FUN_0016b698); role inferred |
| 1.59 | `mover_bit1_or_busy(O) -> b` | 0x1729c0 | 36 | mover flag bit 1 (mover at +0xc, word +8) or object flag 0x40 at +0x160 |
| 1.60 | `busy(O) -> b` | 0x1729f8 | 1274 | the object is in a hit reaction, knocked or locked (FUN_001729f8); role inferred |
| 1.61 | `play_motion_at(oiff)` | 0x170378 | 657 | start a motion at a frame with a blend |
| 1.62 | `obj_vec_860(O) -> v` | 0x172a98 | 132 | the vector at object +0x860; role inferred |
| 1.63 | `motion_near_end(O) -> b` | 0x172b00 | 149 | the motion controller (+0x140) has no blend left (+0x34 <= 0) and the remaining time is at most +0x30 (FUN_0017f400); role inferred |
| 1.64 | `obj_vec_850(O) -> v` | 0x172b38 | 66 | the vector at object +0x850; role inferred |
| 1.65 | `set_flag_26(O)` | 0x172ba0 | 236 | set bit 26 of the flag word at object +0x108 |
| 1.66 | `set_flag_27(O)` | 0x172bc0 | 184 | set bit 27 of the flag word at object +0x108 (1.65 sets bit 26) |
| 1.67 | `motion_length(Oi) -> f` | 0x172be0 | 620 | length in frames of a motion (-1: the running one); role inferred |
| 1.68 | `motion_end(Oi) -> f` | 0x172c18 | 10 | end frame of a motion (-1: the running one); role inferred |
| 1.69 | `obj_f_170(O) -> f` | 0x172c50 | 196 | float at object +0x170 |
| 1.70 | `set_flag(Oi)` | 0x172c68 | 2024 | set bit n of the flag array at object +0x108 |
| 1.71 | `clear_flag(Oi)` | 0x172ca0 | 508 | clear bit n of the flag array at object +0x108 |
| 1.72 | `test_flag(Oi) -> b` | 0x172ce0 | 0 | test bit n of the flag array at object +0x108 (1.70 sets, 1.71 clears) |
| 1.74 | `mover_reset(O)` | 0x171548 | 384 | call the mover's (+0xc) virtual method +4 with the object, then set mover state 0 (FUN_0016b698 with 0; the object-list test in front of it has no effect); role inferred: stop the movement |
| 1.77 | `has_word_144(O) -> b` | 0x172d60 | 50 | the word at object +0x144 is non-zero |
| 1.78 | `hit_stun_b(O) -> b` | 0x172d78 | 114 | bit 9 of +0x160 (the same test as 2.5) |
| 1.79 | `face_toward(Ov)` | 0x172d98 | 977 | turn toward a direction (abrupt unless the object is locked, then the target direction is stored) |
| 1.80 | `turn_toward(Ovf) -> b` | 0x172db8 | 2097 | turn toward a direction at most `speed` per frame (0 = the object's own rate); true while still turning |
| 1.81 | `interrupt_mode(ps)` | 0x170ef0 | 0 | stop the running mode and put the named mode at the front of the queue so it starts on the next pass; priority drops to 0 (FUN_00186fa8) |
| 1.82 | `spawn_child_thread(piiii) -> h` | 0x172df0 | 14 | start a child thread of the object's persistent thread: context, tag, routine, cleanup routine, argument (FUN_00172df0) |
| 1.83 | `bone_to_world(Oiv) -> v` | 0x172e68 | 151 | transform a vector by the matrix of bone/joint `index` of the object (FUN_0016aa18); result in a static buffer |
| 1.84 | `gauges(O) -> p` | 0x172ee8 | 839 | the pointer at object +0x48c; scripts pass it to the record readers 1.228 to 1.233 |
| 1.87 | `spawn_attached(Oiii) -> h` | 0x171800 | 1541 | spawn an effect or child attached to the object; returns its handle |
| 1.88 | `lockon_point(O) -> v` | 0x172f40 | 21 | the object's lock-on point through a temporary target record (FUN_0016b6e0); role inferred |
| 1.89 | `set_velocity(Ovf)` | 0x172f90 | 78 | set the secondary velocity vector at +0x880 (direction normalised, speed in w) |
| 1.90 | `send_message(Osf)` | 0x172fb8 | 366 | send the name of a mode to the mode machines of all objects after a delay (see runtime.rs) |
| 1.92 | `ground_marker(Oi)` | 0x171078 | 466 | place a marker effect on the ground under the object |
| 1.94 | `obj_valid(o) -> b` | 0x170150 | 1034 | the object is in the object list and FUN_0019a440 clears; 0 is false |
| 1.96 | `mover_word_4(O) -> p` | 0x173058 | 0 | word at +4 of the object's mover (+0xc) |
| 1.97 | `obj_part_110(O) -> p` | 0x173070 | 69 | address object +0x110 |
| 1.98 | `ground_position(O) -> v` | 0x173088 | 552 | position of the object with y replaced by the ground height under it (FUN_00183808); static buffer |
| 1.99 | `ground_depth(O) -> f` | 0x1730e8 | 108 | minus the depth of the object below the ground (0 when above it): -max(0, ground - y) |
| 1.101 | `flag_160_4000(O) -> b` | 0x1716d0 | 79 | object flag 0x4000 at +0x160 (after the object-valid check) |
| 1.102 | `link_end(O)` | 0x170e38 | 56 | undo link_start (FUN_001becb8) |
| 1.103 | `nop_103(ppp)` | 0x170e58 | 0 | empty function |
| 1.104 | `set_flag_768(O)` | 0x1731a8 | 9 | set bit 6 of the word at +0x768 |
| 1.105 | `clear_flag_768(O)` | 0x1731c0 | 9 | clear bit 6 of the word at +0x768 |
| 1.107 | `set_f_234(Of)` | 0x173260 | 78 | store a float at object +0x234 (564) |
| 1.108 | `link_method_3c(Op)` | 0x173278 | 173 | virtual method +0x3c of the object behind the link record (record at object +0x670, its +0x14) with the record's word +0x104 and the argument (FUN_0011c580) |
| 1.109 | `link_method_40(Op)` | 0x173298 | 303 | the same with virtual method +0x40 (FUN_0011c5b0) |
| 1.110 | `spawn_kind(O) -> i` | 0x1732b8 | 472 | spawn kind byte of the object's spawn record, -1 without one |
| 1.111 | `set_move(Ovf)` | 0x1732e8 | 946 | set the movement vector at +0x10: direction normalised, speed in w |
| 1.112 | `start_action(Oii)` | 0x171120 | 350 | start the object's action thread (see runtime.rs) |
| 1.114 | `bind_object(pi)` | 0x171d10 | 554 | set the context's object pointer (+4) to the object with entity id `id` (FUN_0016b078; 0x236 is the player) |
| 1.115 | `set_words_100_104(Oi)` | 0x171388 | 2 | object +0x100 = v + 1 and +0x104 = v + 2 |
| 1.119 | `probe_ground(Ovff) -> i` | 0x173648 | 113 | collision probe along a direction; returns 0 (clear) to 3 |
| 1.120 | `target_position(p) -> v` | 0x173688 | 1771 | world position of a target record |
| 1.121 | `set_target_mode(pOi)` | 0x1736d0 | 489 | set the lock mode of a target record (0 to 6 pick the game's lock handlers) |
| 1.122 | `nop_122(p)` | 0x1737f0 | 8 | empty function |
| 1.124 | `flag_124(O) -> b` | 0x173828 | 939 | flag 8 of the object's collision word (FUN_0019aaf0); role unknown |
| 1.125 | `tween_start(Offf)` | 0x173858 | 35 | start a linear ramp of the value pair at object +0x8c8 (value, step) from `from` to `to` over `frames` (FUN_001abf40) |
| 1.127 | `modes_reset(p)` | 0x170e80 | 23 | stop the running mode and clear the queue and priority |
| 1.128 | `mover_method(Of)` | 0x171298 | 250 | virtual call on the object's mover (+0xc) with one float; role unknown |
| 1.130 | `obj_diff(OO) -> i` | 0x170510 | 155 | difference of the two contexts' objects, 0 when either is 0 or they are the same |
| 1.131 | `free_target(p) -> b` | 0x1736f8 | 279 | free a target record (FUN_0018f3f8); role inferred |
| 1.132 | `target_release(p)` | 0x173728 | 68 | return a target record to the pool at 0x1c89bd0 (20-byte entries; FUN_0018f458) |
| 1.133 | `obj_method_44(O)` | 0x170540 | 97 | virtual method +0x44 of the object; role unknown |
| 1.134 | `obj_method_48(O)` | 0x170570 | 68 | virtual method +0x48 of the object; role unknown |
| 1.140 | `target_valid(p) -> b` | 0x173740 | 256 | the target record is usable (FUN_0018ed58); role inferred |
| 1.143 | `bind_object_143(pi)` | 0x171d40 | 36 | set the context's object pointer from FUN_0016b0d8(arg) |
| 1.145 | `obj_word_aa8(O) -> i` | 0x1705d0 | 392 | word at object +0xaa8 |
| 1.147 | `position(O) -> v` | 0x170610 | 8507 | world position of the object |
| 1.148 | `set_position(Ov)` | 0x170658 | 267 | set the position (w forced to 1.0) and sync the mover (FUN_0016a618) |
| 1.149 | `spawn_effect_for(OiOii) -> h` | 0x171840 | 21 | as spawn_effect with another object as the owner (FUN_0016a570) |
| 1.150 | `spawn_bit_238(Oi) -> b` | 0x171600 | 198 | bit n-1 of the byte at object +0x238 |
| 1.151 | `spawn_bit_239(Oi) -> b` | 0x171628 | 471 | bit n-1 of the byte at object +0x239 |
| 1.154 | `gflags_set_2()` | 0x1739f8 | 1 | set bit 1 of the global flag word at 0x35d268 (tested by 1.171 for bit 2 and 1.206 for bit 7) |
| 1.155 | `obj_method_78(Oiii) -> i` | 0x171c48 | 46 | virtual method +0x78 of the object with the arguments (the last as a boolean) |
| 1.157 | `spawn_effect_on(OiO)` | 0x171f40 | 83 | spawn an effect (FUN_001971c0 for the id) anchored at the object's +0x80, optionally bound to a second object (FUN_00197230) |
| 1.161 | `has_link(O) -> b` | 0x1706c8 | 321 | the object is linked to another (+0x570 non-zero and in the object list) |
| 1.163 | `nop_163(pp)` | 0x1737d0 | 0 | empty function |
| 1.166 | `ctx_word_8(p) -> i` | 0x1737e0 | 13 | the word at +8 of the argument |
| 1.167 | `set_mode_priority(ps)` | 0x170f50 | 16 | switch to the named mode when its priority is at least that of the running one (FUN_00186e70) |
| 1.168 | `start_move_alt(Ovifi)` | 0x171460 | 112 | as start_move through the alternate setter (FUN_001b4ee0) |
| 1.170 | `spawn_record_id(O) -> i` | 0x171b30 | 335 | halfword at +2 of the record at ((object +0x818) +8); role inferred |
| 1.171 | `global_flag_4() -> b` | 0x173a28 | 371 | bit 2 of the global flag word at 0x35d268 |
| 1.176 | `tween_to_zero(Oi)` | 0x1719a8 | 456 | ramp the value at object +0x4b0 to 0 over `frames` (FUN_001aec50) |
| 1.179 | `set_word_100(Oi)` | 0x1713a8 | 29 | store at object +0x100 |
| 1.180 | `set_word_104(Oi)` | 0x1713c0 | 23 | store at object +0x104 |
| 1.182 | `bits_10_set(pi)` | 0x173bf0 | 27 | set bit n of the bit array at +0x10 of the argument |
| 1.187 | `gflags_set_8()` | 0x173a40 | 24 | set bit 3 of the global flag word at 0x35d268 |
| 1.188 | `gflags_clear_8()` | 0x173a58 | 24 | clear bit 3 of the global flag word at 0x35d268 |
| 1.191 | `ptr_b40_4(O) -> p` | 0x1706f8 | 4 | the pointer at object +0xb40 plus 4 |
| 1.192 | `flag_160_10(O) -> b` | 0x171650 | 3 | object flag 0x10 at +0x160 |
| 1.195 | `link_call_195(Op)` | 0x171270 | 11 | FUN_0011c648 on the link record at object +0x670 with the argument |
| 1.196 | `global_byte_33e832() -> i` | 0x172108 | 128 | byte at 0x33e832 |
| 1.200 | `set_field_600(Oi)` | 0x171b00 | 7 | bits 3 to 6 of the word at object +0x600 = value & 0xf |
| 1.201 | `facing_vector(O) -> v` | 0x170710 | 1709 | facing vector of the object |
| 1.202 | `entry_flag1_set(i)` | 0x174088 | 67 | set bit 0 of the flags (+4) of the entry whose halfword at +2 equals the argument in the table at 0x1c8af90 (0x38-byte entries, count at 0x35e468; FUN_00199c48); role inferred |
| 1.203 | `entry_flag1_clear(i)` | 0x1740a0 | 49 | clear bit 0 of the same entry's flags (FUN_00199c70) |
| 1.204 | `gflags_set_80()` | 0x173a70 | 5 | set bit 7 of the global flag word at 0x35d268 |
| 1.205 | `gflags_clear_80()` | 0x173a88 | 6 | clear bit 7 of the global flag word at 0x35d268 |
| 1.206 | `global_flag_80() -> b` | 0x173aa0 | 377 | bit 7 of the global flag word at 0x35d268 |
| 1.209 | `obj_event_5c(O)` | 0x171c90 | 130 | unless object flag 4 at +0x7f8 is set, call the object's virtual method +0x5c (FUN_00186530) |
| 1.210 | `bind_object_210(pi)` | 0x171d70 | 39 | set the context's object pointer from FUN_0016b138(arg): the object whose FUN_001699a8 value matches |
| 1.211 | `obj_flags_abc_set(Oi)` | 0x170760 | 383 | set bit n of the word at object +0xabc |
| 1.212 | `obj_flags_abc_clear(Oi)` | 0x170788 | 139 | clear bit n of the word at object +0xabc |
| 1.214 | `stop_action(O)` | 0x171180 | 22 | drop the object's action thread (see runtime.rs) |
| 1.217 | `set_position_1000(Ov)` | 0x1706a8 | 40 | as set_position and set bit 12 of the flags at +0x7f8 (FUN_0016a670) |
| 1.218 | `set_spawn_param(OOi)` | 0x171b88 | 80 | store at object +0x81c + 4 * index (FUN_0016b950; the first argument is validated) |
| 1.222 | `objentry_word0(O) -> i` | 0x170808 | 340 | first word of the object's object-entry row (+8) |
| 1.224 | `mover_flag_1_or_2(O) -> b` | 0x171580 | 0 | bit 0 or bit 1 of the mover's flag word (+8) |
| 1.225 | `mover_method_60(O) -> p` | 0x1708f8 | 79 | when object flag 2 at +0x588 is set: result of the mover's virtual method +0x60, else 0 |
| 1.227 | `mover_method_8(Op)` | 0x171350 | 45 | virtual method +8 of the object's mover (+0xc) with the object and the argument |
| 1.228 | `rec12_word0(pi) -> i` | 0x1740e0 | 189 | word 0 of record i of an array of 12-byte records |
| 1.229 | `rec12_word1(pi) -> i` | 0x174100 | 134 | word 1 of record i of an array of 12-byte records |
| 1.230 | `rec12_ratio(pi) -> f` | 0x174140 | 159 | float(word 0) / float(word 1) of record i: current over maximum of a gauge |
| 1.231 | `rec12_set_word2(ppi)` | 0x1741c0 | 141 | store into word 2 of record i (arguments: base, value, index) |
| 1.232 | `rec12_word2(pi) -> i` | 0x174120 | 60 | word 2 of record i of an array of 12-byte records |
| 1.233 | `rec12_set_clamped(ppi)` | 0x174178 | 13 | word 0 of record i = the value clamped between word 2 (lower) and word 1 (upper) (arguments: base, value, index) |
| 1.234 | `obj_word_af8(Oi) -> i` | 0x174280 | 18 | word at object +0xaf8 + 4 * i |
| 1.238 | `obj_word_158(O) -> i` | 0x171718 | 24 | word at object +0x158 |
| 1.239 | `spawn_flag_8_or_none(O) -> b` | 0x171bc8 | 8 | the object has no spawn record (+0x818) or the record's flag 8 (+4) is set |
| 1.240 | `nop_240(ppp)` | 0x1710d8 | 0 | empty function |
| 1.241 | `broadcast_message(sf)` | 0x172098 | 0 | like send_message without a sender |
| 1.246 | `nop_246()` | 0x1737d8 | 0 | empty function |
| 1.247 | `random_free_point(vff) -> v` | 0x1726d8 | 68 | search around a position for a free point between two radii: samples a spiral with the game's second random generator and tests collisions (FUN_001a6068, FUN_001a6198); returns the point in a static buffer; role inferred |
| 1.250 | `set_scale_250(f)` | 0x173af8 | 16 | store the argument at 0x1c8a7e8 and the argument times the float at 0x360130 + 0x50 at 0x1c8a7ec (FUN_00190808) |
| 1.254 | `has_ptr_810(O) -> b` | 0x170ce0 | 4 | the pointer at object +0x810 is non-zero |
| 1.262 | `obj_method_54(Opp)` | 0x1709c8 | 396 | virtual method +0x54 of the object with two arguments |
| 1.264 | `gflags_clear_2()` | 0x173a10 | 1 | clear bit 1 of the global flag word at 0x35d268 |
| 1.265 | `set_word_ac8(Oi)` | 0x171f98 | 2 | store a word at object +0xac8 |
| 1.270 | `motion_register(Oi)` | 0x171730 | 41 | enter a motion id into the 16-entry list at the motion controller +0x14c unless it is already there (FUN_0017fb28) |
| 1.272 | `flag_7f8_4(O) -> b` | 0x171cb0 | 11 | flag 4 of the word at object +0x7f8 |
| 1.286 | `gflags_set_1000()` | 0x173b10 | 1 | set bit 12 of the global flag word at 0x35d268 |
| 1.287 | `gflags_clear_1000()` | 0x173b28 | 1 | clear bit 12 of the global flag word at 0x35d268 |
| 1.288 | `gflags_set_2000()` | 0x173b40 | 0 | set bit 13 of the global flag word at 0x35d268 |
| 1.289 | `gflags_clear_2000()` | 0x173b58 | 0 | clear bit 13 of the global flag word at 0x35d268 |
| 1.301 | `global_byte_301() -> i` | 0x1744f8 | 458 | the byte at 0x35ea48 + 0x1e (FUN_001abee8); role unknown |
| 1.309 | `flag_309(o) -> b` | 0x170288 | 86 | status bit 0x80 at +0x588 after a validity check; role unknown |
| 1.310 | `objentry_f_50(O) -> f` | 0x170b18 | 0 | float at +0x50 of the object's object-entry row |
| 1.311 | `set_scaled_byte(pii)` | 0x174238 | 153 | byte at +0x1a8 + i of the first argument = (halfword i of the table at its +0x254 + 0x48) * percent + 99) / 100 (FUN_001a0ba0) |
| 1.317 | `gflags_set_20000()` | 0x173b70 | 56 | set bit 17 of the global flag word at 0x35d268 |
| 1.324 | `obj_flags_abc_clear_b(Oi)` | 0x1707b0 | 17 | the same function as obj_flags_abc_clear under a second number |
| 1.335 | `list_800_remove(Op)` | 0x170d38 | 41 | remove the entry with that value from the list at object +0x800 (FUN_001b1750) |
| 1.336 | `list_800_add(Op)` | 0x170d60 | 41 | add a value to the list at object +0x800 (FUN_001b1810) |
| 1.344 | `list_800_clear(O)` | 0x170d88 | 0 | empty the list at object +0x800 |
| 1.350 | `gflags_set_100000()` | 0x173b88 | 1 | set bit 20 of the global flag word at 0x35d268 |
| 1.351 | `gflags_clear_100000()` | 0x173ba0 | 1 | clear bit 20 of the global flag word at 0x35d268 |
| 2.2 | `react_flinch_push(Opf)` | 0x18df00 | 11 | reaction class 3: flinch plus push on the object (FUN_001850c8; see hit-reaction.md) |
| 2.3 | `react_push(Opf)` | 0x18df28 | 24 | reaction class 1: push only (FUN_00185098, sets flag 1 at +0x18) |
| 2.5 | `hit_stun(O) -> b` | 0x18df80 | 1243 | bit 9 of the object's status word (+0x160) |
| 2.6 | `in_reaction(O) -> b` | 0x18dfa0 | 25 | the object's motion state (+0x154) is between 10 and 39 (FUN_00185a08) |
| 2.7 | `hit_reaction(p) -> i` | 0x18e0d0 | 50 | byte at hit +0x26: the effective reaction |
| 2.8 | `hit_is_real(p) -> b` | 0x18e0f0 | 215 | the hit record's reaction class is 2 or more (byte +0x26 through FUN_0018b700) |
| 2.9 | `set_obj_param(Oii)` | 0x18de30 | 264 | set an attribute pair on the object (FUN_001b2148); role inferred |
| 2.10 | `attack_create(Oiii) -> h` | 0x18e2f0 | 222 | create an attack object owned by the object: attack table row key (arg 1, arg 3), hit group id (arg 2, -1 takes a new one) (FUN_001a2ba0, FUN_0018bd08) |
| 2.11 | `attack_set_size(pff)` | 0x18e348 | 239 | attack +0x100 = a, +0x104 = b when b is positive else a (FUN_001a30e0) |
| 2.12 | `attack_move_to(pv)` | 0x18e3a0 | 202 | move the attack object to a position, keeping the previous one for the swept test (FUN_001a2fc8) |
| 2.13 | `attack_arm(p)` | 0x18e3c0 | 232 | set flag 0x800 at attack +4 when the attack object exists (FUN_0018dc98) |
| 2.15 | `hit_set_flag1(p)` | 0x18e190 | 136 | set bit 0 of the flags at hit +0x18 |
| 2.16 | `hit_flag1(p) -> b` | 0x18e1a8 | 0 | bit 0 of the flags at +0x18 (2.15 sets it) |
| 2.17 | `attack_hit_object(pOip)` | 0x18e6e0 | 61 | when the attack may hit the object (FUN_0018c5f8): build the hit record (FUN_0018c6d0), mark it, call the object's virtual method +100 with it (delivers the damage) |
| 2.20 | `request_2_20(pp) -> i` | 0x18e898 | 269 | sound or effect request (FUN_001a4738); role unknown |
| 2.21 | `shared_block(ii) -> p` | 0x18e928 | 156 | the persistent buffer registered under a key (FUN_001a5040): the first call allocates and zeroes size + 1 bytes, every call sets the first byte to 1 and returns the buffer |
| 2.22 | `attack_anchor(pO)` | 0x18e7c8 | 67 | set the attack's anchor (+0x24) to the other object's +0x80 |
| 2.23 | `target(O) -> p` | 0x18ddb0 | 3225 | the object's current target record (+0x9f4) |
| 2.24 | `hit_attacker_block(p) -> p` | 0x18e7e0 | 21 | the word at hit +0xc: pointer to a 16-byte block whose +4 holds the attacker's object (copied by scripts into a context); role inferred |
| 2.25 | `hit_attack_id(p) -> i` | 0x18e7f0 | 119 | the word at +0x14 (compared by scripts with an attack id) |
| 2.26 | `attack_set_flag_40(p)` | 0x18e800 | 0 | set bit 6 of the flags at +4 |
| 2.29 | `attack_flag_8_or_10(p) -> b` | 0x18e420 | 2 | bit 3 or bit 4 of the flags at +4 |
| 2.30 | `attack_set_anchor_raw(pp)` | 0x18e450 | 55 | store a pointer at attack +0x24 |
| 2.32 | `hit_knockback1(p) -> f` | 0x18e138 | 2 | KnockbackStrength1 of the hit's atkp row (halfword at row +0xc) as a float |
| 2.33 | `hit_knockback2(p) -> f` | 0x18e158 | 2 | KnockbackStrength2 of the atkp row (halfword at +0xe) as a float |
| 2.34 | `attack_atkp_type(p) -> i` | 0x18e4b8 | 0 | Type byte of the attack object's atkp row (pointer at +0x28, byte +4) |
| 2.35 | `hit_atkp_type(p) -> i` | 0x18e178 | 2 | Type byte of the hit's atkp row (+4) |
| 2.36 | `nop_2_36(pp)` | 0x18dfd0 | 5 | empty function |
| 2.38 | `attack_callback_8c(pii)` | 0x18e4f0 | 0 | store (current thread, routine, argument) as a callback record at +0x8c |
| 2.39 | `attack_flag_200(p) -> b` | 0x18e518 | 2 | bit 9 of the flags at +4 |
| 2.40 | `attack_flag_8(p) -> b` | 0x18e530 | 0 | bit 3 of the flags at +4 |
| 2.41 | `attack_flag_10(p) -> b` | 0x18e548 | 0 | bit 4 of the flags at +4 |
| 2.42 | `attack_ptr_b0(p) -> p` | 0x18e560 | 0 | address +0xb0 of the attack object |
| 2.44 | `attack_word_84(p) -> i` | 0x18e5d8 | 0 | word at +0x84 |
| 2.45 | `hit_atkp_id(p) -> i` | 0x18e1c0 | 198 | Id of the hit's atkp row (halfword at +2) |
| 2.46 | `hit_word_28(p) -> i` | 0x18e1d8 | 16 | word at +0x28: the damage of a hit record, the atkp row pointer of an attack object |
| 2.48 | `hit_owner(p) -> p` | 0x18e9c8 | 25 | the word at +0xc of an attack object (its owner) |
| 2.49 | `attack_word_10(pi) -> i` | 0x18e9d8 | 28 | word at +0x10 + 4 * i |
| 2.52 | `attack_word_110(p) -> i` | 0x18e618 | 13 | word at +0x110 |
| 2.55 | `attack_word_38(p) -> i` | 0x18ea50 | 36 | word at +0x38 |
| 2.56 | `ptr_b40_4_b(O) -> p` | 0x18de58 | 8 | the same function as ptr_b40_4 under a second number |
| 2.60 | `attack_f_d4(p) -> f` | 0x18eb40 | 29 | float at +0xd4 |
| 2.61 | `attack_set_c4(pp)` | 0x18e628 | 43 | store at attack +0xc4 (checked: attack object exists) (FUN_0018e628) |
| 2.62 | `attack_set_shape2(pfff)` | 0x18e680 | 85 | second shape of the attack: +0xcc = a, +0xd0 = b (default +0x100 when not above 0), +0xd4 = c (default +0x104) (FUN_001a3480) |
| 2.63 | `attack_word_3c(p) -> i` | 0x18eb50 | 25 | word at +0x3c |
| 2.64 | `hit_reaction_original(p) -> i` | 0x18e0e0 | 59 | byte at hit +0x27: the attack row's original EnemyReaction |
| 2.66 | `attack_byte_a4(p) -> i` | 0x18e818 | 39 | byte at +0xa4 |
| 2.67 | `new_hit_group() -> i` | 0x18e828 | 24 | next hit group id from the counter at 0x1d5dd4c (wraps at 0x10000000 to 0x100) (FUN_001bdf00) |
| 2.70 | `hit_byte_25(p) -> i` | 0x18e1e8 | 1 | byte at hit +0x25 |
| 2.72 | `hit_atkp_type_5_or_6(p) -> b` | 0x18e1f8 | 13 | Type of the atkp row is 5 or 6 |
| 2.74 | `attack_callback_98(pii)` | 0x18e870 | 1 | as attack_callback_8c, at +0x98 |
| 2.76 | `set_f_af4(Of)` | 0x18e068 | 52 | store a float at object +0xaf4 |
| 2.77 | `vec_new(pv) -> h` | 0x18ebd0 | 12 | allocate 0x40 bytes and fill it with a vector and a value (FUN_001d1808); role inferred |
| 2.78 | `free_alloc(p)` | 0x18ec18 | 16 | release an allocation made by 2.77 (FUN_001d18e0, free) |
| 2.79 | `attack_flags_10_set(pi)` | 0x18ec50 | 11 | set bit n of the word at +0x10 |
| 2.80 | `vec_copy(pp)` | 0x18ec70 | 0 | copy the 16 bytes at the second pointer to the first |
| 2.81 | `attack_set_f_18_1c_20(pfff)` | 0x18ec90 | 3 | store floats at +0x18, +0x1c, +0x20 |
| 2.82 | `attack_set_f_24(pf)` | 0x18ecb0 | 7 | store a float at +0x24 |
| 2.83 | `attack_set_f_28(pf)` | 0x18ecc0 | 6 | store a float at +0x28 |
| 2.84 | `attack_set_f_2c(pf)` | 0x18ecd0 | 7 | store a float at +0x2c |
| 2.85 | `hit_atkp_element(p) -> i` | 0x18e218 | 43 | Element byte of the hit's atkp row (+9) |
| 2.87 | `react_front_back(Opf)` | 0x18ded8 | 1 | reaction class 14: motion 0x17 (front) or 0x18 (back) plus push (FUN_001856a8) |
| 2.88 | `last_attacker(O) -> o` | 0x18e080 | 46 | the last object that hit this one (+0xae4), the player when there is none |
| 2.91 | `hit_reaction_class(p) -> i` | 0x18e260 | 11 | reaction class of the hit (FUN_0018b700 on the byte at +0x26) |
| 2.92 | `hit_kind(p) -> i` | 0x18e290 | 21 | FUN_0018b748 of the byte at hit +0x27 |
| 6.2 | `player_word_b50() -> i` | 0x2ee668 | 8 | word at +0xb50 of the player object (pointer at 0x35c998) |
| 6.24 | `obj_vec_8a0(O) -> v` | 0x2ee960 | 12 | the 16 bytes at object +0x8a0 copied to a static buffer |
| 6.58 | `queue_mode_remote(ps)` | 0x2ef018 | 3 | queue the named mode and mark the entry for another object: when it comes up the host switches that object to the mode instead (FUN_00186f50; see runtime.rs) |

### Not named

The rest are listed as `group.index xN` (N = call sites on the disc). They keep their numbers in listings and their argument counts come from the executable's table at run time; groups 3 and 9 and 4.0, 4.1 get theirs from inference.
Group 5 is the NPC routine library (`*_RTN` scripts), group 4 the mission and area script library (events, cutscenes), groups 6, 7, 8 and 10 bosses and special scripts.

```
5.34x2346 5.4x1368 5.11x912 5.0x684 1.174x683 5.17x682 1.172x644 5.6x456 5.5x456 5.2x456 5.16x456 3.35x452 4.55x352
4.54x350 0.19x336 1.139x295 5.27x277 4.3x264 3.172x263 4.11x259 4.22x253 4.16x253 5.3x240 1.177x238 3.37x233 5.7x228
5.33x227 5.26x227 3.23x169 3.39x167 4.37x166 4.4x134 4.36x118 6.48x117 4.35x116 3.24x115 4.29x110 4.5x109 1.41x108
4.9x106 3.6x106 4.1x105 4.0x105 3.5x94 3.69x93 6.33x91 3.8x91 3.78x91 3.57x82 2.47x75 3.73x74 1.261x74 4.12x69 3.44x67
10.31x66 3.38x65 1.183x65 6.5x63 4.38x62 3.74x62 3.75x59 3.63x59 1.17x59 3.66x57 1.276x57 4.44x56 3.96x55 3.32x55
1.138x55 4.50x54 4.46x54 4.18x54 6.3x53 3.2x53 6.34x52 1.141x51 4.30x50 1.298x50 3.79x49 3.4x49 4.33x46 3.17x46
4.20x45 3.9x45 3.11x45 3.25x44 3.77x43 3.10x43 4.43x42 3.76x42 3.53x42 1.299x42 2.54x38 1.160x38 1.144x38 1.162x37
3.64x36 3.45x35 1.266x35 1.184x34 1.129x34 3.27x33 3.132x33 1.271x33 3.18x32 1.248x32 4.6x31 3.21x31 1.190x30 1.181x29
3.80x28 3.72x28 10.55x28 3.70x27 3.42x27 4.40x26 3.90x26 0.46x26 6.47x25 3.65x25 3.131x25 0.83x25 3.7x24 3.41x24
3.129x24 3.114x24 1.296x24 3.89x23 3.87x23 3.30x23 3.135x23 1.91x23 6.4x22 10.14x22 1.213x22 3.113x21 3.86x20 3.71x20
1.73x20 1.263x20 1.185x20 6.51x19 6.1x19 3.166x19 1.93x19 1.323x19 1.280x19 0.82x19 3.58x18 3.43x18 3.126x18 2.31x18
10.19x18 1.142x18 0.39x18 3.81x17 1.304x17 1.189x17 1.165x17 6.9x16 1.305x16 1.302x16 1.295x16 1.220x16 1.100x16
0.75x16 0.69x16 6.16x15 5.14x15 3.84x15 3.54x15 3.102x15 1.252x15 2.50x14 2.1x14 1.85x14 1.292x14 1.235x14 1.156x14
1.146x14 6.8x13 6.41x13 6.39x13 4.8x13 4.48x13 3.34x13 3.148x13 3.112x13 1.75x13 1.306x13 6.54x12 3.88x12 3.85x12
3.29x12 1.343x12 1.186x12 1.164x12 1.126x12 0.100x12 4.15x11 3.50x11 3.36x11 3.139x11 3.120x11 3.110x11 1.40x11
1.318x11 1.207x11 0.99x11 0.68x11 9.9x10 6.56x10 4.41x10 4.17x10 3.134x10 2.58x10 2.4x10 1.255x10 0.97x10 0.84x10
3.60x9 3.178x9 3.127x9 3.111x9 3.101x9 10.40x9 1.76x9 1.267x9 1.197x9 0.73x9 0.45x9 9.11x8 7.2x8 6.57x8 6.32x8 3.95x8
3.93x8 3.61x8 3.40x8 3.170x8 3.165x8 3.164x8 3.16x8 3.12x8 3.109x8 3.106x8 10.35x8 1.45x8 1.325x8 0.61x8 7.0x7 6.53x7
5.32x7 5.30x7 4.27x7 3.62x7 3.31x7 3.167x7 3.14x7 3.115x7 3.100x7 1.86x7 1.256x7 1.194x7 7.32x6 7.25x6 6.43x6 6.0x6
4.47x6 4.45x6 4.42x6 4.39x6 4.26x6 4.13x6 3.98x6 3.94x6 3.68x6 3.52x6 3.163x6 3.162x6 3.150x6 3.137x6 3.130x6 3.13x6
3.125x6 3.124x6 3.117x6 2.86x6 2.59x6 2.57x6 1.331x6 1.320x6 1.226x6 1.223x6 1.199x6 0.76x6 0.65x6 9.4x5 9.17x5 6.55x5
6.25x5 6.11x5 5.13x5 3.51x5 3.22x5 3.19x5 3.171x5 3.128x5 3.119x5 3.107x5 3.0x5 2.37x5 10.39x5 10.17x5 10.12x5 1.274x5
1.153x5 1.117x5 0.96x5 0.91x5 9.19x4 7.24x4 7.19x4 6.46x4 6.45x4 6.12x4 4.21x4 4.10x4 3.145x4 3.133x4 3.1x4 10.54x4
10.53x4 10.45x4 10.43x4 10.41x4 10.4x4 10.29x4 10.25x4 10.18x4 1.340x4 1.339x4 1.337x4 1.332x4 1.328x4 1.290x4 1.285x4
1.279x4 1.278x4 1.269x4 1.268x4 1.249x4 1.242x4 0.89x4 9.37x3 9.36x3 9.26x3 7.26x3 7.21x3 6.6x3 6.42x3 6.14x3 5.23x3
4.25x3 3.83x3 3.82x3 3.67x3 3.59x3 3.159x3 3.146x3 3.121x3 3.108x3 3.104x3 3.103x3 2.89x3 2.18x3 10.3x3 10.28x3
10.27x3 10.1x3 1.95x3 1.44x3 1.293x3 1.244x3 1.243x3 1.135x3 1.118x3 0.95x3 0.37x3 9.35x2 9.25x2 9.24x2 9.16x2 9.0x2
8.2x2 7.30x2 7.3x2 7.28x2 7.11x2 6.63x2 6.59x2 6.49x2 6.44x2 6.30x2 6.29x2 6.28x2 6.27x2 6.26x2 6.21x2 4.53x2 4.52x2
4.51x2 4.49x2 3.33x2 3.161x2 3.155x2 3.153x2 3.15x2 3.144x2 3.140x2 3.123x2 2.93x2 2.71x2 10.59x2 10.58x2 10.51x2
10.38x2 10.37x2 10.36x2 10.33x2 10.21x2 10.20x2 10.10x2 1.355x2 1.349x2 1.342x2 1.329x2 1.291x2 1.260x2 1.259x2
1.258x2 1.257x2 1.251x2 1.2x2 1.178x2 1.159x2 1.158x2 0.98x2 0.102x2 9.8x1 9.6x1 9.34x1 9.32x1 9.23x1 9.22x1 9.21x1
9.20x1 9.18x1 9.15x1 9.14x1 9.13x1 9.10x1 9.1x1 8.7x1 8.6x1 8.3x1 8.0x1 7.33x1 7.27x1 6.66x1 6.60x1 6.40x1 6.36x1
6.20x1 6.19x1 6.18x1 5.8x1 5.28x1 4.7x1 4.23x1 3.92x1 3.55x1 3.47x1 3.26x1 3.177x1 3.175x1 3.174x1 3.173x1 3.160x1
3.156x1 3.147x1 3.143x1 3.138x1 3.116x1 2.19x1 2.0x1 10.9x1 10.8x1 10.7x1 10.56x1 10.5x1 10.48x1 10.42x1 10.34x1
10.32x1 10.30x1 10.26x1 10.23x1 10.16x1 10.13x1 1.356x1 1.352x1 1.348x1 1.347x1 1.341x1 1.334x1 1.333x1 1.330x1
1.327x1 1.322x1 1.321x1 1.319x1 1.316x1 1.315x1 1.314x1 1.313x1 1.312x1 1.297x1 1.283x1 1.281x1 1.277x1 1.245x1
1.236x1 1.221x1 1.113x1 0.94x1 0.93x1 0.92x1 0.22x1 0.101x1
```

## Host groups the executable's table lacks (signature inference)

Scripts that call group 3 (154 functions, 91 scripts such as the Gummi ship enemies `G_*`), group 9 (27 functions, the Gummi menu script) and entries 4.0 and 4.1 (105 mission and boss scripts, where they bracket a scene: `4.0(ctx) ... 4.1()`) used to stop with `UnknownHost`. `signatures.rs` derives
their argument counts and result flags from the script: the compiler pushes every argument in order and leaves the operand stack empty at the end of each statement, so the values pushed since the statement began are the arguments (minus values of enclosing expressions, found by running the rest of the statement), and the function returns a value when the code after it needs one.
The method was checked on the functions whose signature the table gives (`cargo run -p kh2-bdx --example sigcheck` hides every n-th of them and compares): with one in seven of the functions not named in `names.rs` hidden, 1512 of 1514 answers are right (99.9 percent), with every second one hidden 4145 of 4278 (96.9 percent); each answer is one function in one script. For the real unknowns (183 functions) the answers agree across all scripts for all but 25 functions, where some scripts differ by one argument;
the per-script answer is what a runtime uses. A mock run of every script on the disc (`--example runall`) shows no stack underflow, overflow or undecodable instruction in scripts that use inferred functions. **What the functions do is not known**: the host receives `Call::Unknown { group, index, args }` for them.
Passing the executable's table with `Runtime::set_host_table` lets the inference see every other function's signature, which resolves calls nested in arguments of other unknown calls; without it only the named ones are known.

## What Shadow (`obj/M_EX020`) does

Header: globals 0x1080, 5 events (0, 1, 6, 13) and 30 routines. Event 0 builds the context at global+8 (object at +4), applies defaults (blend 8 frames, short/long ranges 300 and 600, chase radius 2500/1800 or 2000/1200 depending on the area byte
and a type argument, attack probabilities 0.7 and 0.35), registers 24 modes and calls `set_obj_param(obj, 34, 0)`. Event 1 (main thread) enters `appear_shadow` and queues `mode_battle`, then each frame re-enters `mode_battle` if no mode is pending.
Event 6 (damage): if the hit is a real attack, goes to `mode_revenge` when the attacker flag (`flag_309`) is set, otherwise `mode_battle`.
Modes: `appear` / `appear_shadow` (spawn: pick from `spawn_kind` between a random-length idle (motion 0, random start frame, `rand_float`) and the dissolve-in: fade_in 30 frames, motion 162, face the player each frame until the motion ends);
`mode_battle` / `mode_battle_boss` / `mode_revenge*` choose a difficulty variant (`btl_normal/hard/super_hard`, `rvg_*`, `rst_*` from ctx+0x18); `battle_idle` and `idle*` face the player and wait `rand` frames; `move_target` approaches the player
(turn toward, run motion 2, `probe_ground` for walls, leaves if farther than the chase radius, 1000 when on screen and 2000 otherwise, via the `leave` mode: fade out 30 frames and hide); `move_shadow_target` / `move_shadow_round`
(used when `spawn_param(obj, 1) != 99`) circle the player with `vec_rotate_y` and a random angle step; attacks `atk_craw_l/r/f` (claw) are chosen in `btl_attack` / `btl_short` / `btl_long` by distance to the player
(150 and 350 thresholds) and a random roll against 0.2/0.3/0.9; `footwork`, `freeze`, `dead`, `event`, `revenge` and `rst_*` exist as modes; their bodies were not traced.
The dry run `kh2 bdx M_EX020 --run 300` shows the command sequence on the mock world: `appear_shadow` (SetObjParam 34, SetFlag 3 and 1, FaceToward the player, FadeIn 30, motion 162, FaceDirection every frame for 29 frames,
ClearFlag, motion 47 with blend 2.0), then `battle_idle`, then `move_target` (TurnToward the player, motion 2 with blend 8, ProbeGround each frame).
`tests/disc.rs::shadow_runs_its_whole_mode_cycle` runs it at four player distances, six seeds, with and without a hit: it enters `appear_shadow`, `mode_battle`, `battle_idle`, `move_target`, `btl_attack`, `btl_short`, `btl_long` and the three claw attacks and ends in `leave` when the player is far away,
and every call it makes is decoded (no `Call::Unknown`, no `UnknownHost`). `mode_revenge` needs the attacker flag (host 1.309), which the mock leaves clear.

## What Soldier (`obj/M_EX010`) does (traced in 1E2)

Header: globals 0x1070, events 0, 1, 6, 11, 13. Same structure as Shadow (same context layout, same helper routines); 26 modes are registered (`atk_craw` is the claw swipe). Mode records: priority 100 for the movement and decision modes, 200 for the attacks (`atk_*`), 300 for `revenge`
(flags 1), `leave`, `dead` and `event` (flags 3: no host interruption, and switching is disabled while they run). Context fields the modes use (set by event 0): +0x10 attack counter (starts -1), +0x14 hit counter, +0x18 difficulty variant (0), +0x1c -1, +0x20 fade length 30, +0x24 default blend 8.0,
+0x30 short range 300.0, +0x34 long range 500.0, +0x38 / +0x3c chase radii 2000 (off screen) and 1200 (on screen) for the type Soldier passes (2500 for area byte 10; the other types give 2500 / 1800 and 2500 / 1200), +0x40 and +0x44 the probabilities 0.7 and 0.35 (stored; no Soldier mode reads them), +0x48 a movement parameter block (probe ranges 200 and 200, turn speed 0, move motion 2, blend 8, jump motion 3, air speed 8).

- **Events.** 0 init, 1 main (`appear`, queue `mode_battle`; each frame re-enters `mode_battle` when nothing is pending). 6 damage: when `hit_is_real`, the hit counter +0x14 goes up and the mode becomes `mode_revenge`. 13 event begin: `mover_method(0.0)`, motion 4 with blend 8, `set_flag_26`.
  **11 (three arguments: reply pointer, query id, third)** answers the single query 34 with a freshly allocated target record (`alloc`, a vector, `2.23 target` freed through `1.131 free_target`, a call to `2.20` on it) and ignores every other query; what the game asks with it is not known (inferred: the lock-on anchor).
- **appear.** `spawn_kind` -1 or 2 (spawned in the world): face the player, motion 44 with fade-in 30 frames, face the player every frame until the motion ends, then motion 4 (0 when the object is in a state where `flag_124` or not `busy`). Other kinds: motion 0 started at a random frame, wait while `event_active` and while `global_byte_301` is set, then a random wait up to 30 frames.
- **idle** (priority 100): wait until the object is free (`flag_124` clear and not `busy`, no hit stun), face the target, motion 0 until it ends. **idle_time**: wait a random 15 to 45 frames, face the target, motion 0. **footwork** (motion 47) is never entered by the script; the game's native code must switch to it.
- **mode_battle**: leave check (`global_flag_80` forces leaving, `global_flag_4` forbids it, otherwise leave when the player is farther than +0x3c on screen or +0x38 off screen: mode `leave`), `set_target_mode(target, 0)`, hit counter +0x10 plus one, then by the distance to the target: under +0x30 (300) `btl_attack`, under +0x34 (500) `btl_short`, else `btl_long`.
- **btl_attack**: when the counter +0x10 is at least 3 and a roll is under 0.6: counter reset, `move_back` now, `atk_jump` and `idle` queued. Otherwise a roll under 0.6 gives `move_roll` now and `atk_craw` queued, else `atk_craw` now; then `idle_time` is queued. Then rolls under 0.5 queue `move_roll` or `move_step`
  (twice), and under 0.3 `atk_jump` and `idle`. **btl_short**: roll under 0.4: `atk_craw` then `idle_time`; under 0.8: `move_roll`; else `move_target`; then `move_roll`, and `atk_craw` + `idle_time` (roll under 0.4) or `move_step`, `atk_craw`, `idle_time`. **btl_long**: roll under 0.3 sets `idle_time`, then it resets the queue and sets `move_target`.
- **mode_revenge** (after a hit): `set_target_mode(target, 1)`; distance under 500 gives `rvg_short`, else `rvg_long`; `mode_battle` queued. **rvg_short**: with at least 2 hits taken (counter reset): 90 percent `atk_kick`, else `atk_jump` + `idle`; with fewer: 60 percent `atk_jump`, else `atk_craw`; then `idle`.
  **rvg_long**: leave check, then 70 percent `fight` else `move_target`, then `atk_jump` and `atk_kick` queued. `mode_battle_boss`, `mode_revenge_boss` and `revenge` pick `btl_normal/hard/super_hard`, `rvg_*` and `rst_*` by the difficulty variant, but none of those modes is registered in Soldier, so for it they do nothing.
- **Attacks.** `atk_craw`: motion 201 (202 when the target is more than 200 away) until it ends. `atk_kick`: motion 203. `fight`: motion 154, then motion 4. All through one routine that waits for the object to be free, faces the target, plays the motion with `play_motion_checked` and waits for its end (leaving when the motion state changes or the object is hit and recovered).
  **atk_jump**: wait free, face the target, `start_move` with direction up 260 (motion 3, blend 0): the jump; wait until `airborne`; while airborne: face the target, wait 24 frames, motion 204 (blend 8), then every frame set the movement vector toward the target with speed distance / 30 (at most 30) until the motion ends or the object lands.
- **Movement.** `move_target`, `move_back` and `move_roll` share one routine: for a random number of frames (90 to 180 for `move_target`, 20 to 40 for the others) compute the direction to the target, stop early when closer than 300 (`move_target` only), rotate the direction by 0, 180 (`move_back`) or 90 degrees with a random sign (`move_roll`) and go to the point 500 along it:
  turn toward it, motion 2 with blend 8 (or the air speed 8 when airborne), `probe_ground` along the facing with range 200; when the probe answers 1 or 2 the object starts a move with motion 3 (a jump over the obstacle); an answer of 3 ends the movement. `move_step`: motion 151 or 152 (even roll) facing the target until it ends.
- **leave**: motion 45, `spawn_attached(1, 1, 0)`, `set_flag 3`, `fade_out 30`, wait 30, `hide`, then yield forever. **dead**: ground marker when +0x1c is not negative, `spawn_attached(2, 1, 0)` released at once, `set_flag 3` and `6`, fade out over half of +0x20, wait, hide. **freeze**: motion 0, then yield forever. **event**: yield forever (enter hook: motion 0).
- The dry-run test (`tests/disc.rs::soldier_runs_its_whole_mode_cycle`) runs Soldier on the mock world (which gained jumps and the object's own memory, since the jump writes the movement vector at obj+0x10) at four distances, six seeds, with and without a hit: it enters `appear`, `mode_battle`, `idle`, `idle_time`, `btl_attack`, `btl_short`, `btl_long`, `atk_craw`, `atk_kick`, `atk_jump`, `move_target`, `move_roll`, `move_step`, `move_back`, `fight`, `mode_revenge`, `rvg_short`, `rvg_long`
  and `leave` (player 6000 away), with no error and no undecoded call.

## Verification

`crates/kh2-bdx/tests/oracle.rs` runs the game's interpreter (FUN_001d7cb0 in `kh2-ee`) and this crate's on the same program and compares program counter, operand depth, frame pointer and top of stack after every instruction, plus every host call and its arguments.
Host functions of groups 1 and 2 are replaced on both sides by the same scripted replies; group 0 math (vectors, random numbers, rotation, and since 1E2 `fabs` and `vec_scale_in_place`) runs natively on the game side. With `KH2_BDX_ORACLE_ALL=1` it covers every entry point and every call target of
all scripts: 1246 scripts, 51,709 starting points, 165,032 run slices, 17,485,071 interpreter steps and 1,447,005 host calls, **0 differences** (the routines passed to the thread-spawn calls count as starting points since 1E2). The default run checks every 25th script plus Shadow, Soldier and `00common.bdx`.
It found and fixed: the interpreter's pi constants, the game's own sin/cos kernels, the rotation helper's polynomial, saturating vector arithmetic. Not covered by the oracle: the Rust `Call` decoding of group 1/2 functions (checked by reading
the wrappers, not executed), the mode machine, threads, messages and queues (`runtime.rs`, covered by `tests/runtime.rs` on hand-assembled scripts and by the dry runs), the signature inference (`sigcheck`), and `HostTable`.
Tests that need no disc: `tests/synthetic.rs` (instruction semantics), `tests/runtime.rs` (child threads, tag kill, cleanup, delayed messages, remote mode entries, action thread, inferred signatures), the names table against this document.
Tests that read the disc and skip without `KH2_ISO`: `tests/disc.rs` (parsing, signature table against the executable's, every host call on the disc has a signature, Shadow and Soldier dry runs), `tests/oracle.rs`.

## Gaps and open questions

- The code that registers the host tables of groups 3 and 9 and entries 4.0 and 4.1 is not in the executable file or in any file on the disc that this project can read; their signatures are inferred and what they do is unknown.
- 540 host functions are unnamed. They are mostly in the NPC library (group 5), the mission and area scripts (group 4) and boss scripts. Several named ones only describe a field read or written by the wrapper.
- Roles marked "inferred"; the object fields named are the ones the wrappers touch, not a map of the object.
- The mode manager ports `FUN_001869b8`, `00186900`, `00186cf0`, `001868a8`, `00186db0`, `00186ef8`, `00186ca8`, `00187040`, `00186e70`, `00186fa8`, `00186f50`, `001871d0`; the thread tree ports `FUN_0016c680`, `0016c8d8`, `0016c928`, `0016c3f0`, `0016d458`, `00172df0`; messages `FUN_00187170/00187190/00187250`.
  Not ported: the per-object thread count (+0x488, available as `Runtime::live_threads`), the order of the action thread relative to the script update, the identity of the "other object" of remote queue entries, children started by one object on another object's main thread through 1.82 with a foreign context (assumed to be the object's own), reading another thread's frame memory through a pointer.
  Global memory starts zeroed; whether the game zeroes it is not verified.
- Sine and cosine for arguments beyond about 2^7 * pi/2 use a double precision fallback (the game uses a multi-word reduction).
- The wrapper tables assume the USA release; `HostTable::from_exe` reads fixed addresses.
- Soldier: what event 11 / query 34 is for, and which game code enters `footwork`.
