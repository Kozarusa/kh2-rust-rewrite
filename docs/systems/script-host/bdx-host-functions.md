# BDX: named host functions

[Contract](contract.md) | [Index](../../README.md)

## Named host functions

273 of the 812 distinct host functions called by scripts on the disc are named (73 before work package 1E2); the table has 299 entries counting 26 that no script on the disc calls. The named ones account for 86 percent of all host calls in scripts on the disc and
98.7 percent of the calls in the enemy and boss scripts (`obj/M_*`, `B_*`, `H_*`). Argument counts and return flags of every row are checked against the executable's table by a test (`signatures_match_the_executable`). Rows marked "role inferred" have their effect on the object read from the game code but
their game-level meaning guessed from how scripts use them; rows without that mark that only describe a field ("word at +0xaa8") say what the wrapper does, not what the field is for. "Native" is the address of the host function's wrapper in the executable, "uses" the number of call sites in reachable script code on the disc. Object fields are offsets in the game object.
Group 0 is the library (math, effect handles, flags), group 1 the scripted object, group 2 attacks and hit records (the layout of the hit record is in `docs/systems/combat-effects/hit-reaction.md`), group 6 bosses and NPCs.

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
| 0.37 | `vec_div(vf) -> v` | 0x16ce08 | 3 | divide all four lanes by a scalar into a new vector (FUN_0016ce08: the vector unit's reciprocal times each lane; a division by zero saturates) |
| 0.38 | `effect_set_position(pv)` | 0x16da90 | 78 | copy a vector into the effect's position (the 16 bytes behind FUN_001d9d00(handle)) |
| 0.39 | `effect_set_vector2(pv)` | 0x16dad0 | 19 | copy a vector into the 16 bytes behind FUN_001d9d28(handle), the vector after the effect's position; MAGNET_*.mag writes (s, s, s, 1) there to grow and shrink its sphere, so it reads as the scale (role inferred) |
| 0.40 | `effect_set_vec2_raw(pv)` | 0x16db10 | not counted | copy a vector into the 16 bytes behind FUN_001d9d50(handle), the effect instance's second vector, without the angle conversion of 0.41 (FUN_0016db10 is 0.39's body on FUN_001d9d50's vector instead of FUN_001d9d28's); Peter Pan's Tinker Bell script passes its angles [inferred] |
| 0.41 | `effect_set_vec2(pv)` | 0x16db50 | 32 | copy a vector (through FUN_0017be00) into the second vector of the effect instance (the 16 bytes behind FUN_001d9d50(handle)); role unknown |
| 0.42 | `angle_of(v) -> f` | 0x16d060 | 219 | atan2(v.x, v.z), the yaw of a direction |
| 0.43 | `wrap_angle(f) -> f` | 0x16de78 | 140 | wrap an angle in radians into -pi..pi (FUN_0017bbc0) |
| 0.44 | `effect_stop(p)` | 0x16dba0 | 114 | stop the effect instance of an effect handle (FUN_001e0298 on handle+0x1c, if it has one) |
| 0.47 | `alloc(i) -> p` | 0x16dea8 | 212 | allocate `size` bytes from the game heap (vtable slot 0 of the allocator at 0x33e850) |
| 0.48 | `free(p)` | 0x16ded8 | 0 | release an allocation of 0.47 (FUN_00104fe0) |
| 0.49 | `effect_alive(p) -> b` | 0x16dbd0 | 45 | bit 0 of the handle's halfword at +10 is clear (FUN_001da0d8) |
| 0.50 | `effect_valid(p) -> b` | 0x16dc00 | 74 | the handle has an effect instance (handle+0x1c != 0) |
| 0.51 | `effect_destroy(p)` | 0x16dc30 | 73 | mark the handle released (flags |= 0x10001), destroy its instance (FUN_001e0218, FUN_001e4218) and clear the pointer |
| 0.52 | `effect_fade(pfi)` | 0x16dc48 | 34 | fade the effect out over `frames` (rate = -(level at +0x20) / frames in +0x24, flag bits 0x20000, plus 1 when the last argument is non-zero, which also destroys it at the end); zero or fewer frames acts at once (FUN_001da080, FUN_001d9fd0) |
| 0.53 | `effect_position(p) -> v` | 0x16dc88 | 110 | pointer to the position vector of an effect handle (FUN_001d9d00) |
| 0.57 | `bits_33fe20_set(i)` | 0x16df30 | 15 | set bit `n` of the bit array at 0x33fe20 (FUN_001a7750) |
| 0.59 | `bits_33fe20_test(i) -> b` | 0x16df60 | 15 | test bit `n` of the bit array at 0x33fe20 (FUN_001a77b8) |
| 0.60 | `nop_60(p)` | 0x16df90 | 102 | empty function |
| 0.61 | `party_record(i) -> p` | 0x16d9a0 | 8 | FUN_00187d40(0x33fec0, member): the pointer to the party member's record (docs/systems/core-runtime/progress.md "Party member record"), member 1 Sora, 2 Donald, 3 Goofy (WP-RV2; the table slot 0x35bb98 is 30 entries after 0.31's) |
| 0.68 | `vec_rotate_x(vf) -> v` | 0x16cf78 | 4 | rotate around the x axis by an angle in radians (FUN_0017c398, the matrix of FUN_0011b490 -> FUN_002fb580), result in a static buffer |
| 0.70 | `progress_flag_102b() -> b` | 0x16d5e8 | 8 | progress flag 0x102b (FUN_00180330) |
| 0.75 | `set_member_weapons(iii)` | 0x16d9d8 | 16 | FUN_00187d88(0x33fec0, member, w0, w1) -> FUN_00187648: the two weapon words of the member's record (+0, +2; FUN_0016ee98 reads them) are set. Called by `00common.bdx` entry event 7 (routine R00c1, 13 calls: member 1 Sora 41, 2 Donald 75, 3 Goofy 49, 4 81, 5 54, 6 55 and 80, 7 72, 8 62, 9 59, 10 60, 11 61, 12 74, 13 73), which FUN_00194d78 runs once when the game object is constructed (FUN_00105638); and by `ard/tt04.ard` (`tt04_ms106`, three times, Sora, items 384, 502, 501 for its events 9, 10 and 11) (WP-RV2) |
| 0.76 | `set_form_weapon(ii)` | 0x16da08 | 6 | FUN_00187f60(0x33fec0, form) then the halfword at +0 (the form's second keyblade) is set. `00common.bdx` R00c1 sets form 1 to 4 to 0, form 5 to 71 and form 9 to 42 (WP-RV2) |
| 0.78 | `vec_lerp(vvf) -> v` | 0x16d180 | 29 | a * (1 - t) + b * t on all four lanes, result in a static buffer |
| 0.79 | `release_effect(p)` | 0x16de50 | 395 | hand an effect handle back to the game (queued for deletion) |
| 0.98 | `give_form_ability(ii)` | 0x16da40 | 2 | FUN_00187f18(form record, ability): an entry of the form's 16 ability slots (docs/systems/core-runtime/progress.md "Form record"); the two calls are in `ard/lk02.ard` and `ard/lk03.ard` (WP-RV2; the form record pointer is FUN_00187f60) |
| 0.85 | `effect_stop_mark(p)` | 0x16dbb8 | 87 | like effect_stop and also sets bit 0 of the handle's flags (+8) (FUN_001d9f48) |
| 0.86 | `set_word_4(pp)` | 0x16de68 | 0 | store the second argument at +4 of the first |
| 0.88 | `fader_88(f)` | 0x16d958 | 1 | screen fader at 0x3564d0 with flag 0 over `seconds * DAT_0037a25c` frames (FUN_0014a7f0); role inferred |
| 1.0 | `create_object(i) -> h` | 0x170190 | 51 | construct a new object (0x9c0 bytes) for entity id `id` at the origin (FUN_0016e8c0); returns it The wrapper takes one argument (the objentry id); the scripts' `lea local; push id; call` pair leaves the address on the stack and a later routine stores the handle at `[address]+4`, which is how contexts (`Arg::Obj`) name the new object. Served by `enemy_host.rs` (enemy kinds) and `summon.rs` (the summon rows' objects). WP-EN3 |
| 1.1 | `set_yaw(Of)` | 0x170ac8 | 222 | rebuild the facing vector at +0x550 from an angle (FUN_0016a790), set the yaw at +0x66c, clear flag 8 of +0x7f8; role inferred |
| 1.3 | `player() -> o` | 0x170260 | 775 | the player's object (Sora) |
| 1.4 | `face_direction(Ov)` | 0x170ae8 | 1826 | face along the direction at once (yaw = atan2(x, z) at +0x66c) |
| 1.5 | `modes_clear(p)` | 0x172050 | 716 | clear the mode record table |
| 1.6 | `modes_register(psippppppppp)` | 0x172068 | 13941 | write a mode record: table, name, priority and flags, nine routine words |
| 1.7 | `modes_attach(pp)` | 0x170e60 | 716 | attach the mode table and the context structure to the object's mode machine |
| 1.8 | `set_mode(ps)` | 0x170ea0 | 15744 | switch to the named mode now. The context is `*(*a0 + 4) + 0x300`, the mode machine of the object it names: a context naming another object than the caller (Peter Pan's `appear` sets Tinker Bell's) reaches the host as `Call::RemoteSetMode { obj, name }` (WP-EN3), no longer switching the caller |
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
| 1.29 | `play_motion_with(OOif) -> i` | 0x1715b8 | 249 | play entry `motion` of the first object's motion set on the second object's body (FUN_0017f160: the entry is looked up in the first object's set, FUN_0017ee40 finds inside it the sub-entry named `%03d0` plus a variant digit of the second object's objentry byte +7 and starts it on the second object's controller; the second object's +0x154 becomes the motion number and its +0x158 a new serial from the counter at 0x35ccb0, which is returned) |
| 1.32 | `task_start(pi) -> h` | 0x172360 | 105 | allocate a task object (0xb0c bytes, vtable 0x35eb80) and a 0x1c byte listener, return the task (FUN_001ab6a8, FUN_001aad78); role inferred |
| 1.33 | `task_free(p)` | 0x1723f0 | 105 | call the task's virtual method +0x20 |
| 1.34 | `event_active() -> b` | 0x173888 | 517 | the word at 0x35e3a4 is non-zero: an event or pause is in progress |
| 1.35 | `set_bytes_33e86c(iii)` | 0x1721f8 | 0 | store three bytes at 0x33e86c, 0x33e86d, 0x33e86e |
| 1.36 | `mask_35a810_clear(i)` | 0x172418 | 315 | clear bit n of the 64-bit mask at 0x35a810 and run the slot-off hooks (FUN_0015e928) |
| 1.37 | `mask_35a810_set(i)` | 0x172430 | 222 | set bit n of the 64-bit mask at 0x35a810 and run the slot-on hooks (FUN_0015e9a8) |
| 1.38 | `set_team(Oi)` | 0x170950 | 12 | store the word at object +0x3b0 (the attack object constructor reads it as the owner's team, FUN_0018bd08) |
| 1.39 | `spawn_param(Oi) -> i` | 0x171b50 | 869 | word i of the array at object +0x81c (the spawn record); Shadow tests the value 99 |
| 1.41 | `raise_event(ii)` | 0x1727b0 | 108 | raise the global event (event, argument): FUN_00197b70 runs the event handlers FUN_00195718, FUN_0016b470 and FUN_00197670 (a few events go to FUN_0024f0b8 or FUN_0023f108 instead); Struggle events 159 and 160 use it |
| 1.42 | `bits_1d5d7b8_set(i)` | 0x170da8 | 17 | set bit n of the bit array at 0x1d5d7b8 (FUN_001b7ce8) |
| 1.43 | `bits_1d5d7b8_clear(i)` | 0x170dc0 | 15 | clear bit n of the bit array at 0x1d5d7b8 (FUN_001b7d18) |
| 1.46 | `area_mask_groups_on()` | 0x172800 | not counted | switch the collision group on for each set bit of the area mask 0x35cd18 (area opcode 0xb), FUN_0019d4c0 -> FUN_0015e9a8 per bit |
| 1.47 | `area_mask_groups_off()` | 0x172818 | not counted | switch the collision group off for each set bit of the area mask 0x35cd18, FUN_0019d520 -> FUN_0015e928 per bit |
| 1.48 | `task_done(p) -> b` | 0x1723d8 | 105 | the task's state word (+0x70) equals 3 |
| 1.52 | `bits_47c_set(Oi)` | 0x1728c0 | 177 | set bit n of the bit array at object +0x47c (FUN_0019dd88). WP-RV2, the reader: FUN_0019a6a0(object, part id) is the one that tests the arrays; n is the byte +0 of a collision record of the object's set at +0x8d8 (FUN_0016b4e0 / FUN_0016b5d0 skip a record for which it answers false; FUN_0018ee20 and FUN_0019a760 use it too). The order: +0x47c bit n set: true; +0x480 bit n set: false (the two words are the two bit arrays FUN_0019de20 / FUN_0019de48 test); then the same pair at +0x474; then the default mask at the word +4 of the block +0x8d8 points to. So this call forces collision part n on |
| 1.53 | `bits_480_set(Oi)` | 0x1728e8 | 334 | set bit n of the bit array at object +0x480 (FUN_0019ddb0): forces collision part n off (the reader is under 1.52) |
| 1.54 | `bits_47c_480_clear(Oi)` | 0x172910 | 293 | clear bit n in both bit arrays (FUN_0019dde0). The words at `o + 4*(n>>5)` and `o + 4 + 4*(n>>5)` are and-ed with `~(1 << (n&31))`; exact. WP-RV2, the reader is FUN_0019a6a0 (under 1.52): the call takes the object's override away, so the part follows the pair at +0x474 and then the default mask again. The app has no per-part collision switch, so it stays a Stub that clears the object memory |
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
| 1.83 | `bone_to_world(Oiv) -> v` | 0x172e68 | 151 | transform a vector by the matrix of bone/joint `index` of the object (FUN_0016aa18); result in a static buffer. Party members' scripts (friend host): the hand object's drawn bone; a null handle (member with no hand object) answers the origin, Approx [inferred] |
| 1.84 | `gauges(O) -> p` | 0x172ee8 | 839 | the pointer at object +0x48c; scripts pass it to the record readers 1.228 to 1.233 |
| 1.87 | `spawn_attached(Oiii) -> h` | 0x171800 | 1541 | spawn an effect or child attached to the object; returns its handle |
| 1.88 | `lockon_point(O) -> v` | 0x172f40 | 21 | the object's lock-on point through a temporary target record (FUN_0016b6e0); role inferred |
| 1.2 | `move_toward(Ovf) -> i` | 0x170228 | ? | as set_move toward a point: the direction from the object's position (+0x540) to the point, y zeroed and normalised, stored at +0x10 with the speed in w (FUN_00182538); the result is a leftover VU status word (WP-FIX2; call sites on the disc not counted) |
| 1.89 | `set_velocity(Ovf)` | 0x172f90 | 78 | set the secondary velocity vector at +0x880 (direction normalised, speed in w) |
| 1.90 | `send_message(Osf)` | 0x172fb8 | 366 | send the name of a mode to the mode machines of all objects after a delay (see runtime.rs) |
| 1.92 | `ground_marker(Oi)` | 0x171078 | 466 | place a marker effect on the ground under the object |
| 1.94 | `obj_valid(o) -> b` | 0x170150 | 1034 | the object is in the object list and FUN_0019a440 clears; 0 is false |
| 1.96 | `mover_word_4(O) -> p` | 0x173058 | 0 | word at +4 of the object's mover (+0xc) |
| 1.97 | `obj_part_110(O) -> p` | 0x173070 | 69 | address object +0x110 |
| 1.98 | `ground_position(O) -> v` | 0x173088 | 552 | position of the object with y replaced by the ground height under it (FUN_00183808); static buffer |
| 1.99 | `ground_depth(O) -> f` | 0x1730e8 | 108 | minus the depth of the object below the ground (0 when above it): -max(0, ground - y) |
| 1.100 | `fall_time(f) -> f` | 0x173178 | - | FUN_0017bee0: sqrt(2 x / 20/49) for a height x, the time a body takes to fall it (the constant 20/49 is `DAT_0037a538`); the table gives it no name, so it reaches a host as raw words (`ai::fall_time`, docs/systems/drive-limits-summons/contract.md, WP-SM2) |
| 1.101 | `flag_160_4000(O) -> b` | 0x1716d0 | 79 | object flag 0x4000 at +0x160 (after the object-valid check) |
| 1.102 | `link_end(O)` | 0x170e38 | 56 | undo link_start (FUN_001becb8) |
| 1.103 | `nop_103(ppp)` | 0x170e58 | 0 | empty function |
| 1.104 | `set_flag_768(O)` | 0x1731a8 | 9 | set bit 6 of the word at +0x768. WP-RV2: none of the 30 load sites of +0x768 (`mips_scan --load-off 0x768`) is followed by a test of 0x40, and the only writers of bit 6 are 1.104 and 1.105, so the bit has no reader found; a Stub with no behaviour is exact for what the executable does with it |
| 1.105 | `clear_flag_768(O)` | 0x1731c0 | 9 | clear bit 6 of the word at +0x768 (no reader of the bit found, see 1.104; WP-RV2) |
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
| 1.128 | `mover_method(Of)` | 0x171298 | 250 | virtual call on the object's mover (+0xc) with one float; role unknown (role now known, WP-SM2: the airborne launch, FUN_0017b1e0(speed) on the mover and then FUN_0016b698(object, 1); a negative speed is a downward start, docs/systems/drive-limits-summons/contract.md) |
| 1.130 | `obj_diff(OO) -> i` | 0x170510 | 155 | difference of the two contexts' objects, 0 when either is 0 or they are the same |
| 1.131 | `free_target(p) -> b` | 0x1736f8 | 279 | free a target record (FUN_0018f3f8); role inferred (WP-RV2: the name is the wrong way round, it ALLOCATES. FUN_0018f3f8 takes a free index from the stack `DAT_01c8a5d0` (0x80 indexes, filled by FUN_0018ece8, count `DAT_01c8a7d0`), copies the 5-word record into the pool entry and returns its address; FUN_0018f458 gives an index back, FUN_0018f188(old, new) repoints every object whose target record +0x9f4 is `old` to `new`, and FUN_0018fff0 walks the pool). The code pops a slot index from the stack `DAT_01c8a7d0 / DAT_01c8a5d0` and copies the 5-word record into the pool entry `DAT_01c89bd0 + 0x14 * index`, returning its address: it files the record into a pool; the app's records are not pooled (Stub) |
| 1.132 | `target_release(p)` | 0x173728 | 68 | return a target record to the pool at 0x1c89bd0 (20-byte entries; FUN_0018f458) |
| 1.133 | `obj_method_44(O)` | 0x170540 | 97 | virtual method +0x44 of the object; role unknown |
| 1.134 | `obj_method_48(O)` | 0x170570 | 68 | virtual method +0x48 of the object; role unknown |
| 1.140 | `target_valid(p) -> b` | 0x173740 | 256 | the target record is usable (FUN_0018ed58); role inferred |
| 1.141 | `probe_ground_down(vpf) -> i` | 0x172538 | 56 | collision probe (FUN_001a5c30 with mask 4) from the point down by the distance: 1 and the point found written through the second argument, else 0 and the point itself written there |
| 1.143 | `bind_object_143(pi)` | 0x171d40 | 36 | set the context's object pointer from FUN_0016b0d8(arg) |
| 1.145 | `obj_word_aa8(O) -> i` | 0x1705d0 | 392 | word at object +0xaa8 |
| 1.146 | `push_object(Ovf)` | 0x1705e8 | 15 | push the object along a direction with a power (FUN_001857b0: the decaying ground push of the wall recoil) |
| 1.147 | `position(O) -> v` | 0x170610 | 8507 | world position of the object |
| 1.148 | `set_position(Ov)` | 0x170658 | 267 | set the position (w forced to 1.0) and sync the mover (FUN_0016a618) |
| 1.149 | `spawn_effect_for(OiOii) -> h` | 0x171840 | 21 | as spawn_effect with another object as the owner (FUN_0016a570) |
| 1.150 | `spawn_bit_238(Oi) -> b` | 0x171600 | 198 | bit n-1 of the byte at object +0x238 |
| 1.151 | `spawn_bit_239(Oi) -> b` | 0x171628 | 471 | bit n-1 of the byte at object +0x239 |
| 1.154 | `gflags_set_2()` | 0x1739f8 | 1 | set bit 1 of the global flag word at 0x35d268 (tested by 1.171 for bit 2 and 1.206 for bit 7) |
| 1.155 | `obj_method_78(Oiii) -> i` | 0x171c48 | 46 | virtual method +0x78 of the object with the arguments (the last as a boolean) |
| 1.157 | `spawn_effect_on(OiO)` | 0x171f40 | 83 | spawn an effect (FUN_001971c0 for the id) anchored at the object's +0x80, optionally bound to a second object (FUN_00197230) |
| 1.161 | `has_link(O) -> b` | 0x1706c8 | 321 | the object is linked to another (+0x570 non-zero and in the object list). Party members' scripts (friend host): the hand object's link state; a null handle answers false, Approx [inferred] |
| 1.163 | `nop_163(pp)` | 0x1737d0 | 0 | empty function |
| 1.165 | `offer_command(iiOi)` | 0x1734a8 | 41 | offer a reaction command: slot, command id, the object the target record is built for (position index 0), show flag; priority 100 through FUN_001bb3a8 on the player's command manager (not decompiled; read from the machine code) |
| 1.166 | `ctx_word_8(p) -> i` | 0x1737e0 | 13 | the word at +8 of the argument |
| 1.167 | `set_mode_priority(ps)` | 0x170f50 | 16 | switch to the named mode when its priority is at least that of the running one (FUN_00186e70) |
| 1.168 | `start_move_alt(Ovifi)` | 0x171460 | 112 | as start_move through the alternate setter (FUN_001b4ee0) |
| 1.170 | `spawn_record_id(O) -> i` | 0x171b30 | 335 | halfword at +2 of the record at ((object +0x818) +8); role inferred |
| 1.171 | `global_flag_4() -> b` | 0x173a28 | 371 | bit 2 of the global flag word at 0x35d268 |
| 1.174 | `look_at_object(?, ?, f)` | 0x171940 | 683 | start the head look-at controller at object +0x4a0 toward an object over `frames` (FUN_001aece8, mode 2; needs the object's type-0x14 record; docs/systems/events-movies/npc-savepoints.md section 3 "Head look-at") |
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
| 1.237 | `offer_four(iiiiO)` | 0x173508 | 8 | offer four commands into slots 3 to 6 of the player's command manager (FUN_001bb4b0 with priority 100), the target record built for the object of the last argument |
| 1.238 | `obj_word_158(O) -> i` | 0x171718 | 407 | word at object +0x158: the serial of the motion last started on it by play_motion_with |
| 1.239 | `spawn_flag_8_or_none(O) -> b` | 0x171bc8 | 8 | the object has no spawn record (+0x818) or the record's flag 8 (+4) is set |
| 1.240 | `nop_240(ppp)` | 0x1710d8 | 0 | empty function |
| 1.241 | `broadcast_message(sf)` | 0x172098 | 0 | like send_message without a sender |
| 1.244 | `in_battle() -> b` | 0x173ab8 | 3 | FUN_001903d8: a battle runs (word 0x1c8a7d8). Party members' scripts: Impl, `audio_bgm::BattleFlag::active` (WP-PY3) |
| 1.246 | `nop_246()` | 0x1737d8 | 0 | empty function |
| 1.247 | `random_free_point(vff) -> v` | 0x1726d8 | 68 | search around a position for a free point between two radii: samples a spiral with the game's second random generator and tests collisions (FUN_001a6068, FUN_001a6198); returns the point in a static buffer; role inferred |
| 1.248 | `scatter_point(vffff) -> v` | 0x172730 | 36 | a free place around a position (FUN_001a5ee0 with mask 2): radius random between the first two values, start angle from a counter of eighths (0x35e9b0) and the game's generator, a height random between the last two; tries eighths of a turn with a line test, shrinking the radius; the point in a static buffer (Final Thunder) |
| 1.250 | `set_scale_250(f)` | 0x173af8 | 16 | store the argument at 0x1c8a7e8 and the argument times the float at 0x360130 + 0x50 at 0x1c8a7ec (FUN_00190808) |
| 1.254 | `has_ptr_810(O) -> b` | 0x170ce0 | 4 | the pointer at object +0x810 is non-zero |
| 1.262 | `obj_method_54(Opp)` | 0x1709c8 | 396 | virtual method +0x54 of the object with two arguments |
| 1.264 | `gflags_clear_2()` | 0x173a10 | 1 | clear bit 1 of the global flag word at 0x35d268 |
| 1.265 | `set_word_ac8(Oi)` | 0x171f98 | 2 | store a word at object +0xac8 |
| 1.270 | `motion_register(Oi)` | 0x171730 | 41 | enter a motion id into the 16-entry list at the motion controller +0x14c unless it is already there (FUN_0017fb28) |
| 1.272 | `flag_7f8_4(O) -> b` | 0x171cb0 | 11 | flag 4 of the word at object +0x7f8 |
| 1.277 | `obj_revive(O)` | 0x172030 | 1 | FUN_00178df0 on the object: FUN_00178d80 (clears +0x7f8 bit 4, FUN_00183fe0, the normal mover FUN_0016b698(.., 0), the party record's back pointer), FUN_001d4a50 on the object at +0xb3c (its +0x9a4 and +0xa70 bit 0 set, +0x20 bit 0 cleared) and the object's virtual method +0x78 (FUN_00186238, the HP method) with the word +4 of the status block at +0x48c as the amount: the object is raised with full HP (Tinker Bell's `reraise`) |
| 1.286 | `gflags_set_1000()` | 0x173b10 | 1 | set bit 12 of the global flag word at 0x35d268 |
| 1.287 | `gflags_clear_1000()` | 0x173b28 | 1 | clear bit 12 of the global flag word at 0x35d268 |
| 1.288 | `gflags_set_2000()` | 0x173b40 | 0 | set bit 13 of the global flag word at 0x35d268 |
| 1.289 | `gflags_clear_2000()` | 0x173b58 | 0 | clear bit 13 of the global flag word at 0x35d268 |
| 1.301 | `global_byte_301() -> i` | 0x1744f8 | 458 | the byte at 0x35ea48 + 0x1e (FUN_001abee8); role unknown (WP-RV2: 0x35ea48 is the string of message-window frame groups: the 30 characters from C up to the backtick (`CDEF..XYZ`, then `[`, backslash, `]`, `^`, `_`, backtick) followed by a NUL; index 0x1e is the NUL, so the call is a constant 0 (no store to that byte exists: `mips_scan --addr 0x35ea66` finds only FUN_001abee8; the frame kinds use indexes up to 29). Its other callers FUN_0015a780, FUN_00176a18 and FUN_00179220 treat a non-zero byte as a debug override (the player may act while the flag word 0x35d268 has bit 0x100000 clear), so the Stub's 0 is exact) |
| 1.305 | `camera_position() -> p` | 0x173e80 | - | the camera's world position (FUN_001071c8 of the camera FUN_001068c0(1)), returned as a pointer to a vector; the table gives it no name |
| 1.309 | `flag_309(o) -> b` | 0x170288 | 86 | status bit 0x80 at +0x588 after a validity check; role unknown |
| 1.310 | `objentry_f_50(O) -> f` | 0x170b18 | 0 | float at +0x50 of the object's object-entry row |
| 1.311 | `set_scaled_byte(pii)` | 0x174238 | 153 | byte at +0x1a8 + i of the first argument = (halfword i of the table at its +0x254 + 0x48) * percent + 99) / 100 (FUN_001a0ba0) |
| 1.317 | `gflags_set_20000()` | 0x173b70 | 56 | set bit 17 of the global flag word at 0x35d268 |
| 1.324 | `obj_flags_abc_clear_b(Oi)` | 0x1707b0 | 17 | the same function as obj_flags_abc_clear under a second number |
| 1.325 | `send_reaction(OOiO)` | 0x170b50 | 8 | deliver script event 0xb to the first object: (the second object, the command id, a target record built for the fourth argument's object) through FUN_0016b420, the same call the player's accept path makes |
| 1.335 | `list_800_remove(Op)` | 0x170d38 | 41 | remove the entry with that value from the list at object +0x800 (FUN_001b1750) |
| 1.336 | `list_800_add(Op)` | 0x170d60 | 41 | add a value to the list at object +0x800 (FUN_001b1810) |
| 1.344 | `list_800_clear(O)` | 0x170d88 | 0 | empty the list at object +0x800 |
| 1.345 | `offer_command_ret(iiOi) -> h` | 0x1735c8 | 0 | as offer_command, returning the slot entry (0 when refused); no script on the disc calls it |
| 1.350 | `gflags_set_100000()` | 0x173b88 | 1 | set bit 20 of the global flag word at 0x35d268 |
| 1.351 | `gflags_clear_100000()` | 0x173ba0 | 1 | clear bit 20 of the global flag word at 0x35d268 |
| 2.2 | `react_flinch_push(Opf)` | 0x18df00 | 11 | reaction class 3: flinch plus push on the object (FUN_001850c8; see docs/systems/combat-effects/hit-reaction.md) |
| 2.3 | `react_push(Opf)` | 0x18df28 | 24 | reaction class 1: push only (FUN_00185098, sets flag 1 at +0x18) |
| 2.4 | `next_enemy(p) -> o` | 0x18df50 | 10 | FUN_00184ac8: the next object of the global object list (linked through +0x8b4, FUN_001b7c88) that has +0x588 bit 0x10 (the enemies), after the one given; 0 starts the walk and ends it |
| 2.5 | `hit_stun(O) -> b` | 0x18df80 | 1243 | bit 9 of the object's status word (+0x160) |
| 2.6 | `in_reaction(O) -> b` | 0x18dfa0 | 25 | the object's motion state (+0x154) is between 10 and 39 (FUN_00185a08) |
| 2.7 | `hit_reaction(p) -> i` | 0x18e0d0 | 50 | byte at hit +0x26: the effective reaction |
| 2.8 | `hit_is_real(p) -> b` | 0x18e0f0 | 215 | the hit record's reaction class is 2 or more (byte +0x26 through FUN_0018b700) |
| 2.9 | `set_obj_param(Oii)` | 0x18de30 | 264 | set an attribute pair on the object (FUN_001b2148); role inferred |
| 2.10 | `attack_create(Oiii) -> h` | 0x18e2f0 | 222 | create an attack object owned by the object: attack table row key (arg 1, arg 3), hit group id (arg 2, -1 takes a new one) (FUN_001a2ba0, FUN_0018bd08) |
| 2.11 | `attack_set_size(pff)` | 0x18e348 | 239 | attack +0x100 = a, +0x104 = b when b is positive else a (FUN_001a30e0) |
| 2.12 | `attack_move_to(pv)` | 0x18e3a0 | 202 | move the attack object to a position, keeping the previous one for the swept test (FUN_001a2fc8) |
| 2.13 | `attack_arm(p)` | 0x18e3c0 | 232 | set flag 0x800 at attack +4 when the attack object exists (FUN_0018dc98) |
| 2.14 | `attack_hit_anything(p) -> b` | 0x18e3d8 | 3 | FUN_0018c890: the attack's flags at +4 have 2 (a target), 8 or 0x10 (a wall) or 0x200 (a clash) |
| 2.15 | `hit_set_flag1(p)` | 0x18e190 | 136 | set bit 0 of the flags at hit +0x18 |
| 2.16 | `hit_flag1(p) -> b` | 0x18e1a8 | 0 | bit 0 of the flags at +0x18 (2.15 sets it) |
| 2.17 | `attack_hit_object(pOip)` | 0x18e6e0 | 61 | when the attack may hit the object (FUN_0018c5f8): build the hit record (FUN_0018c6d0), mark it, call the object's virtual method +100 with it (delivers the damage) |
| 2.19 | `attack_set_positions(pvv)` | 0x18e7a0 | 1 | attack +0xe0 = the first vector, +0xf0 (the previous position of the swept test) = the second, +4 gets bit 0x20 (FUN_001a30a8) |
| 2.20 | `start_request(ip) -> h` | 0x18e898 | 278 | start a detached thread at routine `pc` with one argument (FUN_001a4738: FUN_0016c838 makes the thread on the running thread's program, FUN_001a42b8 links it into the list at 0x1c95518; the task body FUN_001a4428 runs every listed thread once per frame and destroys it when it ends); returns the list node |
| 2.21 | `shared_block(ii) -> p` | 0x18e928 | 156 | the persistent buffer registered under a key (FUN_001a5040): the first call allocates, zeroes and registers size + 1 bytes (first byte 0); a later call with the key sets the first byte to 1; returns the buffer (kh2_bdx::shared) |
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
| 2.47 | `limit_task_47(pppp) -> i` | 0x18e988 | - | FUN_001b3438(task, a0, *(a1 + 4), a2) -> FUN_0017f0c0(0, task +8, task + 0x18, ...); the limit script's setup of a participant's part (WP-LT1: logged, answered 0) |
| 2.48 | `hit_owner(p) -> p` | 0x18e9c8 | 25 | the word at +0xc of an attack object (its owner) |
| 2.49 | `attack_word_10(pi) -> i` | 0x18e9d8 | 28 | word at +0x10 + 4 * i |
| 2.50 | `limit_task_50(pppp)` | 0x18e9f8 | - | FUN_001b3468(a2, a0, a1, a3 != 0): the limit task starts an effect (FUN_001971c0, or the BAR's kind 0x16 entry) on a participant (WP-LT1: logged) |
| 2.52 | `attack_word_110(p) -> i` | 0x18e618 | 13 | word at +0x110 |
| 2.54 | `limit_spawn_thread(pp)` | 0x18ea28 | - | FUN_001b3c10(routine, &arg, 1) -> FUN_0016c8d8(limit task +0x34, 0, routine, args, 1): a child thread of the limit script's thread, like host 1.82 with tag 0 and no cleanup (WP-LT1: not run; WP-LT2: `runtime.rs` spawns it as `Request::Spawn` of the main thread, tag 0, no cleanup, one argument) P_EX330, P_EX350, P_AL010 and N_HB040_BTL also call it: they now get a real child thread (WP-LT2 review). |
| 2.55 | `attack_word_38(p) -> i` | 0x18ea50 | 36 | word at +0x38 |
| 2.56 | `ptr_b40_4_b(O) -> p` | 0x18de58 | 8 | the same function as ptr_b40_4 under a second number |
| 2.59 | `limit_task_59(pppp) -> i` | 0x18eb00 | - | FUN_001b3c48 -> FUN_001db868(task + 0x18, a1, a3, 1, 30000, *(a2 + 4)) (WP-LT1: logged) |
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
| 2.75 | `tinker_bell_ready() -> b` | 0x18eba8 | 1 | FUN_001c69b0: a summon is out (DAT_01d5e368), it is the sumn row of command 0x132 Peter Pan (FUN_001d4ca0) and DAT_00340c88 byte 7 is set (Tinker Bell's revive not used yet, cleared by FUN_001adeb8) |
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
| 2.89 | `limit_release_animation(p)` | 0x18eb60 | - | FUN_001b3b60(task): when bit 8 of task +0x40 is set, a call through the player's +0xb44 object, task +0x3c -= 1, bit 8 cleared (WP-LT1: the count and the bit; the call is a gap) |
| 2.91 | `hit_reaction_class(p) -> i` | 0x18e260 | 11 | reaction class of the hit (FUN_0018b700 on the byte at +0x26) |
| 2.92 | `hit_kind(p) -> i` | 0x18e290 | 21 | FUN_0018b748 of the byte at hit +0x27 |
| 6.2 | `player_word_b50() -> i` | 0x2ee668 | 8 | word at +0xb50 of the player object (pointer at 0x35c998) |
| 6.9 | `target_distance(pv) -> f` | 0x2eeb30 | 55 | the distance from the point to the surface of a target record's shape (FUN_0018f308: FUN_0018eec0 builds the shape, FUN_00189628 measures; a sphere's is the centre distance minus the radius, a spheroid's is measured with the height scaled to the radius) |
| 6.23 | `part_nearest(Ov) -> i` | 0x2ee858 | 0 | FUN_002ee858: the part id (byte +1 of the collision record of type 2) of the object whose part point lies nearest to the vector; -1 when it has none |
| 6.24 | `obj_vec_8a0(O) -> v` | 0x2ee960 | 12 | the 16 bytes at object +0x8a0 copied to a static buffer |
| 6.47 | `release_stop_id_all(i)` | 0x2eefe0 | - | FUN_0016bf78(id): for every object FUN_0016bed8 -> FUN_001d59a8(object + 0x518, id) removes the stop id from the three-slot table (WP-LT1: a no-op, the app holds no stop 17) |
| 6.49 | `object_occupied(O) -> b` | 0x2ee7d8 | 1 | FUN_0019b088: the object's +0x518 bit 0 (actor stop, FUN_0016bea0), or FUN_0019af28 (its +0x588 bits 4 and 0x40 clear and the summon state, FUN_001c6918, is 1), or its +0x108 bit 0x400, or the manager at DAT_0036f6a4 holds an entry (FUN_002c5680) |
| 6.58 | `queue_mode_remote(ps)` | 0x2ef018 | 3 | queue the named mode and mark the entry for another object: when it comes up the host switches that object to the mode instead (FUN_00186f50; see runtime.rs) |
| 7.0 | `release_actor_stop(i)` | 0x2ef2f0 | - | FUN_00184b40(id): the actor stop id is released on every enemy (Stitch's limit passes 17); the app sets no stop id but 2, so it changes nothing there |
| 7.19 | `ai_retarget(Oi) -> b` | 0x2ef760 | 4 | FUN_0010df08(object, flag != 0): the class's target refresh (the chooser of the class table at 0x352d28 / 0x352ce8, Peter Pan's FUN_00119d60) writes the object's target record +0x9f4; true when the target changed (+0xb7c bit 3) |
| 7.24 | `ai_b7c_400_clear(O)` | 0x2ef848 | 4 | FUN_0010f6b8: object +0xb7c bit 0x400 cleared (Peter Pan's attack modes bracket the attack with 7.24 / 7.25) |
| 7.25 | `ai_b7c_400_set(O)` | 0x2ef868 | 6 | FUN_0010f6d0: object +0xb7c bit 0x400 set |
| 7.26 | `ai_run_callback(Oi)` | 0x2ef888 | - | FUN_00112518(object, command): the callback the class set at object +0xcfc (Stitch's FUN_00119488 for his limit sub-commands 525 to 527) runs with the command (docs/systems/drive-limits-summons/contract.md) |
| 7.28 | `screen_effect_start()` | 0x2ef7b0 | - | FUN_002f3bd8: FUN_002f3aa0(FUN_00104fa8(0xa00), FUN_0016e208(0x5c3), 0x100), stores the object in DAT_00371518 (WP-LT1: not drawn) |
| 7.30 | `screen_effect_end()` | 0x2ef820 | - | FUN_002f3c18: ends the object in DAT_00371518 (vtable +0x20) and clears the word (WP-LT1: not drawn) |
| 10.1 | `ai_walk_to(Ov)` | 0x2f6a98 | 3 | the friend AI object (+0xb60, FUN_0010c5f0) is placed at the vector: FUN_001175a0 sets AI +0x630 bit 0, copies it to the position +0x640 (its w to the wanted yaw +0x65c, the depth +0x648 = DAT_00378840) and once builds the parent matrix (FUN_00117320, FUN_00193228); docs/systems/drive-limits-summons/contract.md, Status (WP-SUM4b) |
| 10.3 | `ai_vcall_28(O)` | 0x2f6b28 | 3 | the friend AI object's virtual method +0x28 (Stitch FUN_001191a8: links the player and his hand objects to the summon for a limit) (by class, WP-SM2: Stitch FUN_001191a8; Chicken Little FUN_0011ae60, the FPS mode; Genie 0x354910, Peter Pan 0x354978 and Tinker Bell 0x3549b8 hold FUN_00114170, which clears the action (FUN_0010ecb0(object, 0, 0)) and breaks a link the object has (FUN_001becb8)) |
| 10.4 | `ai_vcall_2c(O)` | 0x2f6b58 | 4 | the friend AI object's virtual method +0x2c (Stitch FUN_00119360: undoes FUN_001191a8) (by class, WP-SM2: Stitch FUN_00119360; Chicken Little FUN_0011b008, the FPS mode off; Genie, Peter Pan and Tinker Bell FUN_001141c0, which returns at once) |
| 10.5 | `ai_aim_vec(O) -> v` | 0x2f6b88 | 1 | Chicken Little's AI +0x40 (FUN_0011a6a0): the aim point the class update keeps (the target record's position, else 200 units in front of the object, FUN_0011a2e8) |
| 10.8 | `target_vec(O) -> v` | 0x2f6c20 | 1 | the 16 bytes at object +0xbe0 (FUN_00112538): the position the party manager copies from the object's target record (FUN_00111d18) |
| 10.9 | `ai_genie_form(Oi)` | 0x2f6c68 | 1 | Genie's FUN_00119c18 on the friend AI object: AI +0x14 = the command; the object's parts 2..6 hidden (FUN_00192ce8) and the part of the command's row in the table at 0x3548f0 shown (row + 2, FUN_00192cd0); byte 7 of `DAT_00340c88` = row + 1, or 6 without a row (FUN_001c6b48); docs/systems/drive-limits-summons/contract.md, Status (WP-SUM5) |
| 10.10 | `ai_genie_limit(O) -> i` | 0x2f6ca0 | 2 | Genie's FUN_00119ca8: the limit command paired with AI +0x14 in the table at 0x3548f0 (five pairs), 0 without |
| 10.12 | `ai_slot_start(OiOiiiii) -> i` | 0x2f6d00 | 5 | FUN_00117708 on the friend AI object: a slot of its 0x18-slot list at +0x20 with the kind, the object of the third argument's context (0 without), four words and a flag; returns the slot index |
| 10.13 | `ai_stitch_13(Oi)` | 0x2f6d68 | 1 | Stitch's FUN_001195e8 on the friend AI object: AI slots of kind 1 and, with a target, attack objects of atkp 0x373 around it (FUN_001130d0) |
| 10.16 | `ai_slot_mark(Oi)` | 0x2f6df8 | 1 | FUN_00117840: AI slot n flags |= 0x40000000 |
| 10.17 | `flag_588_800(o) -> b` | 0x2f6e30 | 5 | object +0x588 bit 0x800 |
| 10.18 | `flag_588_400(o) -> b` | 0x2f6e48 | 4 | object +0x588 bit 0x400 (a boss: FUN_001acf20) |
| 10.19 | `in_limit(o) -> b` | 0x2f6e60 | 18 | object +0x10c bit 0x10000 (the limit task marks its participants, FUN_001b2c10) |
| 10.20 | `ai_follow(O)` | 0x2f6e80 | 2 | FUN_001102c8 (the members' follow step with the class speed rule), then the object's +0x10 / +0x20 vectors take the results at +0xc00 / +0xc20 (FUN_0010f7f0 / FUN_0010f820) |
| 10.22 | `obj_entry_flag_40(o) -> b` | 0x2f6f60 | 0 | the object's mover (+0xc) has bit 0x40 in its flags word +8 (a blow mover: the object is knocked away) |
| 10.23 | `target_is(OO) -> b` | 0x2f6f88 | 1 | object +0xae4 (the last attacker) is the second object |
| 10.25 | `ai_join_ready(O) -> b` | 0x2f6fe8 | 4 | FUN_0011a1a8 -> FUN_00119ce8 on object +0xb7c: bit 0x200 clear, the player's attack window (DAT_0034e9d0 >= 0) and DAT_0034e9c8 + 8 >= DAT_0034e9cc |
| 10.26 | `ai_join_mark(O)` | 0x2f7020 | 1 | when 10.25 holds: object +0xb7c |= 0x200 (FUN_0011a1d0) |
| 10.27 | `ai_enter_27(O)` | 0x2f7070 | 3 | FUN_0010f6e0: class 0x20 -> motion request 0x1f; else object +0xb7c bit 0x4000 cleared |
| 10.28 | `ai_enter_28(O)` | 0x2f7090 | 3 | FUN_0010f728: class 0x1f -> motion request 0x20; else object +0xb7c bit 0x4000 set |
| 10.29 | `ai_use_item(OiOi)` | 0x2f70b0 | 4 | FUN_0010c9d0(object, item, target object, arg): +0xb78 = 0, the target record names the target, the item's command is started with motion request 9 (FUN_00175e20) |
| 10.30 | `entry_flag_40_in_limit(o) -> b` | 0x2f70e0 | 1 | the object's mover (+0xc) has bit 0x40 in its flags word +8 (blown) and the object's +0x10c has bit 0x20000 |
| 10.31 | `is_summon(o) -> b` | 0x2f7118 | 66 | object +0x588 bit 0x400000 (a summon object) |
| 10.32 | `ai_stitch_32(Oi)` | 0x2f7138 | 1 | Stitch's FUN_00118aa8 on the friend AI object: a goal (FUN_00116780), motion 0xa1 and the move record (the one FUN_00114280 finds with handler FUN_00117ea0) made the current action (FUN_0010ecb0), +0xb7c |= 4 |
| 10.33 | `recent_target(i) -> o` | 0x2f7170 | 2 | FUN_001127f0(-1): the most recent object of the player's lock-on list (8 entries at 0x34e988, FUN_00112770 appends); a non-zero argument then drops it and every older entry (FUN_00112668 with the flag set) |
| 10.34 | `recent_target_set(oi)` | 0x2f71d0 | 1 | FUN_00112668(object, flag != 0): flag clear removes the object from the list; flag set drops it and every older entry (the entries after it stay) |
| 10.35 | `ai_find_entry(Oi) -> addr` | 0x2f71f0 | 5 | FUN_001141d0 on the friend AI: the address of the first 0x80-byte entry of the AI's class list (object at AI +8, count +0xa0, list +0xa4) whose first word is the id, 0 for none; scripts then read the entry's fields. App: the class block's list in the executable (`ClassAi::entries`), served through the friend host's `read_mem` (WP-PY2) |
| 10.36 | `flag_160_free(O) -> b` | 0x2f7230 | 2 | object +0x160 has bit 4, 8 or 0x10 |
| 10.37 | `flag_160_40000000(O) -> b` | 0x2f7270 | 2 | object +0x160 bit 0x40000000 |
| 10.39 | `ai_action_clear(O)` | 0x2f72c0 | 5 | FUN_0010ecb0(object, 0, 0): object +0xc60 and +0xc64 cleared |
| 10.41 | `ai_flag_b7c_40000000(O)` | 0x2f7328 | 4 | FUN_001129f0: object +0xb7c |= 0x40000000 |
| 10.44 | `objentry_flag_10(O) -> b` | 0x2f7390 | 0 | the object's mover (+0xc) has bit 0x10 in its flags word +8 |
| 10.45 | `status_f_1b4(p) -> f` | 0x2f73b0 | 4 | the float at +0x1b4 of a status block (1.84): the drive / summon timer |
| 10.50 | `status_word_180(p) -> i` | 0x2f7440 | - | the word at +0x180 of a status block (disassembled: lw a0[0]; lw 0x180; sw a0[0]): the member's current MP (docs/systems/party/party-friend-ai.md section 5). No script on the disc calls it (`kh2 bdx --scan` and a scan of every BDX list no 10.50; the earlier note "10.50 x75" in docs/systems/party/contract.md has no source) (WP-RV2) |
| 10.51 | `ai_find_target(Opff)` | 0x2f7450 | 2 | FUN_0011a6d8(half width, half height, the AI object, record): the record names the valid enemy whose projected position lies in the screen box, the nearest to the centre (Chicken Little's FPS aim) (runtime: the record, the second argument, is written as the object found, 0, 0, -1; an empty record is object 0 and -1 in word 3, FUN_0018ed40; WP-SM3). Word 4: on a hit FUN_0011a6d8 copies FIVE words (the iterator FUN_0018f348's object, then its entry byte into word 1, 0, -1, and a fifth from the iterator's stack copy), but FUN_0018ed40 clears only four and nothing in FUN_0018f348 / FUN_0018f490 writes the fifth, so the original copies an uninitialised stack word into word 4; the arm writes four words and leaves word 4 as the script had it, which is the only defined behaviour [inferred: nothing reads word 4 in the scripts checked]. Word 1 in the original is the first byte of the object entry the iterator found (an index), the arm writes 0 |
| 10.53 | `effect_list_add(Op)` | 0x2f74c0 | 2 | FUN_00136388(the object's list at +0xb5c (FUN_0010c5b8, made on first use), effect instance): an (instance, generation) pair is entered, an older entry of the same instance dropped first, the oldest when 0x20 are held; a zero instance is ignored (Chicken Little's child registers its effect with the summon) |
| 10.54 | `effect_list_has(Op) -> b` | 0x2f74f8 | 2 | FUN_001364d0 on the object's list at +0xb5c: the instance (with its generation) is in the list |
| 10.58 | `attach_record(Opi)` | 0x2f7598 | 2 | FUN_001131e8(the friend object, record, value): the bit 2 of the word +0x1c of the object the record's first word points to is set, the friend object's +0xb50 = the record and +0xb54 = the value. The only reader in the friend range is the destructor FUN_0010c6e0, which unlinks and frees +0xb50 (FUN_001a4e00: the list at DAT_01c95518, chained at +0xc) and frees +0xb54 (FUN_00104fe0); nothing else reads the pair (`mips_scan --load-off 0xb50` / `0xb54`: the other hits are in the player and effect classes, which keep other fields there). A Stub is exact unless the +0x1c bit has a reader, not traced. The two calls on the disc are at 0x00de of `obj/P_EH000.mdlx` and `obj/P_EH000_LAST.mdlx` (Riku) (WP-RV2) |
| 10.59 | `ai_set_aim_vec(Ov)` | 0x2f75c0 | 2 | FUN_0011a6b8: AI +0x40 = the vector |

