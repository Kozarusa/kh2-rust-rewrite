# Enemy AI runtime: Shadow, Soldier and Dusk under their BDX scripts

**Files (since WP-7R; the code was one file, `enemy_ai.rs`, before).** `crates/kh2-app/src/enemy_obj.rs` holds the object as the script sees it:
`Obj` with its fields and small methods, the handle and size constants (`PLAYER`, `TARGET`, `GAUGES`, `VOLUME`, `OBJ_SIZE`, ...) and the
game-space conversions (`to_game`, `from_game`, `yaw_of`). `crates/kh2-app/src/enemy_host.rs` holds the host-call dispatch: `BevyHost` and
`Env`, the answer helpers (`imp`, `approx`, `stub`, `missing`), `call_id`, and the arms of every host call split by group into `group0`,
`group1`, `group2` and `other` (new arms go into the function of their group; inside a function the order of the arms matters, the first
match wins), plus the pure helpers the arms use (`game_wrap_angle`, the prize table and `compute_drop`). `crates/kh2-app/src/enemy_ai.rs` keeps
the plugin and the systems (`load_programs`, `ai_attach`, `ai_update`, `ai_fade`), `AiBrain`, the `Mailbox` and the `AiTrace` log.
Where this document says "`enemy_ai.rs`" for a host call or the object, read the file named here.

(WP-6C added the Dusk, `obj/M_EX990.mdlx`, WP-6H `N_EX570_BTL`, WP-7D `M_EX930`, `N_EX500_BTL` and `N_EX600_BTL`; why, their stats and where they spawn are in `docs/enemies.md`.
The kinds are `enemy::KINDS`; `load_programs` reads every kind's script.)

Work package 2E. The Heartless of `enemy.rs` used to run a small state machine written for the app. Shadow (`obj/M_EX020.mdlx`) and
Soldier (`obj/M_EX010.mdlx`) now run the game's own scripts (the BDX entry of each object file, see `docs/bdx.md`) through
`kh2_bdx::Runtime`. The code is `crates/kh2-app/src/enemy_ai.rs`; `enemy.rs` keeps only the hit reactions (WP-I1 removed the placeholder state machine and `KH2_NO_BDX`; an enemy whose script did not load only stands and reacts to hits).

Nothing from the disc is stored in the repository: the scripts, the motion sets and the `objentry` row are read at start-up.

## How it runs

- `ai_attach` gives every new Shadow and Soldier an `AiBrain` (a `Runtime` and the script's view of the object) right after the wave spawner.
  The script's first frame is the frame the enemy appears in.
- `ai_update` runs once per 60 Hz tick in `Sim::Enemy`, before `enemy::think`. Per enemy it (1) refreshes the object from the world,
  (2) delivers the death and the hit events, (3) calls `Runtime::update` (main thread, children, mode manager) and (4) carries out what the
  script asked for. `ai_fade` applies the fade to the materials.
- `enemy::think` skips a scripted enemy unless it is in `Hurt` or `Blown`; those two states are the hit reactions of WP-2A and are
  unchanged. `receive_hits` also writes an `AiHit` message (the hit and whether the effective reaction's class is 2 or more); the next
  `ai_update` delivers it as event 6.
- The interpreter and the mode manager in `kh2-bdx` are untouched, so the oracle comparison against the game's interpreter still holds.

`KH2_BDXLOG=1` prints, per enemy (`Shadow#2` is the second enemy spawned): every mode that starts, every state-changing host call with
its arguments and result, every motion request, events, a status line once a second (mode, motion slot, distance to Sora, hp), the
first use of each host function with its classification, and a table of all host calls with counts every 900 ticks.
`KH2_BDXLOG=all` also prints the per-frame queries, the root motion of every playing motion and the fade. An unimplemented host
function is always logged, once per function, with `group.index`: `unimplemented host call 1.133 (...) ...`.

## The object as the script sees it

Handles: the script's own object, the player (`0x0200_0000`), the player's target record, effects and hit records are opaque numbers.
The object's own 0x1000 bytes of memory are readable and writable by the script (a write over `+0x10..+0x20` steers like `set_move`);
a write anywhere else is stored and logged once.

Coordinates. Scripts see the game's space: centimetres, Y down, Z mirrored against the app's (the app is the game rotated half a
turn about X, as `docs/camera.md` and `fx.rs` say). `to_game` / `from_game` convert; a facing of yaw `a` in the app is
`(sin a, 0, -cos a)` to the script, and `yaw_of` turns a script direction into the app's yaw (unit tests in the module).

Motions. The script's motion number is the slot of the object's motion set (`kh2 mset obj/M_EX010.mset` lists them: Soldier's
claw swipes 201 and 202, kick 203, jump slam 204, appear 44, steps 151 and 152). Inferred from the scripts and the sets agreeing
(for example 201..204 are exactly the slots with attack triggers); the game's slot function (FUN_0017da30) also returns the number
itself when its controller test fails, which is what an object without state variants would do. A motion loops when its return frame
is below its end frame (`frame_return < frame_end`: Shadow's idle 47 and walk 2 loop, appear 162 and the attacks do not).

## Host call mapping

Status: Impl = does what the game's function does; Approx = the visible part, with the stated difference; Stub = accepted, answered
with a default, nothing in the app reads it; Missing = fails visibly. Addresses are in the USA executable.

| call | status | what it does here |
| --- | --- | --- |
| 1.3 player, 2.88 last_attacker, 2.23 target | Impl | the player, the player, the player's target record |
| 1.147 position, 1.201 facing_vector | Impl | position and facing of self or the player, in game space |
| 1.120 target_position | Approx | the player's origin (the game uses the lock-on part's point, FUN_0018efa0) |
| 1.121 set_target_mode, 1.131 free_target | Stub | the lock mode of a target record has no consumer; 1.131 is really the allocation of a pooled copy of the record (FUN_0018f3f8, pool of 0x80 records, WP-RV2, docs/bdx.md) |
| 1.11 play_motion, 1.61 play_motion_at | Approx | restart the slot on the `Animator` with the blend in ticks (at a start frame for 1.61); the motion controller's queue is not ported |
| 1.12 play_motion_checked | Approx | as 1.11 unless the slot already plays (FUN_0017e2d8 / FUN_0017e278 compare the running motion with the requested one) |
| 1.14 motion_done | Impl | the motion reached its end during the last tick; looping motions report it each time they wrap (FUN_0017e3b8 sets bit 1 of the controller flags at every end, loop or not) |
| 1.15 motion_state | Impl | the slot playing (the game's +0x154, written with the requested number), -1 before the first motion; a reaction changes it behind the script's back |
| 1.67 motion_length, 1.68 motion_end | Impl | the motion's end frame and its return frame, in the motion's own frames (FUN_0017f4e0, FUN_0017f558) |
| 1.4 face_direction, 1.79 face_toward | Impl | facing set at once to atan2(x, z) of the direction (FUN_0016a7f0) |
| 1.80 turn_toward | Impl / Approx | at most `speed` radians per tick, true while still turning, snaps to the target within a step (FUN_00183848). Speed 0 means the object's own rate (+0x11c): that value was not found, the placeholder's 5 rad/s is used |
| 1.119 probe_ground | Impl | FUN_001834f8: a line from `a` above the object, `w` ahead; a wall gives 3; then the floor under the far point, searched down to `b` below the object: none gives 3, higher than the tolerance 1, lower 2, else 0. The wall test and the floor lookup are the collision of `world.rs`; the tolerance is 40 cm (the game: a data-record halfword times 0.4, not decoded) |
| 1.55 start_move, 1.168 start_move_alt | Approx | plays the motion and jumps (see Movement); the alternate setter FUN_001b4ee0 was not read |
| 1.111 set_move | Approx | horizontal movement for one tick (direction normalised, speed in w, centimetres per tick); the vertical part is ignored |
| 1.57 airborne | Impl | the mover is not on the ground |
| 1.60 busy, 2.5 hit_stun | Approx | a hit reaction plays (`Hurt` or `Blown`) |
| 1.56 on_screen | Approx | the origin one metre up is inside the camera's view |
| 1.124 flag_124 | Impl | bit 3 of the `objentry` flags halfword (FUN_0019aaf0); false for both objects |
| 1.309 flag_309 | Approx | true for the player (status bit 0x80, the party class) |
| 1.70 / 1.71 set_flag / clear_flag | Impl | bit n of the object's flag array. Bit 3 means "cannot be hit": FUN_0019a4f0, the victim test of the attack code, requires bit 3 clear, bit 2 of +0x7f8 clear and HP above 0, and `Fighter::hittable` follows it. Other bits are stored |
| 1.65 set_flag_26, 1.104 / 1.105 flag 768 | Stub | no consumer in the app; none in the executable either for bit 6 of +0x768 (WP-RV2, docs/bdx.md) |
| 1.19 fade_out, 1.20 fade_in, 1.28 hide | Impl | alpha tween 1 to 0 and 0 to 1 over the given frames (blended materials while it runs); hide sets alpha 0 and the invisible flag (FUN_00170f78) |
| 1.87 spawn_attached | Approx | element `id` of the object's effect file, bound to it; id 2 goes through `GameEffect::Death` (the death effect, `fx::EFFECT_DEATH`); the flag and extra arguments are ignored |
| 0.79 release_effect | Stub | effects run to their end |
| 2.9 set_obj_param | Impl | the game builds the object's status block from the `enmp` row (FUN_001b2148 -> FUN_0019f8a8); the app built it at spawn, the call checks the row number (Shadow 34, Soldier 33) |
| 1.110 spawn_kind | Impl | byte +0x1c of the spawn record (FUN_0016b858): the area group's value for enemies spawned from an area script (`docs/enemies.md`), -1 for the app's own waves |
| 0.14 progress_flag_set | Impl | FUN_0016d5a0 -> FUN_0017ff60 on the app's story (`progress::Story::set`: the bit at 0x33f6b0 and its `00progress.bin` side effects, docs/events.md section 5); Stub (not kept) when no story is installed (unit tests) |
| 0.15 progress_flag_test | Impl | a bit of the story progress array at 0x33f6b0 (FUN_0016d5b8 -> FUN_001802e0) read from the app's story (`progress::shared_story`, WP-E5): clear in a new game, set by `KH2_STATE` `story_flags`, area-script opcode 0x19 and host 0.14. Stub (all clear) without a story. The Dusk uses flags 2267, 2256, 2122, 2091 only to pick its drop row |
| 0.70 progress_flag_102b | Impl | FUN_0016d5e8 -> FUN_00180330: flag 0x102b of the same story |
| 1.63 motion_near_end | Impl | FUN_0017f400: a motion plays, no blend left (the request's blend, run down per tick) and the controller time has reached the value of the motion's method +0x1c, read as the return frame [inferred: FUN_0017e3b8 wraps a loop to time - end + that value] |
| 1.69 obj_f_170 | Impl | object +0x170 is the motion controller's (+0x140) time (+0x30): the playing frame, or the first frame of a motion requested this tick |
| 1.66 set_flag_27 | Stub | bit 27 of +0x108, no consumer |
| 1.74 mover_reset | Approx | FUN_00171548 ends the mover and installs mover type 0; the app clears the movement and jump speeds |
| 1.98 ground_position | Impl | the object's position with the floor height under it: a line from 200 above down 100000 (FUN_00183750 with DAT_0037a894, DAT_0037a890); no floor keeps the height |
| 1.99 ground_depth | Impl | (WP-EN10c) FUN_001730e8: the floor height of FUN_00183808 (as 1.98) minus the object's height in the game's Y-down space, kept only when positive, answered negated: in the app minus the height above the floor, 0 on or below it or with no floor. The branch of FUN_00183808 that returns object +0xe0 (flag bits 0x20 and 1 of the descriptor, `+0xc -> +8`) is not read. Used by `M_EX660` (0x13e5, 0x13f7) |
| 1.59 mover_bit1_or_busy | Stub | (WP-EN10d) 0x1729c0: bit 1 of the mover's word +8 (mover at +0xc of the object's +4 record) or bit 0x40 of its +0x160, the no-ground-pull test of FUN_00181e00 and FUN_001729f8. The writers were not found: read clear (grounded). Used by M_EX520 (0x0ef8 wait loop, 0x0f56 chase height) |
| 1.162 bind_register_162 | Stub | (WP-EN10f) 0x173798 -> FUN_0018ffb0 -> FUN_001cb530(table 0x1c89b90, arg1, word +4 of arg0): enters the object that 1.143 stored at +4 of the shared block into an 8-slot table (tag arg1), only when FUN_0019a3f8 finds it in the object list. 1.143 answers no object in the app (Stub), so the word is 0 and nothing is entered. The table's reader was not traced. Used by M_EX210 (0x1a69, once, from the shared-block set-up R1a25) |
| 6.24 obj_vec_8a0 | Approx | (WP-EN10f) 0x2ee960: the 16 bytes at +0x8a0 of the object's +4 record, copied to the static buffer 0x1e25680. FUN_00182690 writes them each tick as (position +0x5c0 - previous position +0x540 - the offset +0x5a0) times DAT_00356f48; the app answers the displacement made this tick (as 1.62), the +0x5a0 offset taken as zero [inferred]. Other objects: Missing. Used by M_EX210 (0x105f: its length against half of 1.64's ends a chase loop) |
| 1.150 / 1.151 spawn_bit_238 / 239 | Impl | bit n-1 of object +0x238 / +0x239 = the motion controller's (+0x140) bytes +0xf8 / +0xf9: range triggers 16..19 set bits 0..3 of +0xf8 (FUN_00188030, cleared per pass by FUN_00187fb0); frame triggers 9..12 set bits 0..3 of +0xf9 (dispatcher 0x188498, cases 0x1886c0..; cleared by FUN_0017e3b8 before each advance). WP-6H; this changes the Dusk's 1.150 answers too |
| 1.64 / 1.62 obj_vec_850 / 860 | Approx | written each tick by the movement step FUN_00181e00: the displacement the object asked for and the one it made after collision, times the frame-step factor DAT_00356f48 (1.0 at start, taken as 1). The app gives the tick's horizontal request plus the jump/fall step, or for a grounded object the root's vertical change minus one tick of gravity [inferred: a grounded mover presses down]; the real one is new minus old position. The Dusk's sinking and landing loops end when the real downward movement is under a tenth of the asked one |
| 1.39 spawn_param | Impl / Stub | word i of object +0x81c (FUN_0016b940): words 1 and 2 are the spawn record's +0x20 / +0x24 (FUN_0016b870) for area-group enemies; 99 for the app's own waves (no record) |
| 1.84 gauges, 1.228 / 1.229 / 1.232, 1.230, 1.231 | Impl | (WP-6H) the status block (+0x48c); records of 12 bytes (current, maximum, lowest HP per part, FUN_0019f770); part 0 only. 1.231 sets the lowest, which the HP change FUN_001a0100 never goes below (`Enemy::min_hp`) |
| 1.155 obj_method_78 | Approx | virtual +0x78 = FUN_00186238 in 27 object tables (the N_EX570 table not traced): HP + amount, clamped; applied after the tick. On the PLAYER from Peter Pan's script (WP-SUM7, `summon.rs` `SummonHost`): amount 5, part 0 -> `magic_rules::healed`, applied to Sora's HP after the script ran |
| 2.21 shared_block, 2.75 tinker_bell_ready (WP-SUM6), 0.40 effect_set_vec2_raw (WP-SUM7) | Impl / Approx | summon objects only (`SummonHost`): keyed buffers (kh2_bdx::shared), the revive-ready byte, the effect's second vector taken as the angles 0.41 stores |
| 2.21 shared_block (WP-EN9b) | Impl | enemy scripts too (`BevyHost`, M_EX760's R1ddd): one `SharedBlocks` list for all enemy scripts, emptied at the area's group set-up; `read_mem` / `write_mem` fall back to it |
| 1.58 set_dash | Approx | +0x20 = normalised direction times w, +0xf0 = factor k, mover 5 (table 0x35e4d8, FUN_0019e740 -> FUN_001832b8): v = v*k + move*(1-k) with a movement vector, else v*k. Ended by 1.74 and start_move |
| 1.211 / 1.212, 2.76 | Stub | bits of +0xabc (readers FUN_00185be0 / FUN_0018c6d0 test 9, 10, 11, 13; N_EX570 uses 1 and 12) and a float at +0xaf4: no reader found |
| 1.262 obj_method_54 | Stub | voice line request (FUN_001970b8 -> FUN_001ae798 priority slot at +0x9b8); no enemy voice in the app |
| 2.9 set_obj_param (WP-6H) | Impl | a row other than the kind's rebuilds the stats with full HP (N_EX570_BTL: 135 or 155) || 1.23 global_byte_33e830 | Approx | 2, the world number of Twilight Town |
| 1.171, 1.206, 1.34, 1.301 | Stub | global flags and a global byte set by other scripts or the game: false and 0; 1.301 is a constant 0 (the NUL after a string at 0x35ea48, WP-RV2, docs/bdx.md), so exact |
| 1.130 obj_diff | Impl | the two objects' difference, 0 when either is 0 or both are the same (FUN_00170510); M_EX950 R1fe7 asks it before `mode_revenge` (WP-EN9e) |
| 1.140 target_valid | Approx | FUN_00173740 -> FUN_0018ed58: a target record is valid when it names an object (or FUN_0018ee20 finds one); the app's record is always the player's, so any non-zero record is valid (WP-EN9e) |
| 1.179 set_word_100, 1.75, 1.76, 1.128 mover_method | Stub | 1.179 stores a word at object +0x100 (FUN_001713a8); 1.75 / 1.76 set and clear word +4 of the motion controller at +0x140 (FUN_0017d9c8 from a FUN_001a7da8 lookup, FUN_0017da18); 1.128 calls method +0xc of the mover (FUN_00171298): no reader or role was traced, nothing in the app consumes them (WP-EN9e) |
| 1.143 bind_object_143 (spawn record byte +0x1e) on an enemy script (WP-EN10a) | Stub | FUN_0016b0d8: the object whose spawn record byte +0x1e equals the argument. The app keeps no such byte per object, so the answer is 0 (an `enemy_host` arm for `ObjectKey::SpawnGroup`, before the player-only arm). M_EX650 asks once, for 92, from its shared-block set-up (R15f9 at 0x1628) and never reads the answer [inferred: not found in the original either when no member carries 92]. The interpreter then fails to write the answer into the shared block (kh2-bdx runtime.rs writes a context through `Mem::write_u32` without the host fallback that `write_bytes` has): one `script error BadAddress(0x7a000014) at 0x162a` per kind, which ends only that set-up routine |
| 2.6 in_reaction, 2.67 new_hit_group, 6.9, 0.45 / 0.46, 0.68, 1.13 on M_EX110 (WP-EN10b) | Approx / Impl | 2.6 (FUN_00185a08): motion state 10..=39, Approx on the app's slot. 2.67 (FUN_001bdf00): the counter at 0x1d5dd4c in `enemy_host.rs`, starting at 0. 0.68 (FUN_0016cf78, vector turned about X) is `kh2_bdx::math::rotate_x`, proved by `tests/rotate_x_oracle.rs`; 6.9 target_distance Approx (distance to the player's origin); 0.45 / 0.46 Approx through `friend_host::advance_vec` / `turn_vec`; 1.13 on M_EX110 queues like M_EX900's (its `atk_foot_stamp` waits for the state to leave 202) |
| 1.13 queue_motion on the object itself | Stub | the controller's queue (+0x3c, FUN_0017e170) is not in the app: M_EX950 queues motion 0 after its attacks and the next mode starts its own motion (WP-EN9e) |
| 2.8 hit_is_real | Impl | the effective reaction's class is 2 or more (FUN_0018b700 on the hit record's byte +0x26) |
| mode manager hooks | Approx | `ModeInterrupted` and `ModeMayStart` follow FUN_00186b70 and FUN_001873d0 read as "a hit reaction plays" and "not in hit stun"; `ModeFrozen` (FUN_00186b38, object flag 0x40000) is false because hit stop is not implemented |
| 1.13 on the object itself, 1.311, 1.213, 2.21 and 2.45 on enemies (WP-EN9d, M_EX900) | Approx / Impl | 1.13 queues (slot, blend) in `Obj::motion_queue` (4 at most), started by `enemy_ai` when the playing motion ends [inferred]; enabled for M_EX900 only until verified for the others (`queues_motions`: M_EX930, M_EX940, M_EX850, M_EX760, M_EX420 keep the unmapped answer); 1.311 `Obj::weak_scale` -> `Stats::weakness` (FUN_001a0ba0); 1.213 answers 1 (`NeoStatus` of Sora's row, the target is the player); 2.21 a process-wide `SharedBlocks`; 2.45 the `AiHit::atkp` of the hit. See docs/enemies.md "M_EX900" |
| 1.29 play_motion_with, 1.238 obj_word_158, 1.13 queue_motion, 1.89 set_velocity, 1.70 / 1.71 on the player, 1.14 on the player | Approx | the calls of Soldier's reaction thread on the player: handed to the player side as `PlayerReactionCmd` (no consumer yet), the paired motion's length gives the player's `motion_done` |
| 0.47 alloc / 0.48 free, 2.20 start_request, 1.132 target_release | Impl / Stub | the script heap and the detached request thread are the runtime's own (see `docs/bdx.md`); target records are not pooled |
| 1.92 ground_marker | Approx | the drop (WP-6G, below): row `id` of the prize table `przt` is looked up and the drop computed as FUN_001a3cb0 does; the result is logged and its prize objects run in `prize.rs` (WP-7B), its item objects too (WP-7Bb, `ItemDrop`) |
| 6.31 (no arguments) | Impl | wrapper FUN_002eece8: FUN_001a40b8 (every pickable prize drawn into the player in mode 1 and paid) and FUN_001bda98 (every item object handed to the player); sends `prize::CollectAll` (WP-7Bb). No BDX script on the disc calls it |
| 1.133, 1.134, 1.128 | Missing | in the scripts but not reached in a fight: two virtual methods of the object, a virtual call on the mover (1.89 on the enemy itself too: the secondary velocity of a scripted object has no consumer). They would be logged when first called |
| 1.90 send_message, 1.241 | Approx | the global message queue (WP-6G, below): every scripted enemy receives the message at the start of the next tick, with its own delay from the generator at 0x3716a0. Objects without a script (the player) draw no delay |
| 0.42 angle_of | Approx | atan2(v.x, v.z) (FUN_0016d060 -> FUN_003003e8); the platform's atan2, not the game's routine |
| 0.43 wrap_angle | Impl | FUN_0017bbc0: a positive angle `fmod(x + pi, 2 pi) - pi`, else `pi - fmod(pi - x, 2 pi)`, with the floats at 0x37a528 / 0x37a52c |
| 0.21 angle_between | Impl | the wrapped angle from a to b: `wrap(atan2(b.x, b.z) - atan2(a.x, a.z))` (0x16cfd0, FUN_0017bbc0 for the wrap); answered by the enemy host (group 0) since WP-E2b, the townspeople use it too |
| 1.94 obj_valid | Approx | 0x170150: 0 is false, else "in the object list and FUN_0019a440 clear" (FUN_0019a3f8). Self and the player are true (the +0x108 bits 0x80000 / 0x10000000 of FUN_0019a440 were not traced and read clear); any other handle reads absent |
| 1.101 flag_160_4000 on the player (WP-7D) | Approx | bit 0x4000 of the player's +0x160 (0x1716d0; the same test as FUN_00179538); the word is written through the motion controller's base and no writer of the bit was found (`mips_scan --store-off 0x160`: only FUN_00188ad0, which sets 0x20000): read clear |
| 1.225 mover_method_60 on the player (WP-7D) | Approx | FUN_001708f8: 0 unless bit 2 of the player's +0x588 is set, then his mover's virtual +0x60; the bit's writer was not traced: 0 |
| 1.8 set_mode on the context of an object an enemy script made (WP-EN4) | Approx | FUN_00186db0 on the other object's mode machine. The runtime hands the host `Call::RemoteSetMode { obj, name }`; `Obj::ask_mode` queues it for a handle the script made (`knows`), `enemy_ai::ai_remote` gives it to the object's `Obj::modes_asked` and its script switches at the start of its next tick (the game switches at once). Another object's handle or the host 6.58 queue: Missing. Unit test `a_mode_asked_of_a_made_object_is_queued_for_it`; no enemy of the app was seen making this call in a run |
| 1.18 link_start of a summon-made object on the player (WP-EN4) | Approx | Tinker Bell's `appear` (R0679 of P_EX340): (me, player, 0x4002, 0, 0, 0). FUN_001beb90 stores the parent +0x570, the halfwords +0x574 / +0x576, clears the offset +0x540 and, with bit 1 of +0x576 clear, the facing +0x550. `summon.rs` places the object at the player's origin with his facing each tick [inferred from the cleared offsets: the link mover's own update was not read]; the routine arguments are not run. Summon-script objects only (`SummonHost::own`) The routine and argument (5th and 6th) are stored and run when the link breaks; 1.102 link_end is Approx (WP-SM1). |
| 1.114 bind_object by entity id on a summon script (WP-EN4) | Impl | FUN_0016b078: the object whose id is the argument. Chicken Little's child binds to 1736, the Entity1 of the summon's `sumn` row = his own object; found while he is in his slot (`SummonEnv::main`) |
| 10.53 / 10.54 effect_list_add / effect_list_has, 6.49 object_occupied, 1.277 obj_revive (WP-EN4) | Approx | summon objects only (`SummonHost::own`); see docs/bdx.md rows. 6.49 is true only in the arrival (summon state 1); 1.277 sets the player's HP to the maximum and stands him up |
| 1.161 has_link on the enemy itself (WP-EN10e) | Approx | Same function on the script's own object (M_EX770: R145b 0x1479, R1659 0x165f): +0x570 is the link parent that 1.18 writes; the app writes none: false |
| 1.161 has_link on the player (WP-7D) | Approx | FUN_001706c8 -> FUN_001beb00: the player's +0x570 (link parent) is set. The app has no links (1.18): false |
| 1.18 link_start on the player (WP-7D) | Stub | 0x170dd8 -> FUN_001beb90: parent +0x570, halfwords +0x574 / +0x576, facing +0x550 cleared and 8 or-ed into +0x7f8 (flags bit 1 clear), mover 3, the child entered in the parent's list (+0x760 / +0x764); FUN_00169ee8 ends it when +0x576 bit 0 is set and the parent's +0x160 loses 0x8000, FUN_001becb8 undoes it. The player side has no link mover: Sora is not attached (the Dancer's force dance, `docs/enemies.md`) |
| 2.7 hit_reaction (WP-7D) | Impl | byte +0x26 of the hit record (0x18e0d0): the effective reaction of `reaction_rules::victim_outcome`, carried by `AiHit::reaction` with every hit (2.8 classifies the same byte) |
| 1.95 (WP-7D) | Approx | 0x173038 -> FUN_00183470: mover 1 continuing the last real movement (+0x860): horizontal velocity kept, apex time vy / g and apex height from its vertical speed (g = DAT_0037a884, 0.5 = DAT_0037a888). The app starts a `Motor` jump to that apex or a fall already that fast; not in `kh2_bdx::names` (arrives undecoded, taken as the object's own) |
| 1.54 bits_47c_480_clear (WP-7D, EN2) | Stub | 0x172910 -> FUN_0019dde0: bit n cleared in the bit arrays at +0x47c and +0x480 (object memory since WP-EN2); the reader is FUN_0019a6a0, the per-part collision enable test (WP-RV2, docs/bdx.md 1.52) |
| 1.53 bits_480_set (WP-EN2) | Stub | 0x1728e8 -> FUN_0019ddb0 on +0x47c: bit n of the word at +0x480 + 4 * (n >> 5), set in the object memory (M_EX940 sets bits 5 and 7); no reader was traced |
| 1.97 obj_part_110 (WP-EN2) | Impl | 0x173070: the address of the object's block at +0x110 (`Value::Handle(self + 0x110)`); M_EX940 stores a float at its +0xc |
| 1.108 / 1.109 link_method_3c / 40 (WP-EN2) | Stub | 0x173278 / 0x173298 -> FUN_00192cd0 / FUN_00192ce8 -> FUN_0011c580 / FUN_0011c5b0: virtual method +0x3c / +0x40 of the object behind the link record at object +0x670 (record +0x14), with the record's word +0x104 and the argument; the app has no link records |
| 1.0 create_object (WP-EN3) | Approx | 0x170190 -> FUN_0016e8c0(DAT_0037a484, id, 0x1c883a0): the object of objentry `id` at the zeroed place (origin, facing 0). The app makes the enemy kinds in `ObjTable::kinds` (M_EX850 for 0x6bf) a tick later (`enemy::spawn_created`) and returns the handle at once; other ids: Missing. The summon scripts' own 1.0 (Tinker Bell 1268, Chicken Little's P_EX360 2098) is in `summon.rs` (`SummonHost::own`) |
| 1.148 set_position (WP-EN3) | Impl / Approx | 0x170658 -> FUN_0016a618: the point (w 1.0) is the position. On the object itself Impl; on another object of the app it is queued and applied at the end of the tick (Approx) |
| 1.28 / 1.70 / 1.71 / 1.15 / 1.94 / 1.98 on another object (WP-EN3) | Approx | `Obj::others` (the other scripted objects' positions and motion slots as the tick starts) and `Obj::creates` (made this tick): position and motion slot are read, hide and flag changes are queued (`Remote`) |
| 4.29 (WP-7D) | Approx | FUN_0019ebd0 -> FUN_001d6738(i): 0 without the manager at DAT_01d5dd20 (created by FUN_001b9338 from the event command FUN_00180b48), else word +0x114 of its 0x38-byte entry i when bit 0 of the entry's +0xf4 is set. The Struggle opponents compare entries 0 and 1 (the score [inferred]). The app has no such manager: 0 |
| host table (WP-7D) | Impl | every enemy runtime gets the executable's host table (`HostTable::from_exe`, 0x35bce8) for the functions `kh2_bdx::names` does not carry; they reach the host as `Call::Unknown` (before, 4.29 stopped N_EX500_BTL's script with `UnknownHost`) |
| target record memory (WP-7D) | Approx | `read_mem` of the object's own target record (2.23): the player, index 0 [inferred], 0, -1, 0 (FUN_0018ed40 + FUN_0018f490 layout); the Dusk reads word 0 at 0x136d |
| party members: 1.247 random_free_point (WP-S5) | Approx | `friend_host` (the members' script host): FUN_001726d8 -> FUN_001a5ea8 as `party_rules::free_point` (test `party_free_point_matches_fun_001a5ea8`: the point, the generator 0x3716a0 and the counter 0x35e9b0 agree with the game's routine in kh2-ee); the floor cast and the line test are the area's polygons without the masks (FUN_001d1648 not ported). The enemy host does not answer it |
| party members: 2.14 / 2.29 / 2.39 / 2.40 on an attack object (WP-S5) | Impl / Approx | FUN_0018c890 and the flag words: hit a target (2), met a wall (8 / 0x10, FUN_0018c8e0 -> FUN_0018ca78 with `combat::wall_cast` for BG Hit rows after a move of more than 2 units), clash (0x200) never (the members' objects meet no other attack); 8 and 0x10 are not told apart |
| party members: 6.9 target_distance (WP-S5) | Approx | FUN_0018f308: the point's distance to the target record's shape; the member's target part taken as a sphere (centre distance minus radius) |
| party members: 1.21 spawn_effect, 0.41 effect_set_vec2 (WP-S5) | Approx | element `id` of the member's effect file unbound at the member (placed by 0.38 at once); 0.41's angles (FUN_0017be00) become the element's rotation |
| party members: 6.3 / 6.4 / 6.5 / 10.55 (WP-S6) | Impl / Approx | the runtime does the target search on the script's block (`kh2_bdx::search`: FUN_002f01a0 / FUN_002f0748 / FUN_002f04a8 / FUN_002f0758; test `search_matches_fun_002f04a8` against the game's routine in kh2-ee) and asks the host for the objects' target parts (`Host::target_parts`): the members' host lists the hittable enemies' TARGET shapes for kind 0 (none for kinds 1 / 2), and `target_found` points the member's target at the part found (docs/party.md section 17). A host without `target_parts` (the enemy host) gets 6.5 as before (`Call::Unknown`); 6.3 / 6.5 reach the host only without the executable's numbers |
| 6.54 (WP-S6) | Impl | the runtime: the vector copied to a static buffer (0x2ef110), the answer its address (a scratch vector) |
| party members: 2.19 attack_set_positions, 2.44 attack_word_84 (WP-S6) | Approx | 2.19 (FUN_001a30a8) sets the attack object's position and previous position (+4 bit 0x20 not kept); 2.44 answers the member (the owner +0x84). Neither is reached by P_EX020 (docs/party.md section 17) |
| party members: 1.88, 1.160 (not the blade's), 2.43, 2.57, 6.51 (WP-S6) | Stub | not reached by the members' scripts: P_EX020's blizzard library sets their request fields only to 0 and its bounce branch tests `-1 >= 0` (docs/party.md section 17) |
| party members: the hand object (WP-S5) | Approx | 1.234 names the hand-0 object; on it 1.161 / 1.102 / 1.18 (off and back on the hand), 1.83 (its drawn bone), 1.148, 1.89 (moves it in the frame it was set: FUN_00181e00 clears +0x880 after the move, WP-S6), 1.79 (a yaw only, FUN_0016a828), 1.11 (its slot), 1.94; 1.126 / 1.235 / 1.236 Stub (no linked motion controllers) |
| party members: 2.38 attack_callback_8c, 1.160, 2.67 (WP-S5) | Approx | the routine runs on every hit of the attack object (FUN_0018cf50 -> FUN_0016c550, `Runtime::call_routine`) with (argument, victim, attack); 1.160 points the member's target record at the victim; 2.67 takes the members' own group counter |
| summon objects: group 10 of Stitch's script, 1.84 / 1.272 / 1.165 / 2.74 (WP-SUM4) | Impl / Approx / Stub | `summon::SummonHost` over the party host: 10.12 / 10.16 / 10.39 / 10.41 / 10.45 / 1.84 (the player's status block) / 1.272 / 2.74 Impl, 10.1 / 10.19 / 10.32 Approx (WP-SUM4b: Stitch placed in his plane, FUN_001175a0; FUN_00118aa8 ported), 10.13 / 1.165 Stub (summon limits are not ported); WP-SM2: 10.3 / 10.4 by class (Genie, Peter Pan, Tinker Bell Impl: FUN_00114170 / a return; Stitch's limit placement and Chicken Little's FPS link Approx), 10.59 Impl, 10.51 Approx, 1.100 Impl, 1.128 Approx, 7.26 Approx, 10.29's effect on the player Approx; table and addresses in docs/summons.md "Status (WP-SUM4)" and "WP-SM2" |
| summon objects: Genie's 10.9 / 10.10 (WP-SUM5) | Impl / Approx | 10.10 Impl (FUN_00119ca8: the limit command of the form command at AI +0x14, the table at 0x3548f0); 10.9 Approx (FUN_00119c18: AI +0x14 and byte 7 as the game, the model parts 2..6 it hides and shows through object +0x670 are not drawn); docs/summons.md "Status (WP-SUM5)" |
| reaction class 5 from a party member's attack object (WP-S5) | Approx | `enemy::receive_hits` / `think` take the attack object's place from `friend_ai::MemberAttacks` as from `magic::SpellAttacks` (Goofy's `tornado`, atkp 1188): the victim is pulled until the object is finished or another reaction replaces the pull |
| group 5 hosts and 1.172 / 1.173: who calls them (WP-EN6) | n/a for the enemy host | `cargo run -p kh2 -- bdx <model>` on every enemy kind of `enemy::KINDS` (M_EX020, M_EX010, M_EX990, N_EX570_BTL, M_EX930, N_EX500_BTL, N_EX600_BTL, M_EX940, M_EX850): none of the nine programs calls a `5.x` host or 1.172 / 1.173. The disc-wide scan (`kh2 bdx --scan`) counts 1.172 in 228 scripts, the group 5 library in the `*_RTN` townsperson programs. All of them are answered by `NpcHost` in `npc.rs` (group 5: `group5`, 1.172 / 1.173: `in_front`), not by this table; there is nothing `Stub` or `Missing` in group 5 (every 5.x arm is `Impl` or `Approx`; 1.173 without a point is `Missing`) |
| 1.172 / 1.173 cone, exact rule (WP-EN6; ported by WP-NPC1 as `npc::in_cone`, both branches, test `cone_without_and_with_a_head_part`; Approx only for the part's point = the head bone of the previous tick [inferred]) | Approx in `npc.rs` | 0x1718d0 / 0x171908 -> FUN_001af878 / FUN_001af650(object, point), the point being FUN_0016bbe8(second object) (its part of type 0x11, else the lock-on point FUN_0016b6e0). FUN_001af650 looks for a part of type 0x14 on the first object (FUN_0016b558). Without one: direction = point - object position (+0x540), y zeroed, normalised, angle to the facing (+0x550) by FUN_0017bf10 (acos of the dot product; 0 for a dot >= 1.0 and pi for <= -1.0, 0x37a53c / 0x37a540 / 0x37a544), true when angle < 0.2618 (15 degrees, 0x37b950). With one: the direction starts at the part's own point (FUN_001894d0, the record's joint and offset) and is not flattened; true when angle <= the record's halfword +0x10 times 0.017453 (degrees to radians, 0x37b954). Data (a scan of every `obj/*.mdlx` type 23 table, 1218 files): 110 records of type 0x14, e.g. in N_EX510_RTN, N_EX520_RTN, N_EX560_RTN, N_EX570_RTN, N_EX580_RTN, N_EX590_RTN, both `_RAW_` variants too; each is joint 0x80 / 0x45 / 0x44 / 0x8b / 0x2f (the head bone of the model [inferred]), offset 0, radius word 0x32 = a cone of 50 degrees. The 15 degree branch alone was wrong for these townspeople; WP-NPC1 ported the part branch in `npc.rs` (the head record's joint pose is stored in `NpcFields::cone_part` before the script's tick) |
| 5.16 (FUN_002ea768 -> FUN_002d3148), exact rule (WP-EN6) | Approx in `npc.rs` | only when the path is not locked (+0x86 clear): a = position of one object minus position of another (FUN_0016b960: +0x540, or +0x70 while linked), normalised; skipped when its length is under 0.01 (0x3837e0); the path's nearest place to the first object (FUN_002d4dc8) gives the tangent (FUN_002d4590), normalised and skipped under 0.01; the direction +0x84 = (dot(a, -tangent) <= dot(a, tangent)). The app flattens a to the horizontal plane and takes the nearest place of the object itself; the wrapper's argument order (which of the two objects is the path owner) is not recoverable from the decompiled wrapper (its arguments are dropped) and was not disassembled |

## Event 11: reaction commands (WP-2C2)

`reaction_cmd::accept` writes `ReactionAccepted { cmd, pos, target }`; `ai_update` delivers it to the target's runtime as event 11 with (the player, the command id, a 16-byte target record {the enemy, the part index, 0, -1}) (`FUN_0016b420` -> `FUN_0016c620`). Soldier answers command 34 (`docs/limits-reactions.md`): the thread it starts (host 2.20, run by `Runtime::update_requests` each tick) makes calls on the **player**, which the host turns into `reaction_cmd::PlayerReactionCmd` messages: `set_flag` / `clear_flag` of bit 3 (`Unhittable`), `play_motion_with` (host 1.29: entry 252 and 253 of the *Soldier's* motion set, a BAR of per-player-model sub-motions named `%03d0` of Sora's objentry byte +7; `PairedMotion`, with the sub-motion's length so that `motion_done` of the player comes true when it would have ended), `set_velocity` (`Velocity`, 30 per tick), `queue_motion` (`QueueMotion`). The player's `+0x158` serial returned by `play_motion_with` is the counter of `FUN_0017ef98` (started at 0 here, inferred). **Nothing on the player side consumes the messages yet** (a hook for `sora.rs`); the log shows them with `KH2_BDXLOG=1`. `read_mem` answers the controller (the word at +4 is the player) and the target records.

## Events, death, leaving

- **Event 6 (hit).** After the reaction code has played its reaction, the next tick starts the handler; the running mode's damage hook
  runs first (the runtime does that). Shadow and Soldier go to `mode_revenge`. The mode manager then interrupts the low-priority modes while
  `busy` holds (`ModeInterrupted`), which is why the scripts wait for "not busy" before acting again.
- **Death.** FUN_001b25d0 (reached through FUN_001d5df8) switches the object to the mode named `dead` when it has one, and does the orb and
  munny bookkeeping. The app does the switch when HP is 0, before event 6, so the mode's "no switching while it runs" flag (priority 300,
  flags 3) refuses the revenge mode. What the game's trigger for it is (damage code or the reaction's end) was not found. The `dead` mode
  spawns element 2 (the death effect, `GameEffect::Death`), sets flags 3 and 6, fades out over half of the fade length and hides.
- **Leaving.** Far from Sora (the chase radius against the camera test) `mode_battle` switches to `leave`: motion 45, effect 1, flag 3,
  fade out, hide. Seen in the acceptance run after a Soldier was blown a long way off.
- **Removal.** `hide` ends an enemy: the app despawns an enemy the moment its script hides it. The game's rule for deleting a hidden object was not found (flag 0x10 at
  +0x7f8 is all `hide` sets), so this is an inference; without it leaving enemies would keep the next wave from spawning.

## Movement

- Root motion. The playing motion's root translation and yaw move the object, rotated by the facing the motion started with, exactly as for Sora
  (`root_delta`). This is what makes Shadow and Soldier walk: `move_target` only turns, probes and plays motion 2 (4.2 cm per tick of root motion).
  A grounded object also takes the root's height as an offset above the floor (the poses are stored relative to the root; Soldier's appear
  motion raises its root). A root below the floor is held at the floor (the game's movement step collides the whole displacement;
  the Dusk's sinking motion 181 otherwise put it under the floor, from where it fell for ever).
- `set_move` is used for one tick, then cleared (inferred from the scripts, which set it again every frame of a lunge); `start_move` keeps
  its horizontal speed while airborne.
- Jump (`Motor`, the model Sora uses: rise time sqrt(2h/g), eased height, parabola down, g = 20/49 per tick squared). FUN_0017b278 passes the direction's y to
  FUN_0017b1e0, which reads y below 0 as a rise of that many units (Y is down) and y of 0 or more as a fall that starts that far above the object.
  Both scripts pass positive values (Shadow's pounce 100, Soldier's jump 260) and then wait for `airborne`, so a fall that lands at once cannot be what they
  mean; the app jumps |y|. This is a deviation from the decoded function, kept because the other reading hangs both scripts.
- Falling and landing motions. FUN_001712d0 sets, with the commit flag, motions `m + 1` (falling) and `m + 2` (landing) at +0x100 and +0x104
  of the object; FUN_0017b1b0 resets them to none and 5; FUN_0017b9e0 plays the landing one (and queues motion 0 behind it), FUN_0017b8f8 tests
  the falling one. The app plays `m + 1` when the rise is over and the landing motion on landing; the apex as trigger is inferred.

## Verified and inferred

Verified by reading the game's code (addresses above): the probe algorithm, turn_toward, face_*, play_motion_checked's comparison, the motion-done flag
semantics, motion_length and motion_end, the can-be-hit test and flag 3, hide, set_obj_param's role, the drop behind ground_marker, the
mode manager hooks, the jump setup and its follow-up motions, flag_124's source. Inferred: script motion numbers are slots, loop rule from the return
frame, "busy" and "hit stun" as the reaction states, the one-tick lifetime of the move vector, the jump height from |y|, the death trigger,
the removal of hidden objects, Flag309 as "is the player".

## Gaps

- The object's own turn rate (+0x11c, set by the mover's initialisation from a parameter block) and the probe tolerance record are not decoded.
- The motion controller (blend queue, motion state variants for airborne objects) is not ported; requests restart the `Animator`.
- Spawn records (spawn kind, spawn parameters) come from the areas' event data, which the app does not read; defaults are used.
- Drops: orbs and munny are prize objects since WP-7B (docs/prizes.md); dropped items have no object yet.
- Hit stop is not implemented, so `ModeFrozen` is always false.
- Target records are the player's origin, not the lock-on part.
- Event 13 (event begin) is not delivered. Event 11 (a reaction command accepted by the player) **is** delivered since WP-2C2; event 0x13 (clash) and messages between objects since WP-6G, see below.

## Clash: mode hook slot 8 and event 0x13 (WP-6G)

The enemy clash handler at 0x1b2478 (read from the disassembly) first calls FUN_001b21a0 (the other volume's owner into
+0xae4, and into +0xaec when it is in the object list with +0x3b0 == 1); then, only when the object has a script thread
(+0x484): FUN_00187130(+0x300, own volume, other volume), which is FUN_00187040 with 7, i.e. the **running mode's hook in
slot 8** (the halfword at record +10 + 2 * 7) with (context, own volume, other volume); then FUN_0016c620 delivers
**event 0x13** with (own volume, other volume). Only after that come its own tests (volume flag 0x40, `RefactSelf` 2) and
the recoil. `kh2_bdx::Runtime::clash` does the script part (hook, then event; no script thread, nothing). `clash.rs` now
writes `enemy::EnemyClash` for every clash of an enemy volume (motion `None` when the handler's own part does nothing) and
`ai_update` delivers it the next tick with two volume objects (handles `0x0700_0000 + k * 0x40`) whose word +8 is the
owner (the enemy itself, and the player for the other one: the word 0x1b2478 reads); other fields are not served.
FUN_00187040 also skips the hook while the machine's +0x9c bit 4 is set (a mode that still waits for `ModeMayStart`).

Finding: none of the three scripts the app runs uses it. Shadow, Soldier and Dusk have no event 0x13 handler, and none
of their 26, 26 and 30 mode registrations has a slot 8 routine (the last value before each `host 1.6`), so a clash
changes nothing in their behaviour. Test `a_clash_runs_the_mode_hook_then_event_19` (kh2-bdx tests/runtime.rs) checks
the order and arguments on a hand-made script. Harness: `KH2_CLASH_FORCE_ENEMY=<s>` gives the first living enemy one
clash at that time (rows 0 and 0, no recoil); no natural enemy clash happened in the runs below (tt04 Dusks, tt00
Shadows / Soldier).

## Messages between objects: the Dusk's surprise (WP-6G)

Host 1.90 (0x172fb8) is FUN_00187170(delay, sender's mode machine, name): the first free entry of the 64-entry queue at
0x1c89738 gets (name, sender's context = machine +0x88, delay); a full queue drops the message. The per-frame task
FUN_00187250 hands each entry to every object of the list (FUN_001b7c88: the list from 0x1d5d7a0 through +0x8b4,
skipping objects where FUN_0019a440 is set): a delay of 0 or less runs the mode of that name's message hook at once
(FUN_001871d0: slot 6 with (own context, sender's context)), a positive one goes into the object's pending slot with
the delay times a fraction from the generator at 0x3716a0 (`seed * 0x10dcd`, times 2^-32 at 0x37a980). The runtime side
was already in `kh2-bdx` (`Runtime::receive_message`, `message_delay`); `enemy_ai.rs` now keeps the queue (`Mailbox`)
and at the start of each tick dispatches last tick's messages to every scripted enemy in query order. When the game's
task runs relative to the object updates was not found; one tick of lateness is [inferred].

The Dusk's event 11 handler, for command 24 or 69 only, runs a helper (alloc 0.47, target 2.23, free_target 1.131, then
a request thread at word 0x6e via 2.20) and sends `dusk_surprise` with delay 0. Every Dusk (the sender too) gets the
message hook of its mode `dusk_surprise`: if it is not busy (1.60) and the player is within 500 of it, it switches to
that mode, which faces the player, plays motion 234, waits for its end and switches to `mode_revenge`. The request
thread plays the Dusk's paired motion 241 or 242 on the player (by the sign of the wrapped angle between the Dusk's
facing and the direction to the player), queues motion 0 behind it and, while a counter that starts at 30 is positive and the
player's +0x158 is still that motion's serial, places the player relative to the Dusk's position (read while
`obj_valid` holds). Oracle test
`dusk_reaction_command_sends_dusk_surprise_like_the_game` (kh2-bdx tests/oracle.rs): commands 24 and 69 make the
calls 0.47, 2.23, 1.131, 2.20, 1.90 with the script's own string `dusk_surprise` and delay 0.0, step for step equal to
the game's interpreter; 34 and 0 make none. The request routine at 0x6e is not in the oracle (its address is a
parameter).

What offers 24 / 69 on a Dusk in the game was not traced here; the app's acceptance uses the harness offer
(`KH2_REACTION_FAKE=24 KH2_REACTION_FAKE_ON=Dusk`). `reaction_cmd` now reads every kind's collision table, the Dusk's
included, for the native offer.

## Drops (WP-6G)

Host 1.92 (0x171078 -> FUN_0016b738(object, id)): the point is the collision of a line from the object's position
(+0x540, raised by +0x5fc) to its lock-on point (FUN_0016b6e0, FUN_001d1648 with 6), or that point; then
FUN_001a3cb0(point, id). That function looks up row `id` of `przt` (`00battle.bin`, loaded by FUN_00193610 into
DAT_01c95514; 0x18-byte rows sorted by the halfword id, bisected by FUN_001a41d8, falling back to row 0x48), and, when
the prize manager DAT_0035e7a0 exists:

- for each of nine prize kinds `k`: `ceil(byte[+2 + k] * orb)` prize objects (FUN_001a4240, FUN_00300008 is a ceiling;
  FUN_001a3ef0 -> FUN_001a3b28 with the kind's 16-byte entry at 0x37b4b0 + 16k, three generator steps per object for its
  launch). `orb` = DAT_01c8a7e4 (1.0, from 0x37ad10 by FUN_00190370; settable by the host function at 0x173ae0) plus each
  party member's ability-block float +0x60 (Jackpot, FUN_00190788);
- unless 0x35d268 bit 0x400: for each of the three (item, chance) pairs at +0xc: one generator step, and the item drops
  (FUN_001bd490) when `seed * 2^-32 * 100 < chance * item`, `item` = 1.0 (0x37b494) plus each member's +0x64 (Lucky Lucky);
  the seed is read unsigned (the disassembly's sign fix-up at 0x1a3ee0);
- then FUN_001c5b10, FUN_001c8590 and FUN_001d54d8 with the point and the row (not read).

Bit 0x200 of 0x35d268 skips the prizes and items; the player state FUN_001ce6c0 (+0x588 bit 0x2000000) remaps the kinds
(table 0x37b4a0) and skips the items. Both are taken as clear. `enemy_host::compute_drop` does this with Sora's Jackpot and
Lucky Lucky from `abilities::Abilities` and logs `drop (host 1.92) asked row .., used row ..: prizes per kind [..],
items [..] (n prize objects, m item objects)`; tests `a_drop_follows_fun_001a3cb0`,
`prize_rows_parse_and_fall_back_to_row_0x48`, `real_przt`.

WP-7B: a prize takes its three generator steps only when the pool (0x100 entries) has a free entry, and a
dropping item takes one more step (FUN_001bd490 -> FUN_001bd0d0); `compute_drop` now returns each prize's kind
and launch values, and `ai_update` hands them to `prize.rs` (message `PrizeDrop`), which runs the prize objects:
flight, rest, pickup and the pickup table (docs/prizes.md). The three calls at the end serve the mini-game prize
pools (PRIZE_MU, PRIZE_HE, PRIZE_COLOSSEUM), absent in Twilight Town (docs/prizes.md). Gaps: the flags and the
generator's real state.

WP-7Bb: a dropping item takes four steps, not one: FUN_001bd0d0 takes one for the object's yaw and then enters state
12 (`FUN_0016b698(obj, 0xc)` -> FUN_0016b648 -> the state object 0x1c88f70 with vtable 0x35f2b8, set by FUN_00179880),
whose enter FUN_001bd580 takes three for the launch, all before the next item's roll. `compute_drop` returns them
(`item_launches`) and `ai_update` sends `prize::ItemDrop`; the item objects are in docs/prizes.md.

## Acceptance run

`KH2_ISO=... KH2_BDXLOG=1 KH2_KEYS="4.5:F3,<lock on and attack every 0.45 s from 6 s>" KH2_SCREENSHOT=out.png KH2_SHOT_AT=40 kh2-game tt00`
(Sora stands where he starts in tt00, locks on with Q and attacks with J; the first wave spawns by itself at 4 s). In a run of about 40 s a first wave of four Shadows and
then a Soldier fought Sora; modes entered: Shadow `appear_shadow`, `mode_battle`, `battle_idle`, `move_target`, `btl_attack`, `btl_short`, `btl_long`,
`atk_craw_l`, `atk_craw_r`, `atk_craw_f`, `mode_revenge`, `rvg_short`, `dead`; Soldier `appear`, `mode_battle`, `btl_attack`, `btl_short`, `btl_long`, `idle`,
`idle_time`, `move_target`, `move_roll`, `move_step`, `atk_craw`, `atk_jump`, `atk_kick`, `mode_revenge`, `rvg_short`, `rvg_long`, `leave`. 45 distinct host functions were called, none with
a script error; the only one that failed visibly was 1.92 (`GroundMarker`, 5 times, once per dying Shadow). Nobody heard anything: no sound is
involved. The run's host call table is the last block `KH2_BDXLOG` prints (counts above).
