# Enemy AI runtime: Shadow and Soldier under their BDX scripts

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
function is always logged, once per function, with `group.index`: `unimplemented host call 1.92 (GroundMarker) ...`.

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
| 1.121 set_target_mode, 1.131 free_target | Stub | the lock mode of a target record has no consumer |
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
| 1.65 set_flag_26, 1.104 / 1.105 flag 768 | Stub | no consumer in the app |
| 1.19 fade_out, 1.20 fade_in, 1.28 hide | Impl | alpha tween 1 to 0 and 0 to 1 over the given frames (blended materials while it runs); hide sets alpha 0 and the invisible flag (FUN_00170f78) |
| 1.87 spawn_attached | Approx | element `id` of the object's effect file, bound to it; id 2 goes through `GameEffect::Death` (the death effect, `fx::EFFECT_DEATH`); the flag and extra arguments are ignored |
| 0.79 release_effect | Stub | effects run to their end |
| 2.9 set_obj_param | Impl | the game builds the object's status block from the `enmp` row (FUN_001b2148 -> FUN_0019f8a8); the app built it at spawn, the call checks the row number (Shadow 34, Soldier 33) |
| 1.110 spawn_kind | Impl | -1: no spawn record |
| 1.39 spawn_param | Stub | 99: no spawn record (the area's event data is not read); 99 is the value Shadow tests for the plain behaviour |
| 1.23 global_byte_33e830 | Approx | 2, the world number of Twilight Town |
| 1.171, 1.206, 1.34, 1.301 | Stub | global flags and a global byte set by other scripts or the game: false and 0 |
| 2.8 hit_is_real | Impl | the effective reaction's class is 2 or more (FUN_0018b700 on the hit record's byte +0x26) |
| mode manager hooks | Approx | `ModeInterrupted` and `ModeMayStart` follow FUN_00186b70 and FUN_001873d0 read as "a hit reaction plays" and "not in hit stun"; `ModeFrozen` (FUN_00186b38, object flag 0x40000) is false because hit stop is not implemented |
| 1.92 ground_marker | Missing | not a marker: FUN_0016b738 finds the ground under the object and FUN_001a3cb0 spawns row `id` of the drop table (orbs, munny). The app has no pickups |
| 1.62, 1.64, 1.89, 1.133, 1.134, 1.128, 2.20 | Missing | in the scripts but not reached in a fight: object vectors at +0x860 and +0x850, the secondary velocity, two virtual methods of the object, a virtual call on the mover, a sound or effect request (Soldier's event 11 only). They would be logged when first called |

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
  motion raises its root).
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
- Drops (orbs, munny) do not exist; the death's `ground_marker` call is the visible gap.
- Hit stop is not implemented, so `ModeFrozen` is always false.
- Target records are the player's origin, not the lock-on part.
- Events 11 and 13 (Soldier's lock-on query, event begin) are not delivered; messages between objects are not wired (neither script sends any).

## Acceptance run

`KH2_ISO=... KH2_BDXLOG=1 KH2_KEYS="4.5:F3,<lock on and attack every 0.45 s from 6 s>" KH2_SCREENSHOT=out.png KH2_SHOT_AT=40 kh2-game tt00`
(Sora stands where he starts in tt00, locks on with Q and attacks with J; the first wave spawns by itself at 4 s). In a run of about 40 s a first wave of four Shadows and
then a Soldier fought Sora; modes entered: Shadow `appear_shadow`, `mode_battle`, `battle_idle`, `move_target`, `btl_attack`, `btl_short`, `btl_long`,
`atk_craw_l`, `atk_craw_r`, `atk_craw_f`, `mode_revenge`, `rvg_short`, `dead`; Soldier `appear`, `mode_battle`, `btl_attack`, `btl_short`, `btl_long`, `idle`,
`idle_time`, `move_target`, `move_roll`, `move_step`, `atk_craw`, `atk_jump`, `atk_kick`, `mode_revenge`, `rvg_short`, `rvg_long`, `leave`. 45 distinct host functions were called, none with
a script error; the only one that failed visibly was 1.92 (`GroundMarker`, 5 times, once per dying Shadow). Nobody heard anything: no sound is
involved. The run's host call table is the last block `KH2_BDXLOG` prints (counts above).
