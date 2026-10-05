# BDX: enemy AI bytecode

Findings for work package 1E. Everything here was read from the USA executable (`SLUS_210.05`) and from the BDX files on the
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
sin 81, cos 42, deg-to-rad 690, rad-to-deg 28, exit 36, copy-indirect 534.

Float semantics: the console's FPU has no infinities or NaNs (overflow saturates to the largest normal number); this crate applies that clamp.
The interpreter's own pi is 0x40490fda and 2 pi is 0x40c90fda (one unit below the nearest floats); sin/cos use the SunPro-style kernels with the game's constants (`trig.rs`).

## How the game runs a script (verified unless noted)

- Per object: `FUN_00169a90` creates one thread, runs **event 0** (arg: the object pointer) as a short handler, then makes **event 1** the persistent main thread (obj+0x484).
- Every frame `FUN_0016a288` runs the main thread one slice; while it is alive the **mode manager** (obj+0x300, `FUN_001869b8`) runs.
- Events are started by the game with `FUN_0016c620(thread, id, args, n)` and run to their first `yield`, then are discarded. Known ids from call sites: 0 init, 1 main,
  6 damage received (arg: the hit record; `FUN_001b21f0`), 0xd / 0xe event begin / end (`FUN_00182f68`, `FUN_00183080`), 0xb (3 args), 0xf (2 args), 0x10 (3 args), 0x13 (2 args),
  0x17, 0x18, 0x19, 0x1a (`FUN_001b3d18`, `FUN_001b3cc8`, `FUN_001b3c70`), 0x1b (3 args). Meanings other than 0, 1, 6 are inferred from the names of the call sites only.
  Ids 2, 3 occur in `00common.bdx` (`FUN_001954f8`).
- **Modes.** A script registers named modes in a 0x80-record table in its globals (host 1.6; record = name pointer, priority u16, flags u16, nine routine words) and attaches the table and a context
  structure (host 1.7). Record routine slots: 0 main routine (runs as the mode's thread, argument = context), 1 on exit, 2 event begin, 3 event end, 4 damage hook (args context, hit record),
  7 on enter (run once at mode start). Slots 5, 6 and 8 are called by the manager for messages (delayed set-mode; not ported). Host 1.8 switches now (kills the running thread, runs exit hook, starts the new thread, runs the enter hook),
  1.9 queues a mode to start after the running one ends (only when its priority is back to 0), 1.10 tests "queue not empty or a mode running", 1.127 resets. The manager runs the mode's thread each frame;
  a switch made inside a running mode thread lets the new thread run in the same frame. Record flag bit 1 of the high half disables switching while the mode runs; bit 0 clear means "ask the host before running" (`ModeMayStart`).
  Priority below 300 lets the host interrupt (`ModeInterrupted`). Implemented in `runtime.rs`; the delayed-message queue (host 1.5/1.241/1.275/1.277 use FUN_00187190) and the cross-object queue flag are **not** ported.
- Context structure pointer: host functions of "type A" take the script context (word at +4 = the game object); "type B" take the object directly (scripts pass `ctx->obj` with `ldf +4`). `runtime.rs` resolves both and the `Host` only sees objects.

## Host interface

Host functions are `group.index`. The tables are in the executable at 0x35bce8 (group pointers) with `{function, flags}` entries; flags low 16 bits = argument count, bit 30 = returns a value.
Groups 0, 1, 2 (math; the scripted object; enemy side) are static. Groups 3 and 9 are filled in at run time by other code (FUN_0016ca10) and are absent from the static table. The calling convention
(FUN_001d7cb0): arguments go into a 16-word buffer, the native function receives a pointer to it and writes its result into word 0.
`kh2_bdx::HostTable::from_exe` reads the table from the user's executable; `names.rs` carries the argument counts of the analysed functions and the tests check both agree.

`kh2_bdx::Host` has one method, `call(&Call) -> Value`, plus `read_mem`/`write_mem` for game memory a script holds pointers into (hit records, targets). `Call` (host.rs) names every analysed function with typed fields; a function not
analysed arrives as `Call::Unknown { group, index, args }`. Group 0 (vector math, random numbers) is implemented inside the crate and needs no host: `frame_time` returns 1.0 (the game's per-frame unit),
`vec_add/sub/scale` work on all four lanes, `vec_length`/`vec_normalize` on xyz (normalize stores the length in w and returns it), `vec_rotate_y`, and the generator `seed = seed * 0x10dcd` with
`rand_int(n) = seed / ceil(2^32 / n)`, `rand_float(max) = seed * 2^-32 * max`, `rand_range(a, b) = a + seed * 2^-32 * (b - a)` (all verified).

Analysed functions (name: effect read from the wrapper; role marked inferred where it comes from script usage). Obj fields are offsets in the game object.

- Motion: `play_motion(obj, id, blend)` 1.11 (motion controller at obj+0x140, FUN_0017e220); `play_motion_checked` 1.12 (FUN_0017e2d8, role inferred: switch only when the game accepts); `play_motion_at(obj, id, start, blend)` 1.61; `motion_done` 1.14 (obj+0x160 bit 1);
  `motion_length(obj, id)` 1.67 and `motion_end` 1.68 (id -1 = current; inferred); `motion_state` 1.15 (obj+0x154).
- Facing and movement: `face_direction` 1.4 (obj+0x66c = atan2(x, z)), `face_toward` 1.79, `turn_toward(obj, dir, speed)` 1.80 (returns 1 while still turning), `set_move` 1.111 (normalised direction, speed in w, obj+0x10), `set_velocity` 1.89 (obj+0x880),
  `start_move` 1.55 / `start_move_alt` 1.168 (direction + motion), `probe_ground` 1.119 (collision probe; returns 0..3), `hide` 1.28, `fade_out` 1.19, `fade_in` 1.20 (alpha tween 1 to 0 / 0 to 1 over the given frames), `ground_marker` 1.92.
- Queries: `player` 1.3, `position` 1.147, `facing_vector` 1.201, `obj_vec_860` 1.62, `obj_vec_850` 1.64, `target` 2.23 (obj+0x9f4), `target_position` 1.120, `last_attacker` 2.88 (obj+0xae4, falls back to the player), `on_screen` 1.56 (camera frustum test),
  `airborne` 1.57 (inferred), `busy` 1.60, `hit_stun` 2.5 / 1.78 (obj+0x160 bit 9), `hit_is_real` 2.8 (byte +0x26 of the hit record), `spawn_param(obj, i)` 1.39 (word i of the array at obj+0x81c; Shadow tests value 99), `spawn_kind` 1.110,
  `event_active` 1.34 (global 0x35e3a4), `global_flag_4` 1.171 and `global_flag_80` 1.206 (global 0x35d268), `global_byte_33e830` 1.23, `global_byte_301` 1.301, `flag_124` 1.124, `flag_309` 1.309.
- Flags and effects: `set_flag` 1.70 / `clear_flag` 1.71 (bit n of the array at obj+0x108), `set_flag_26` 1.65, `set_flag_768` 1.104 / `clear_flag_768` 1.105 (obj+0x768 bit 6), `spawn_attached` 1.87 (returns a handle),
  `release_effect` 0.79, `obj_method_44/48` 1.133/1.134 (virtual calls on the object; role unknown), `mover_method` 1.128 (virtual call on obj+0xc), `set_target_mode` 1.121, `set_obj_param` 2.9 (FUN_001b2148), `request_2_20` 2.20, `free_target` 1.131.

73 of the 812 distinct host functions called on the disc are named. The rest are listed below as `group.index x total-uses`. That is 739 functions; they keep their numbers in listings and their argument counts come from the executable table at run time. Groups 3 and 9 are runtime-registered and have no static signature, so scripts that call them cannot be run by this interpreter yet.

```
0.2x2484 5.34x2346 5.4x1368 1.94x1034 5.11x912 1.84x839 0.21x724 5.0x684 1.174x683 5.17x682 1.172x644
1.114x554 1.98x552 1.58x527 1.151x471 1.176x456 5.2x456 5.5x456 5.6x456 5.16x456 3.35x452 1.13x449 1.18x401
1.262x396 1.145x392 1.74x384 1.211x383 1.90x366 4.55x352 1.112x350 4.54x350 1.222x340 0.19x336 1.170x335
1.53x334 1.161x321 1.36x315 1.109x303 1.139x295 1.54x293 5.27x277 1.148x267 4.3x264 3.172x263 4.11x259
1.140x256 4.16x253 4.22x253 0.0x252 5.3x240 2.11x239 1.177x238 3.37x233 2.13x232 5.7x228 5.26x227 5.33x227
0.23x222 1.1x222 1.37x222 2.10x222 0.42x219 0.47x212 1.29x206 2.12x202 1.150x198 2.45x198 1.69x196 0.1x193
1.228x189 1.66x184 0.15x177 1.52x177 1.108x173 3.23x169 3.39x167 4.37x166 1.230x159 2.21x156 1.130x155
1.311x153 1.83x151 1.63x149 1.231x141 0.43x140 1.212x139 2.15x136 1.229x134 4.4x134 1.22x132 1.209x130
1.196x128 2.25x119 4.36x118 6.48x117 4.35x116 3.24x115 0.44x114 0.53x110 4.29x110 4.5x109 1.41x108 1.99x108
3.6x106 4.9x106 0.35x105 1.32x105 1.33x105 1.48x105 4.0x105 4.1x105 0.60x102 0.14x99 3.5x94 3.69x93 3.8x91
3.78x91 6.33x91 0.85x87 2.62x85 1.157x83 3.57x82 1.218x80 1.101x79 1.225x79 0.38x78 1.107x78 0.9x75 2.47x75
0.50x74 1.261x74 3.73x74 0.51x73 1.97x69 4.12x69 1.24x68 1.132x68 1.247x68 1.202x67 2.22x67 3.44x67 10.31x66
1.183x65 3.38x65 6.5x63 3.74x62 4.38x62 2.17x61 1.232x60 1.17x59 2.64x59 3.63x59 3.75x59 1.276x57 3.66x57
1.102x56 1.317x56 4.44x56 1.138x55 2.30x55 3.32x55 3.96x55 4.18x54 4.46x54 4.50x54 3.2x53 6.3x53 2.76x52
6.34x52 1.0x51 1.141x51 1.77x50 1.298x50 2.7x50 4.30x50 1.203x49 3.4x49 3.79x49 1.155x46 3.17x46 4.33x46
0.49x45 1.26x45 1.227x45 3.9x45 3.11x45 4.20x45 3.25x44 2.61x43 2.85x43 3.10x43 3.77x43 1.299x42 3.53x42
3.76x42 4.43x42 1.270x41 1.335x41 1.336x41 1.217x40 1.210x39 2.66x39 1.144x38 1.160x38 2.54x38 1.162x37
0.32x36 1.59x36 1.143x36 2.55x36 3.64x36 1.125x35 1.266x35 3.45x35 0.52x34 1.129x34 1.184x34 1.271x33 3.27x33
3.132x33 0.41x32 1.248x32 3.18x32 3.21x31 4.6x31 1.190x30 0.78x29 1.179x29 1.181x29 2.60x29 2.49x28 3.72x28
3.80x28 10.55x28 1.182x27 3.42x27 3.70x27 0.46x26 3.90x26 4.40x26 0.83x25 1.21x25 2.6x25 2.48x25 2.63x25
3.65x25 3.131x25 6.47x25 1.187x24 1.188x24 1.238x24 1.296x24 2.3x24 2.67x24 3.7x24 3.41x24 3.114x24 3.129x24
1.91x23 1.180x23 3.30x23 3.87x23 3.89x23 3.135x23 1.213x22 1.214x22 6.4x22 10.14x22 1.88x21 1.149x21 2.24x21
2.92x21 3.113x21 1.73x20 1.185x20 1.263x20 3.71x20 3.86x20 0.8x19 0.82x19 1.93x19 1.280x19 1.323x19 3.166x19
6.1x19 6.51x19 0.39x18 1.142x18 1.234x18 2.31x18 3.43x18 3.58x18 3.126x18 10.19x18 1.42x17 1.165x17 1.189x17
1.304x17 1.324x17 3.81x17 0.69x16 0.75x16 1.100x16 1.167x16 1.220x16 1.250x16 1.295x16 1.302x16 1.305x16
2.46x16 2.78x16 6.9x16 0.57x15 0.59x15 1.43x15 1.252x15 3.54x15 3.84x15 3.102x15 5.14x15 6.16x15 1.82x14
1.85x14 1.146x14 1.156x14 1.235x14 1.292x14 2.1x14 2.50x14 1.75x13 1.166x13 1.233x13 1.306x13 2.52x13 2.72x13
3.34x13 3.112x13 3.148x13 4.8x13 4.48x13 6.8x13 6.39x13 6.41x13 0.100x12 1.38x12 1.126x12 1.164x12 1.186x12
1.343x12 2.77x12 3.29x12 3.85x12 3.88x12 6.24x12 6.54x12 0.68x11 0.99x11 1.40x11 1.195x11 1.207x11 1.272x11
1.318x11 2.2x11 2.79x11 2.91x11 3.36x11 3.50x11 3.110x11 3.120x11 3.139x11 4.15x11 0.84x10 0.97x10 1.255x10
2.4x10 2.58x10 3.134x10 4.17x10 4.41x10 6.56x10 9.9x10 0.24x9 0.45x9 0.73x9 1.76x9 1.197x9 1.267x9 3.60x9
3.101x9 3.111x9 3.127x9 3.178x9 10.40x9 0.31x8 0.61x8 0.70x8 1.45x8 1.122x8 1.239x8 1.325x8 2.56x8 3.12x8
3.16x8 3.40x8 3.61x8 3.93x8 3.95x8 3.106x8 3.109x8 3.164x8 3.165x8 3.170x8 6.2x8 6.32x8 6.57x8 7.2x8 9.11x8
10.35x8 1.86x7 1.194x7 1.200x7 1.256x7 2.82x7 2.84x7 3.14x7 3.31x7 3.62x7 3.100x7 3.115x7 3.167x7 4.27x7
5.30x7 5.32x7 6.53x7 7.0x7 0.65x6 0.76x6 1.199x6 1.205x6 1.223x6 1.226x6 1.320x6 1.331x6 2.57x6 2.59x6 2.83x6
2.86x6 3.13x6 3.52x6 3.68x6 3.94x6 3.98x6 3.117x6 3.124x6 3.125x6 3.130x6 3.137x6 3.150x6 3.162x6 3.163x6
4.13x6 4.26x6 4.39x6 4.42x6 4.45x6 4.47x6 6.0x6 6.43x6 7.25x6 7.32x6 0.26x5 0.91x5 0.96x5 1.117x5 1.153x5
1.204x5 1.274x5 2.36x5 2.37x5 3.0x5 3.19x5 3.22x5 3.51x5 3.107x5 3.119x5 3.128x5 3.171x5 5.13x5 6.11x5 6.25x5
6.55x5 9.4x5 9.17x5 10.12x5 10.17x5 10.39x5 0.89x4 1.191x4 1.242x4 1.249x4 1.254x4 1.268x4 1.269x4 1.278x4
1.279x4 1.285x4 1.290x4 1.328x4 1.332x4 1.337x4 1.339x4 1.340x4 3.1x4 3.133x4 3.145x4 4.10x4 4.21x4 6.12x4
6.45x4 6.46x4 7.19x4 7.24x4 9.19x4 10.4x4 10.18x4 10.25x4 10.29x4 10.41x4 10.43x4 10.45x4 10.53x4 10.54x4
0.37x3 0.95x3 1.44x3 1.95x3 1.118x3 1.135x3 1.192x3 1.243x3 1.244x3 1.293x3 2.18x3 2.81x3 2.89x3 3.59x3 3.67x3
3.82x3 3.83x3 3.103x3 3.104x3 3.108x3 3.121x3 3.146x3 3.159x3 4.25x3 5.23x3 6.6x3 6.14x3 6.42x3 6.58x3 7.21x3
7.26x3 9.26x3 9.36x3 9.37x3 10.1x3 10.3x3 10.27x3 10.28x3 0.98x2 0.102x2 1.2x2 1.115x2 1.158x2 1.159x2 1.178x2
1.251x2 1.257x2 1.258x2 1.259x2 1.260x2 1.265x2 1.291x2 1.329x2 1.342x2 1.349x2 1.355x2 2.29x2 2.32x2 2.33x2
2.35x2 2.39x2 2.71x2 2.93x2 3.15x2 3.33x2 3.123x2 3.140x2 3.144x2 3.153x2 3.155x2 3.161x2 4.49x2 4.51x2 4.52x2
4.53x2 6.21x2 6.26x2 6.27x2 6.28x2 6.29x2 6.30x2 6.44x2 6.49x2 6.59x2 6.63x2 7.3x2 7.11x2 7.28x2 7.30x2 8.2x2
9.0x2 9.16x2 9.24x2 9.25x2 9.35x2 10.10x2 10.20x2 10.21x2 10.33x2 10.36x2 10.37x2 10.38x2 10.51x2 10.58x2
10.59x2 0.22x1 0.25x1 0.88x1 0.92x1 0.93x1 0.94x1 0.101x1 1.113x1 1.154x1 1.221x1 1.236x1 1.245x1 1.264x1
1.277x1 1.281x1 1.283x1 1.286x1 1.287x1 1.297x1 1.312x1 1.313x1 1.314x1 1.315x1 1.316x1 1.319x1 1.321x1
1.322x1 1.327x1 1.330x1 1.333x1 1.334x1 1.341x1 1.347x1 1.348x1 1.350x1 1.351x1 1.352x1 1.356x1 2.0x1 2.19x1
2.70x1 2.74x1 2.87x1 3.26x1 3.47x1 3.55x1 3.92x1 3.116x1 3.138x1 3.143x1 3.147x1 3.156x1 3.160x1 3.173x1
3.174x1 3.175x1 3.177x1 4.7x1 4.23x1 5.8x1 5.28x1 6.18x1 6.19x1 6.20x1 6.36x1 6.40x1 6.60x1 6.66x1 7.27x1
7.33x1 8.0x1 8.3x1 8.6x1 8.7x1 9.1x1 9.6x1 9.8x1 9.10x1 9.13x1 9.14x1 9.15x1 9.18x1 9.20x1 9.21x1 9.22x1
9.23x1 9.32x1 9.34x1 10.5x1 10.7x1 10.8x1 10.9x1 10.13x1 10.16x1 10.23x1 10.26x1 10.30x1 10.32x1 10.34x1
10.42x1 10.48x1 10.56x1
```

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

## What Soldier (`obj/M_EX010`) does

Header: globals 0x1070, events 0, 1, 6, 11, 13. Modes (strings in the file): `appear`, `idle`, `idle_time`, `footwork`, `mode_battle`, `mode_revenge`, `fight`, `move_target`, `move_step`, `move_roll`, `move_back`,
`atk_craw` (claw), `atk_kick`, `atk_jump`, `btl_attack/short/long`, `rvg_short/long`, difficulty variants, `leave`, `dead`, `freeze`, `event`. Its structure is the same as Shadow's (same context layout, same helper routines). Event 11 is handled
(three arguments, also in 177 other scripts); I did not trace its body. **Soldier was read at the level of mode names and its shared helpers only**; the per-mode behaviour (when it kicks, jumps or rolls) was not traced and is a gap.

## Verification

`crates/kh2-bdx/tests/oracle.rs` runs the game's interpreter (FUN_001d7cb0 in `kh2-ee`) and this crate's on the same program and compares program counter, operand depth, frame pointer and top of stack after every instruction, plus every host call and its arguments.
Host functions of groups 1 and 2 are replaced on both sides by the same scripted replies; group 0 math (vectors, random numbers, rotation) runs natively on the game side. With `KH2_BDX_ORACLE_ALL=1` it covers every entry point and every call target of
all 1246 scripts: 49,642 starting points, 161,714 run slices, 19,357,886 interpreter steps and 1,423,227 host calls, **0 differences**. The default run checks every 25th script plus Shadow, Soldier and `00common.bdx`.
It found and fixed: the interpreter's pi constants, the game's own sin/cos kernels, the rotation helper's polynomial, saturating vector arithmetic. Not covered by the oracle: the Rust `Call` decoding of group 1/2 functions (checked by reading
the wrappers, not executed), the mode manager (`runtime.rs`), and `HostTable`.

## Gaps and open questions

- Soldier's per-mode behaviour (above), and the bodies of events 11, 13 and 14 outside Shadow.
- 739 host functions are unnamed. Groups 3 and 9 and the functions registered at run time (FUN_0016ca10 callers) are not in the static table.
- Roles marked "inferred" above; the object fields named are the ones the wrappers touch, not a map of the object.
- The mode manager ports `FUN_001869b8`, `00186900`, `00186cf0`, `001868a8`, `00186db0`, `00186ef8`, `00186ca8`, `00187040`; the delayed-message queue (FUN_00187190/00187250), the "send to another object" queue flag (FUN_00186f50) and child threads
  spawned by scripts (FUN_00172df0 / FUN_0016c8d8) are not ported. Global memory starts zeroed; whether the game zeroes it is not verified.
- Sine and cosine for arguments beyond about 2^7 * pi/2 use a double precision fallback (the game uses a multi-word reduction).
- The wrapper tables assume the USA release; `HostTable::from_exe` reads fixed addresses.
