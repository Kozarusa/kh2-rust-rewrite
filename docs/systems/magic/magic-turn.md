# Magic: the turn, phase 2, gaps and index

[Contract](contract.md) | [Index](../../README.md)

## The magic turn (WP-7A)

- FUN_00179470 (Magic Lock-On, for Fire, Blizzard, Thunder and Magnet): when the command's target record (+0x9f4,
  copied from the request by FUN_00174cf8) is not valid (FUN_0018ed58), FUN_0018fb98 picks one into it with a range of
  0.0. Its first test `range <= 0.0` then lets every candidate through (no distance or height window): every target
  shape (type 3, records without bit 0 of +2) of every targetable object, scored `(d + |dy| * plyr+0x6c) * ((1 -
  facing . dir) * 0.5 * (1 - plyr+0x70) + plyr+0x70)`, the lowest wins. When one is found the lock-on controller
  (+0xb40) goes to mode 1 on it (FUN_0019b3f0: the target marker, FUN_0013e528 with 5 or 1) and FUN_0019bcb8 sets its
  timer from the player's +0x130 row +0x68. [V] Port: `magic_rules::lock_on_pick`, `magic_lock_on_applies`; the mode-1
  target is kept for the cast motion. Timer (WP-MG2): FUN_0019bcb8 sets +0x1c to the row's +0x68 (`plyr`
  AutoLockOnTime) in mode 1 only (oracle test `lock_on_timer_and_global_flag_8_match_the_executable`); each frame of mode
  1 (FUN_0019b7a8) the timer loses `DAT_00356f40` (1.0 in the executable) while it is above 0 and the player is up
  (FUN_0019a4f0, see below), else the controller looks for a new record (FUN_0018f538 case 6, see below); the
  trigger 28 refills the timer before it turns (FUN_001861b8). Port: `magic_rules::mode1_step`, `mode1_frame`,
  `mode1_timer_start`, `CastTurn::lock_timer`. [I: the controller's frame runs before the trigger in a frame.]

  Controller frame in mode 1 (WP-MG3, FUN_0019b7a8 read in full). The record the input record `*(player + 0xb38)` is NOT the
  ability block: it is the player's input record (words at +0 held, +8 newly pressed, as `FUN_001babf0` reads
  `*(actor + 0x970) + 8`). Its bit 0x800 at +8 is logical bit 0x800, which the field button set 0x379768 builds from
  physical R1 (pair 0x800 -> 0x800; FUN_001676a0 / FUN_00167660 with the shared table 0x379698 and the set at
  0x35b8ac), the lock button. Order of a mode-1 frame: (1) lock button newly pressed: FUN_0019b3f0 with mode 2 on the
  same record (the marker changes to 0x1b / 0x17 in FUN_0019bbc0, FUN_00197b70(0x9e, ..) was not traced,
  the timer is not used again); (2) timer above 0 and FUN_0019a4f0 true: the timer loses `DAT_00356f40`; (3) else
  FUN_0018f538(result, 6, player) and, when its record is valid (FUN_0018ed58), FUN_0019b5e8 sets it; (4) no valid
  record: FUN_0019b580 releases the controller (mode 0, the record cleared, the marker released). Mode 0 with the
  button pressed goes to mode 2 through FUN_0018f538(4) (FUN_0019be08 false), else to mode 1 through
  FUN_0018f538(6); mode 2 keeps its record while FUN_0019a5d0 holds and the button is not pressed again, else
  FUN_0019b580. Port: `magic_rules::mode1_frame` (test `mode1_frame_order_and_player_up`); in the cast the
  button is `sora::Pad::lock_newly_pressed` (WP-FL1: the press `sora::tick` takes, kept until the next tick), which
  replaces the earlier inference from Sora's lock-on target; mode 2 keeps the cast's record while it is alive and a
  second press releases the controller (`magic_rules::mode2_keeps`; the system sound FUN_002c14a8(8) is not played).
  FUN_0019a4f0 (all four conditions, `magic_rules::player_up`): `+0x7f8 & 4` clear (the dead flag), `+0x108 & 8`
  clear (the cannot-be-hit bit, enemy host 1.70), `+0x510` (the status record) not null, and its first word (hp)
  above 0. Oracle: `player_up_and_mode1_keep_match_the_executable` runs FUN_0019a4f0 for all 32 combinations and
  the keep branch of FUN_0019b7a8. The app feeds it Sora's `hp`, `Fighter::hittable` (bit 3) and a status that is
  always present.
  The re-pick, FUN_0018f538 case 6 (the jump table at 0x37ace0, entry 6 = 0x18f60c; read from the executable, the
  decompiler lost it): with the player's row (`+0x130`) null it only stores the kind; otherwise FUN_0018fb98 with the
  range `plyr` +0x5c (AutoLockOnRange), unlike the first pick (FUN_00179470: range 0.0, no window). With a range
  above 0 FUN_0018fb98 admits a target shape only when `MinH - r_v <= dy <= MaxH + r_v` (`plyr` +0x60 / +0x64,
  `dy` target minus player height in the game's Y-down sign, both heights times FUN_00190838 when that is above
  1.0) and `distance - r_h <= range`; `r_h` / `r_v` are the shape's halfwords +0x10 / +0x12 (FUN_001894d0, zero
  unless the shape has flag 0x4000 or the object's `+0x7f8` bit 0x400 is set). The score is the one of the first
  pick. The candidates also need FUN_0019a5d0 and FUN_0019a4f0. Port: `magic_rules::LockWindow`,
  `lock_on_pick(.., Some(window), ..)` (test `lock_window_follows_fun_0018fb98`, the same window `sora::auto_target`
  applies with the Y flipped); in the cast the pick runs every frame once the timer is out (FUN_0019b5e8 does not
  refill it) and a missing pick drops the mode-1 target. [GAP: the shape radii are not modelled (zero), so the
  window is slightly tighter than the game's; FUN_00190838's writer FUN_00190808 (called only from FUN_002c5af0)
  was not traced, the scale is taken as not above 1.0; FUN_0018fb98 and FUN_0019b7a8's other branches were not run
  in the oracle (they walk the object list and the marker pool).]
  The marker, FUN_0019bbc0: FUN_0013e528(5) when the record's shape byte (`shape + 1`, from FUN_001894d0 via
  FUN_0019b388 into `+0x24`) is 0, else FUN_0013e528(1), for mode 1 (mode 2: 0x1b / 0x17); FUN_0013e528 takes a slot
  of the nine-slot field icon pool (`docs/systems/ui/contract.md`, 0x13e5a0) and FUN_0013e1f0 makes the icon group of that number
  and a second member of group 20 (the speech bubble). FUN_0019bbc0 then places it on the record's target point
  (FUN_0018efa0) and hands it a flag that is 1 for a player object (`+0x588` bit 2) with `+0xac8` or `+0xacc` above 0 (FUN_0013e438; the flag's use was not traced).
  Marker feed (WP-MG4): `run` inserts the HUD's `MagicMarker { mode, shape, at, alert }` each frame while `CastTurn::lock_mode1` holds a living
  record (mode 1, or 2 once `lock_mode2` is set) and removes it when the controller is released (FUN_0019b580: second press in
  mode 2, no re-pick in mode 1) or the cast state ends (the port drops the controller with the cast, see above). The shape byte is
  byte +1 of the picked target shape: FUN_001894d0 puts the shape pointer into word 3 of the 0x18eec0 result, FUN_0019b388 stores
  `*(shape + 1)` into controller +0x24, FUN_0019bbc0 reads it as word 9; in the model's collision table that byte is
  `Shape::parts` (record +1), taken from the enemy's first TARGET shape (the one `victim_targets` uses for the point). `at` is that
  point. Log lines `magic: marker mode M shape byte S group G` and `magic: marker released`. Test
  `marker_groups_follow_mode_and_shape_byte`. [GAP: the bubble flag (target +0x588 bit 1 with +0xac8 / +0xacc above 0) is a
  player-object rule; enemy objects here carry neither field, so `alert` stays false. What a non-zero `parts` byte means for
  gameplay (which enemies get the pink ring) was not traced beyond this writer chain; the port feeds the table value as it is.]
- The turn itself is two range triggers of the cast motions (FUN_00188030, player objects only, +0x588 & 2): 22 turns
  toward the command's target record (FUN_00186118), 28 toward the lock-on controller's target (FUN_001861b8, which
  also refreshes the mode-1 timer). The parameter is the limit in degrees per frame (times `DAT_0037a988`, pi/180).
  FUN_00186018: nothing when the record is invalid or names the caster; else, when the horizontal distance from the
  caster's position (+0x70 plus +0x560 [I: no horizontal part]) to the target point (FUN_0018efa0: the target's target
  shape centre, else its position) is above `DAT_0037a970` (50.0), FUN_00183848 turns the facing toward it by at most
  the limit (times the frame step), snapping within it. [V] On the disc the level-1 cast motions carry: Thunder and
  Cure on the ground trigger 22 (frames 0..16, 15 degrees), Blizzard (all three), Thunder's finisher and air motions,
  Cure's air motion and Magnet trigger 28 (15 degrees); Fire and Reflect none (test `magic_turn_data_from_the_disc`).
  Port: `magic_rules::cast_turn` (test `cast_turn_rule`); the app's command target record is Sora's target (lock-on or
  auto target), Cure's names Sora [I], the controller's target is Sora's lock-on, else the mode-1 target. The turn is
  applied after `sora::tick`, so the body shows it one tick later [app].

## Running the scripts in the interpreter (WP-7A)

`kh2_bdx::Runtime::start_thread` / `restart_thread` give the spell thread of FUN_001a4798 (docs/systems/script-host/bdx-format.md "A thread
whose code is an event", tested against the game's interpreter). With `KH2_MAGIC_VM=1` a cast runs its slot's own
script (the first BDX of the `.mag`) instead of the hand port: event 20 at the cast with (caster, resource record,
kind, `magi` row), event 21 from the cast trigger, one slice per tick. `magic::SpellHost` answers the host calls of
Fire's script: 1.139 (an element of the spell's effect file on the caster; the mode word is not told apart [I]), 0.44,
0.50, 0.53 (the element's position as the hand port takes it), 1.15 (1 while the cast motion plays), 1.57, 2.10,
2.11, 2.12, 2.13, 2.30, 2.31, the caster's first 16 bytes (word 1 the object) and the `magi` row's floats. With
`KH2_MAGIC_LOG=2` both paths log every live attack per tick relative to the cast (`magic: timeline +<tick>`).

Compared in harness runs (tt00, the `magic-fire` keys with 0, 1, 2 or 3 `G` presses):
- Fire: the same timeline in both paths (trigger at +22, attack 502 radius 200 height 80 from +23 to +66, the same
  hits), and test `fire_script_in_the_interpreter_matches_the_hand_port` checks the requests against the disc's
  script. The comparison found the inverted event-21 branch above (fixed in the hand port).
- Blizzard: the script stops at host 1.201 (`FacingVector`), not answered; then a read through a null pointer.
- Thunder: stops at 2.23 (`Target`), 1.140, 1.147 (`Position`).
- Cure: stops at 2.23, 1.140, 1.343, 1.83.

So the hand port stays the default; the interpreter path is a check for Fire. [GAP: the hosts above, then 2.20
request threads (already run by the runtime), 1.120, 1.138, 1.141, 1.146, 2.14, 2.17, 2.29, 2.39, 0.38, 0.41,
0.46, 0.52, 1.93, 10.31, 0.100 for the other three.]

## Phase 2 (WP-7I)

### Levels 2 and 3

Every level's `magc` row names its own `.mag` (`FIRE_2.mag` ...), its own command (Fire 49 / 119 / 120, Blizzard 51 /
121 / 122, Thunder 50 / 123 / 124, Cure 52 / 125 / 126, Magnet 174 / 175 / 176, Reflect 177 / 178 / 179 on the
disc) and its effect file, but the BDX program of every level is the same (byte for byte): the level is carried by
the kind (`2 * level - 2`), which picks the `atkp` rows (`SubId` high byte) and the `magi` row. So the hand port
covers levels 2 and 3 with no new code paths. What was missing is the kind the scripts keep after event 20
(`magic_rules::script_kind`): Fire, Blizzard and Thunder clear the finisher bit in Master form (`kind / 2 * 2` when
kind / 10 = 2), Reflect always; Cure and Magnet keep it. [V, script] Test `levels_two_and_three_have_their_rows`
(rows, files, identical scripts, commands, and the attack and `magi` rows of every kind, forms included).
Harness: `KH2_MAGIC=3,3,3,3` casts Firaga (atkp 502 `SubId` 1024, radius 240, height 180), Blizzaga, Thundaga and
Curaga (atkp 708 `SubId` 1024, Power 600) with their rows.

The `magc` slot order is read from the data (`Id`): 0 Fire, 1 Blizzard, 2 Thunder, 3 Cure, 4 Magnet, 5 Reflect.

### Magnet (MAGNET_*.mag) [V, script]

Runs in the interpreter (slots 4 and 5 always do, WP-7I). Event 20 starts element 0 on the caster. Event 21: stop
it, element 1; Final sets bit 3 of the global flag word 0x35d268 (host 1.187) over a yield (WP-MG2: the only reader is FUN_001905e8, `magic_rules::global_flag_8_active`, called by FUN_001a3848, the attack-versus-object hit dispatch that sends the attack to the player's hit routine FUN_001a3a88, and FUN_001bd358, the enemy target pick; the other writer is FUN_001b9140 case 3; a scan of `--jal 0x1905e8` lists these two calls; the app has neither path, so the bit has no effect here). WP-MG3 read both: FUN_001a3848(attack, object) sends the attack to FUN_001a3a88(attack, player, object) (FUN_001c5808 then FUN_001a39b0, the hit on the player) at once when the bit is set and the player exists, before its own loop over the three party objects (FUN_00175b68) and the hit test FUN_001c57b8 ; FUN_001bd358(enemy) skips its distance and target tests (FUN_001a3700, FUN_001bdb10) and calls FUN_001bd1d8(enemy, player), which makes the player the enemy's target (`+0x9e8`, flag 4 of `+0x9e0`, mover 0xd), when the enemy's own gates pass (player listed, his `+0x588` bit 0x2000000 clear, the enemy's `+0x9e0` bit 4 clear, FUN_0019a4b0). So with Final Magnet active every attack lands on the player and every enemy picks him. The next bit of the same word, 0x800 (FUN_00190610), also sets flag 8 of the attack's `+0x28` in FUN_001a3848. [GAP: not ported; the dispatch lives in `combat.rs` / `enemy_ai.rs` (the app's hit test and enemy target pick), outside this WP's ownership. Exact change: in the attack-versus-fighter dispatch, when `global_flag_8_active` and the player exists, deliver the attack to the player's fighter without the hit test; in the enemy target pick, choose the player without the distance and target tests.]) The
place: the target record's position (2.23 / 1.140 / 1.120) or element 1's position; host 1.141 probes the ground
from `magi` +0x3c above it down `2 * magi +0x3c` and, when found, puts the place `magi` +0x3c above the ground.
Attack 658 of the kind at the place, a sphere of `magi` +0x4c; element 2 placed there (1.138 + 0.38). 16 frames
later, in Wisdom, Master and Final, a second attack 976 (radius `magi` +0x3c * 0.5). Element 3 at the place one
unit up grows over `magi` +0x40 frames (host 0.39 with `(sin((t - 0.5) pi) + 1) / 2`), then `magi` +0x44 frames
pass and the attacks are finished; Final adds attack 1440 (radius `magi` +0x3c * 0.6) for one frame; element 4
starts; element 3 shrinks over `magi` +0x48 frames and is destroyed (0.51). Test
`magnet_script_runs_in_the_interpreter`.

The pull is the victim's: atkp 658 has `EnemyReaction` 5, class 5 of FUN_00184e80 -> FUN_00185668 ->
FUN_001bf920: the victim keeps the attack object (+0xaac), a hold distance random between `KnockbackStrength1`
and `KnockbackStrength2` (+0xab0, the game's generator at 0x3716a0), the row's `Unknown` field (+0xab4, -3 on
Magnet's rows) and plays motion 0x14 (facing the attack) or 0x15; each frame FUN_001bfc10 moves it toward the
attack object's position by `clamp((distance - hold) * DAT_0037bd30, DAT_0037bd2c, DAT_0037bd34)` plus the
flat direction turned by FUN_0017c2e8(DAT_0037bd38) times +0xab4 (a swirl), times the frame step; FUN_001bfba8
ends it when +0xaac is cleared. [V for the functions]

Ported in WP-7M2 (`reaction_rules::Pull`, `PullConsts`; `enemy::receive_hits` and `think`; test
`pull_follows_fun_001bfc10`, disc test `pull_numbers_from_the_disc`). The details read from the code:
- FUN_001bf920: the hold distance is `KnockbackStrength1 + r * 2^-32 * (KnockbackStrength2 - KnockbackStrength1)`
  with `r` the generator's state after its step (`DAT_0037bd18` = 2^-32); the motion is 0x14 when FUN_00184dd0 holds
  (the victim's facing against the hit direction is negative: hit from the front), else 0x15; the request is
  FUN_0017e220(blend 0, ..., motion). FUN_001bfdf8 then turns the victim toward the attack object (its method +8)
  from its body centre (FUN_0016b7d0: the first collision shape of type 1, else +0x540), times `DAT_0037bd3c` (-1)
  for motion 0x15 (it keeps its back to it). The mover is switched to 14, or 15 when the mover's flag 2 or +0x160
  bit 0x40 is set (FUN_0016b698); its object is constructed at run time (table 0x35ca48 points into the BSS), and
  the methods it calls are those of the vtable at 0x35f348: +0x18 = FUN_001bfba8 (each frame: re-face with
  FUN_001bfdf8, or, once +0xaac is 0, the default mover and motion 0 with the blend `DAT_0037bd28` = 8), +0x1c =
  FUN_003332f0 -> FUN_001bfc10 (the velocity). [I: mover 15 was not told apart from 14.]
- FUN_001bfc10: `d` = attack position - body centre (all three axes); speed `(|d| - hold) * DAT_0037bd30` (0.1),
  below `DAT_0037bd2c` (-20) that, else above `DAT_0037bd34` (20) that; the pull `unit(d) * speed`; the swirl is the
  pull turned about Y by `DAT_0037bd38` (pi / 2, FUN_0017c2e8), its height zeroed, made unit, times +0xab4 (the
  row's `Unknown`: -3, -6, -12 by level); the velocity is their sum times `DAT_00356f40` (1.0), per frame. It has a
  height part and no gravity [I: the blow mover's velocity method FUN_0019e4d8, at the same vtable slot, adds its
  own gravity; this one adds none].
- The end: FUN_0018ccf8 (an attack object being finished: from FUN_0018c0d8, FUN_0018c248, FUN_0018e408) clears
  the hit-memory bit of the attack in every actor and +0xaac of every actor that holds it.
  FUN_00186600 (mover +4 of the blow mover 0x35e4a8 and of 0x35e728) clears the hit-memory bit of the held attack
  when those movers are left; the pull mover's +4 (0x331388) is not decompiled [GAP].
- The numbers are read from the user's executable (`PullConsts::from_exe`). The generator is the app's own
  instance of `x * 0x10dcd` started at 1 (`enemy::PullRng`; the game shares one state, so the sequence differs).

In the app: `magic::SpellAttacks` publishes each tick the live spell attacks' positions by group, this tick's
spell hits (victim, atkp, group) and the enemies' body-centre offsets; `victim_outcome` still answers class 5 as
`NotDecoded { class: 5 }` (the variant stays so that `sora.rs` keeps compiling; Sora is never the victim of an atkp
658), and `enemy::receive_hits` turns it into the pull when the hit came from a spell attack. The enemy is in
`EState::Hurt` with the pull until the attack group is gone; then motion 0 with blend 8. Any other reaction ends
the pull. `KH2_REACTIONLOG=1` logs the pull every 10 ticks.

Harness (tt00, `KH2_MAGIC=1,1,1,1,1,1 KH2_MAGIC_LOG=1 KH2_REACTIONLOG=1 KH2_YAW=0`, keys
`2:F3,4:Q,5:W:1.2,6.8:G,7.1:J,7.25:G,7.4:G,7.55:G,7.7:G,7.9:J`): `enemy: Shadow pulled (reaction class 5,
FUN_001bf920): attack 1, hold 40.0 (kb 40 .. 120), swirl -3, motion 0x14` for the four Shadows, then lines such as
`Shadow pull: 52 units from attack 1 (hold 40), velocity Vec3(2.6, -0.1, -2.0) units/tick` (the Shadows circle the
sphere near their hold distances), and `Shadow pull ends: attack 1 finished (FUN_001bfba8), motion 0 blend 8`
about 2.8 s later. Screenshot at 9.0 s: the blue sphere with the Shadows gathered at its foot.

### Reflect (REFLECT_*.mag) [V, script]

Event 20 asks for the persistent block of key 0x10002 (host 2.21, FUN_001a5040: 12 bytes kept across casts),
clears the finisher bit and starts no charge effect. Event 21: the barrier is element 2 when host 1.60 holds
(FUN_001729f8: airborne by FUN_0019aa28, or class flag 2, or +0x160 & 0x40), 12 in Wisdom and Final, else 0; a
barrier still in the block is destroyed first, the new one is kept there (+4). After a yield the guard: atkp 573
(Type 3, Power 0), a sphere of `magi` +0x50, moved to the barrier's position every frame while the barrier lives
(0.49); each rise of its byte +0xa4 (2.66) starts the flash element (barrier + 1) unless one still runs. When the
barrier ends: the clashes are added to the block's +8, the guard is finished, and if the barrier is still the
block's, the block is reset and, with clashes, the counter-burst: 1, 2 or 3 by the count against `magi` +0x64
and +0x70, element 4 / 5 / 6 (+10 in Wisdom, Master and Final) on the caster, then attack 574 / 576 / 578
(radius and height `magi` +0x5c / +0x60, +0x68 / +0x6c, +0x74 / +0x78) at the element for `magi` +0x54 frames, then
667 / 668 / 669 20 larger for `magi` +0x58 frames. Test `reflect_script_runs_in_the_interpreter`.

The guard: byte +0xa4 is counted by FUN_0018d430, the handler of an attack-against-attack pair of FUN_0018d808
(docs: `clash.rs`). The app pairs the guard (`magic::run`) with enemy attack volumes the executable's clash table
allows (`ExeConsts::types_clash` with Type 3): each enemy window counts once, the enemy gets its clash reaction
(`reaction_rules::enemy_clash`, `EnemyClash`) and its volume is disabled (its window marked landed on the player's
fighters, as clash.rs does) unless its own row says Nothing, so it cannot hit Sora. The guard hits no object
[I: Power 0, reaction 0]. The barrier's duration is the barrier element's own life in the effect runtime (27
frames in a tt00 run at level 1).

A live clash (WP-7M2, no forcing): tt00, `KH2_MAGIC=1,1,1,1,1,1 KH2_MAGIC_LOG=1`, Sora standing still after F3 so
that a Shadow attacks him (the first hit lands about 8.8 s into the run), Reflect cast just before it with the keys
`2:F3,7.60:G,7.75:J,7.90:G,8.02:G,8.14:G,8.26:G,8.38:G,8.60:J` (casts at 8.45 and 8.75 clash too). The log:
`magic: guard 1 clashes with 1076v0 attack 10 (clashes 1)`, `enemy: Shadow clash: recoil motion 140`, `magic(vm):
effect 1: element 1 (on the caster)` (the flash), `magic(vm): attack 1 over (clashes 1)`, `magic(vm): effect 2:
element 4 (on the caster)`, `attack Some(2) made: atkp 574`, three rounds of `hit by attack 2` on the four Shadows
(SubId 0, 1, 1; Interval 10), then `attack Some(3) made: atkp 667` (Power 550) hitting them; no `sora hit` in that
window. Screenshot at 9.2 s: white burst bubbles around Sora with the Shadows close around him.

### Drive forms

- Fire: `fire_plan` (event 21 elements per kind; test `fire_hand_port_matches_the_script_for_every_kind` against
  the script for every kind of levels 1 to 3, forms included).
- Blizzard (`blizzard_plan`, R0017): elements 1; Final non-finishers 1 and 7; Final finishers by the program's
  global +0x2c (casts counted at L022b, kept in `MagicState` [I: the globals live as long as the slot's resource]):
  7 for 1 and 3, 1 and 7 for 4, else 1. Shots: the Wisdom finisher five (turn scale 0.5 ahead, 1.0 at +-35 degrees,
  2.0 at +-70 degrees, flag 0); Master seven (scale 1, flag 0: ahead, with the height -0.96 and 0.96, then 0.4 and
  -0.4 each turned +-45 degrees); otherwise one ahead with flag 1 and one more from element 7 when it started. The
  turn rate of a shot is `magi` +0xc times its scale. Host 0.13 (FUN_0016cf08 -> FUN_0017c2e8 with FUN_0011b4d8 ->
  FUN_002fb628) is taken as libvu0's Y rotation [I]. Test `blizzard_forms_follow_the_script`. The block's flag
  (+0x64) with host 2.14 and the push 1.146 are ported since WP-7M2 (Blizzard section above).
- Thunder (`thunder_plan`): Master strikes at the caster's position and 0.47 and 0.78 of the way to the place, 16
  frames apart, then the place (without a target 800 ahead turned 45 degrees); the Wisdom finisher three more
  strikes 8 frames apart around the place (`magi` +0x24 * 1.5 along the facing turned 120, 240, 360 degrees).
  Final (R026a, L014c) strikes 2 (7 for a finisher, R0276) more times, each after a wait of 8 frames, at host
  1.248's place around the main place with probe height 330 (WP-7M2, below). Test `thunder_forms_strike_counts`.
- Host 1.248 (FUN_00172730: FUN_001a5ee0(out, centre, mask 2, f12..f15 = the four floats), read from the machine
  code since the decompiler dropped the float arguments). Thunder passes R = 2.5 * `magi` +0x24 as (0.4 R, R,
  -0.1 R, 0.1 R). FUN_001a6068: radius `r_min + rand * 2^-32 * (r_max - r_min)`; start angle `bucket * pi/4 + rand
  * 2^-32 * pi/4` with `bucket = (t & 4) >> 2 | t << 1` of the counter `t` at 0x35e9b0, which then steps modulo 8
  (the buckets 0, 2, 4, 6, 9, 11, 13, 15: the eighths in a spread order). FUN_001a63c8: try k = 1, 2, ...: angle
  start + k pi/4 (wrapped, FUN_0017bbc0), the candidate is the centre plus (sin, 0, cos) (FUN_00300278 sinf,
  FUN_003000d8 cosf, the game's axes) times the radius, raised by `h_min + rand * 2^-32 * (h_max - h_min)`; it is
  taken when FUN_001a5bf8 (FUN_001d1648, a line test, negated) finds the line from the centre to the point 20
  (0x37b5cc) further out and 100 (0x37b5d0) above the candidate clear; after 8 tries the radius shrinks by 100 and
  k starts again; once the radius is below `r_min` the centre is returned. Port: `magic_rules::scatter_place`,
  `ScatterConsts` (read from the executable), `ScatterState` (the app's generator instance and the counter, kept in
  `MagicState`); the line test is `World::raycast_m` against every triangle [I: mask 2's surfaces not told apart].
  Oracle test `scatter_place_matches_fun_001a5ee0` (kh2-ee runs FUN_001a5ee0 on the user's executable with the line
  test stubbed to always / never clear: same place, generator state and counter for six seeds), and
  `scatter_place_by_hand`. Harness (tt00, `KH2_DRIVE=4 KH2_DRIVE_AT=3 KH2_MAGIC_LOG=1 KH2_YAW=1.57`, keys
  `2:F3,6.8:G,7.1:J,7.25:G,7.4:G,7.6:J`): `magic: Thunder event 21 (kind 30): 3 strikes`, then twice `magic: Thunder
  in Final: host 1.248 around Vec3(1.152, 0.8, 2.65) (radius 170 .. 425): Vec3(-0.54, 0.45, 2.50) (generator
  0x16c43621, counter 1)` with the strikes 8 frames apart.
- Cure: the kind alone (rows 708 / 709 of +10 / 20 / 30 exist, `cure_heals_for_every_kind_from_the_disc`).
- Magnet, Reflect: their scripts' own branches in the interpreter.
- Valor and Anti cannot cast (`DisableForm`, unchanged). The form is the player object's (`DriveState::object_form`
  in the command menu and in `magic::run`).

### The interpreter host (WP-7I additions)

`magic::SpellHost` now also answers: 1.138 (an element placed in the world), 0.38 (its position), 0.49 (the
instance still runs), 0.51 (destroy), 1.60, 2.23 / 1.140 / 1.120 (the cast's target record), 2.21 (the persistent
block, read and written through the script's memory), 2.66 (the attack's clash count), 1.187 / 1.188 (nothing).
Effect handles are kept in one table for all casts (`VmWorld`), since Reflect keeps a handle in its block from
cast to cast.

WP-7M2 named 0.39 (`effect_set_vector2`, `pv`), 1.141 (`probe_ground_down`, `vpf -> i`), 1.146 (`push_object`,
`Ovf`), 1.248 (`scatter_point`, `vffff -> v`) and 2.14 (`attack_hit_anything`, `p -> b`) in `kh2_bdx::names` (rows in
docs/systems/script-host/contract.md). 0.39 is now read: Magnet's sphere grows from 0 to 1 and shrinks back (test
`magnet_script_runs_in_the_interpreter` samples element 3's scale: first 0.0, maximum 1, last 0.0096).
1.141 (WP-F1 item 1, closed): its second argument is where FUN_001a5c30 writes the found point (or copies the start
point), and MAGNET_*.mag passes `lea local+0x20`, an address in the script's own stack, which a host cannot write.
The runtime (`kh2_bdx::runtime`, `dispatch`) now has its own arm for (1, 141): it decodes the point and the distance
(`vpf`), passes the call to the host, writes the host's `Value::Vec(p)` to the second argument and returns 1, or
writes the start point there and returns 0 when the host answers anything else (test
`probe_ground_down_writes_the_found_point_through_the_script_pointer`). `SpellHost::probe_ground_down` answers from
the area's collision (`World::ground` from the point straight down by the distance; `VmCtx::map`), so Magnet's attack
sits `magi` +0x3c above the floor under the place. Log (`KH2_MAGIC_LOG=1`): `magic(vm): 1.141 ground probe from
(x y z) down 10000: floor at y N (game space)`.

### Effect colours (Fire's sparks)

Fire's sparks were drawn pale tan. The data: `KH2_FXLOG=1` on a Fire cast shows the vertex colours of FIRE_1.mag's
elements, for example (1.24, 0.59, 0.23) of 0x80 (bytes about 0x9e, 0x4b, 0x1e) on the sparks, with GS ALPHA 0x48
(A = Cs, B = 0, C = As, D = Cd: additive). The GS modulates the texel by the vertex colour in display space, so a
white texel comes out orange (255, 150, 59 before the blend). The app multiplied the colours in linear space, which
lifts the weak channels (pale tan). `fx.rs` now raises the vertex colour channels to 2.2 before the mesh (the HUD's
`gamma22` convention; alpha stays linear). Screenshot of the same cast after the fix: orange-red sparks. [The
additive blend itself still sums in linear space, which differs from the GS's sum in display space where effects
overlap: not changed.]

## Not ported / gaps

- The menu's input code (cursor, wrap, closing after a cast), above.
- Running Blizzard, Thunder and Cure in the interpreter (above). Phase 2 gaps closed in WP-7M2: Magnet's pull,
  Final Thunder's scatter, Blizzard's shot flag and push, host 0.39, and (WP-F1) host 1.141's output. Still open:
  the pull mover 15 (air) against 14; the pull mover's leave method 0x331388; the line-test mask 2
  of host 1.248 (every triangle blocks in the app); Sora as the victim of reaction class 5 (`sora.rs` still takes
  it as `NotDecoded`); a pull by an attack that is not a spell's (the app knows only spell attack objects).
- The lock-on controller's mode-1 marker, its re-pick when the timer ran out (FUN_0018f538 case 6) and mode 2 (above). Which target record the menu code puts into a spell's
  request (FUN_00174cf8 copies it; its writer was not traced) [I: Sora's target, Cure's the caster].
- The HUD's MP charge bar: `hud_gauge::MpRecharge` gets +0x1bc (time left, which counts down) and +0x1c0 as its doc
  names them; whether the game's bar fills with the left time or with the elapsed time was not traced in the HUD code
  (0x1556a0), and in a run the recharge bar is drawn at the right edge of the screen (hud_gauge layout, not checked).
- `enemy::receive_hits` looks the row up with `GameData::atkp` (the first row of the id): until it uses
  `GameData::hit_row`, every Fire hit uses `SubId` 0 for damage (the hit spark already uses the hit's row, WP-7A).

Done in WP-7A: element bones 0x4001..0x4009 (docs/systems/combat-effects/effects-damage.md "Object bones"), so the cast effects start at
the keyblade's tip; the spell's own hit spark (`fx::SpellHit`); the cast voices; the form-start refill; the magic
turn; the interpreter entry point.

## Decompile index

| address | what |
|---|---|
| 0x193610 -> 0x1a4c68 | `magc` table pointer (0x1c95520) |
| 0x1a4c78 | `magc` row of a slot |
| 0x1a45c0, 0x1a4530, 0x1a4f00 | load / free the six magic resources (table 0x35e810) |
| 0x1a4798, 0x179400, 0x174cf8 | cast start, its caller, the command executor |
| 0x19b7a8, 0x19b3f0, 0x19bcb8, 0x1861b8, 0x19a4f0 | lock-on controller frame, mode change, mode-1 timer set, trigger 28, player-up test (WP-MG2) |
| 0x172f40, 0x16b6e0, 0x18eec0, 0x18ee20, 0x1894d0 | host 1.88 and the target point (WP-MG2) |
| 0x1905e8, 0x1a3848, 0x1bd358, 0x1b9140 | global flag 0x35d268 bit 3 reader, its two callers, its second writer (WP-MG2) |
| 0x1a4a60, 0x188498 (case 0x11) | frame trigger 17: pay and event 21 |
| 0x1a42b8, 0x1a43b8, 0x1a44a8 | magic thread list: link, destroy, clear |
| 0x1bbee8, 0x1b6f48, 0x1a4dd0 | magic list, command lookup, slot command |
| 0x1ba2c0, 0x1a4bb8, 0x178a78 | availability |
| 0x1769e0, 0x186368, 0x186440, 0x1a0180, 0x1a0530, 0x1a0540, 0x1a05a8, 0x175f48 | MP pay, charge start, MP add, charge step, charge end, per-frame charge |
| 0x176908, 0x176b00 | finisher test, press logic |
| 0x1cc1f0, 0x1cc140 | `magi` row |
| 0x18e2f0, 0x1a2ba0, 0x1a30e0, 0x1a2fc8, 0x18dc98, 0x1a2c58, 0x1a2cc0, 0x189610 | attack objects |
| 0x1be108, 0x1be190, 0x1be280, 0x1be300, 0x19e7e0 | hit memory, row variant |
| 0x1a0610, 0x1a0a18, 0x185a68, 0x186238 | damage, element boost, heal/damage apply |
| 0x175340, 0x1bff00, 0x1a0100, 0x13de10, 0x17cee8 | receive handler (types 5 and 6 pass), status conversion, HP add and clamp, HUD heal event 4, revive (WP-FIX-13) |
| 0x188030 (cases 0x24, 0x25) | range triggers 36 / 37: status mask bits +0xac0 / +0xac4 (none in the spell motions) |
| 0x173988 | host 1.139: FUN_001db868(effect file, id, 1, 1, 0, object) |
| 0x16d098, 0x16d138 -> 0x17c0f8, 0x18e3d8 -> 0x18c890, 0x172538 -> 0x1a5c30, 0x1705e8 -> 0x1857b0 | hosts 0.45 (move along a direction), 0.46 (turn toward), 2.14, 1.141 (probe), 1.146 (push) |
| 0x18c8e0, 0x18ca78 | terrain segment test of a moving attack, wall handler |
| 0x16aa18, 0x16ae50, 0x16ae90, 0x175df8, 0x16b7d0, 0x16b558, 0x16b4e0, 0x1894d0, 0x16b960 | element bone matrices (0x4000..0x4009): weapon of a hand, blade shape (type 15), body shape (type 1), shape centre |
| 0x175d18, 0x1ae3c8, 0x1970b8 | player method +0x54: the cast voice |
| 0x1a0240, 0x1a05d0, 0x1a05a8, 0x1a0210, 0x1a04b0 | MP refill of the form start |
| 0x179470, 0x18fb98, 0x18ed58, 0x19b3f0, 0x19bcb8, 0x19bbc0, 0x1768a8 | Magic Lock-On: target pick, lock-on controller mode 1 |
| 0x188030 (cases 0x16, 0x1c), 0x186118, 0x1861b8, 0x186018, 0x183848, 0x18efa0, 0x18eec0, 0x18ed98 | the turn triggers 22 and 28 |
| 0x16c500, 0x16c4c0, 0x1d7b78, 0x16c4d8, 0x1d7bb0, 0x1d7b30, 0x16c9b8, 0x16c230, 0x16c7c8 | a thread pointed at an event |
| 0x173988 -> 0x1db868 | host 1.139 (effect file, id, mode, 1, extra, the context's object) |
| 0x188498 (ghidra), 0x1889d0, 0x188cc0, 0x17f588, 0x17e220, 0x17e000 | trigger 17's parameter against the motion controller's +0x14 (the requested motion id) |
| 0x1a4bb8, 0x1ba2c0 (`cmd +4`) | the MP Charge switch bit of the command's slot |
| 0x185668, 0x1bf920, 0x1bfc10, 0x1bfba8, 0x1bfdf8, 0x186600, 0x184dd0 | reaction class 5: Magnet's pull (victim side, WP-7M2) |
| 0x3332f0, 0x35f348 (vtable), 0x16b698, 0x16b648, 0x179868 (0x35ca48), 0x16b7d0, 0x16b558, 0x16b4e0, 0x1894d0, 0x16a828, 0x18ccf8, 0x1be2c8, 0x19e4c0, 0x1a1048 | the pull mover, the mover switch, the body centre, the facing, the release of +0xaac |
| 0x37bd18, 0x37bd28, 0x37bd2c, 0x37bd30, 0x37bd34, 0x37bd38, 0x37bd3c, 0x356f40 | the pull's numbers (`PullConsts`) |
| 0x172730, 0x1a5ee0, 0x1a6068, 0x1a63c8, 0x1a5bf8, 0x17bbc0, 0x300278, 0x3000d8, 0x35e9b0, 0x37b5a8..0x37b5d0 | host 1.248 (`scatter_place`, `ScatterConsts`) |
| 0x172538, 0x1a5c30, 0x16dad0, 0x1705e8, 0x18e3d8, 0x18c890 | hosts 1.141, 0.39, 1.146, 2.14 |
| 0x18d430, 0x18d808, 0x18e818 | clash handler counting byte +0xa4, the pair test, host 2.66 |
| 0x1729f8, 0x18e928, 0x16dad0, 0x16cf08, 0x17c2e8, 0x11b4d8, 0x11be08 | hosts 1.60, 2.21, 0.39, 0.13 and the Y rotation |
| 0x172730, 0x1a5ee0, 0x1a6068, 0x1a63c8 | host 1.248: Final Thunder's random places (WP-7M2) |

**WP-FL1 (global flag word 0x35d268, readers).** `magic_rules::GlobalFlags` holds the word; each writer publishes its own bits (magic: bit 3 from hosts 1.187 / 1.188 through `VmReq::GlobalFlag`; summons: 0x800 and 0x1000, `summon::publish_gflags`). The brief's reading of FUN_001a3848 and FUN_001bd358 as the attack hit dispatch and the enemy target pick was wrong: read in full, FUN_001a3848 is the prize pickup dispatch (the prize manager's check, prize.rs) and FUN_001bd358 the item object's pickup check (already cited in prize.rs); neither touches an enemy or an attack. Rules ported as pure functions: `global_flag_8_active` (FUN_001905e8), `global_flag_800_active` (FUN_00190610), `prize_dispatch` (FUN_001a3848 up to its member loop: bit 0x800 ors 8 into the record's +0x28; bit 3 hands the prize to FUN_001a3a88 for the player and returns; the record's flag 8 with +0x44 at least 3 does the same and runs on into the member loop), `item_taken_by_player` (FUN_001bd358 after FUN_001a3700 fails and FUN_001bdb10 is false: either bit takes the item), and `update_gate_global` (bit 0x1000 of FUN_0019afe8). Test `global_flag_readers_follow_the_executable`. NOT wired: the callers are in prize.rs (outside this package); FUN_0019afe8's callers FUN_00184498 (the weapon object's update, skipped while the gate holds) and FUN_00191c18 (clears +0x768 bit 8 of the object while it holds; the bit's meaning was not found) need the objects' own +0x10c / +0x108 bits and DAT_0035ee9c, which the app has no model for [GAP].

WP-FL2 correction: FUN_001a3848 and FUN_001bd358 are the prize pickup check and the item pickup check, not an attack hit dispatch or an enemy target pick (see WP-FL1 in `docs/systems/drive-limits-summons/contract.md`). Final Magnet's bit 3 is now read in `prize.rs`: prizes go to the player at once and items are taken without the reach test (`docs/systems/combat-effects/prizes.md`, "Global flag word in the pickup path"). Enemies are not affected.

**WP-WU1 (update gate applied).** `sora::sync_weapon` now asks `sora::weapon_update_held` (`magic_rules::update_gate_global`, bit 0x1000 of 0x35d268). FUN_00184498 calls FUN_0019afe8 on the owner (`this+0x9f4`); when it returns 1 the weapon's virtual update (vtable +0x48) is skipped and its +0x10c bit 8 is cleared, so the weapon motion stops (the app sets the weapon animators' speed to 0 and logs "weapon update gate" on each change). FUN_00191c18 uses the same gate for the owner's +0x768 bit 8 (cleared while it holds, set otherwise); that bit has no reader found, so it is not modelled [GAP]. The other terms of FUN_0019afe8 (owner +0x10c bits 0x80 / 0x200, DAT_0035ee9c, FUN_0019af28, FUN_0019b100, FUN_002c56c0) and the +0x10c bit 8 clear are not modelled [GAP]. Party members' weapons (party.rs) are outside this package and not gated.
