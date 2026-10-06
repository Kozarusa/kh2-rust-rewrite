# Magic (work package 6B, phase 1: Fire, Blizzard, Thunder, Cure at level 1)

How Sora casts a spell in the PS2 USA release (`SLUS_210.05`), read from the decompiled executable and the
disassembled spell scripts (`kh2 bdx magic/FIRE_1.mag`), and what `crates/kh2-app/src/magic.rs` and
`magic_rules.rs` do with it. Addresses are functions of the executable (`FUN_xxxxxxxx`). Marks: **[V]**
read in the code or the data, **[I]** inferred, **[GAP]** not found. No game values are written here or
in the code; they come from the user's disc.

## Data

| source | what |
|---|---|
| `magc` (00battle.bin, 56-byte rows) | per spell and level: `Id` (slot 0 Fire, 1 Blizzard, 2 Thunder, 3 Cure, 4 Magnet, 5 Reflect), `Level`, `World` (0 any, 10 the Pride Lands variants), the resource BAR `FileName` (`magic/FIRE_1.mag`), the `Item` whose count is the level owned, the `Command`, the ground / finisher / air motions with their blends, and three voices. [V, FUN_001a4c78, FUN_001a4798] |
| `cmd` | the spell commands: `Execute` 2, `Argument` = the magic slot, `Cost` = MP (255 = all MP), `SubMenu` 1 on the generic Magic command 2. [V] |
| `magi` (pref of 03system.bin) | one row per cast kind (FUN_001cc1f0 -> FUN_001cc140: `count`, offsets; a missing row falls back to row 0), read by the scripts as floats: +0 attack radius, +4 attack height, +8 attack time (Fire), +0x1c (Blizzard, host 1.146), +0x38 heal range (Cure). [V for the readers; names inferred from the use] |
| `prty` MpDrive (row of the player, +0x1c) | the MP charge time. [V: FUN_00186440 reads `+0x12c` -> `+0x1c` of the player object; that `+0x12c` is the `prty` row is inferred from the field name] |
| `atkp` | the spell's attack rows: Fire 502, Cure 708 (on its target) and 709 (on another member). `SubId` high byte = the cast kind, low byte = the hit count it starts at (see "Hits"). [V] |
| `magic/<SPELL>_<n>.mag` | a BAR: one PAX effect file (type 18) and two identical BDX scripts (type 3). The game keeps the first script, the effect file and the row per slot (FUN_001a45c0: table at 0x35e810, 0x2c bytes per slot). [V] |

## The command list

- The Magic sub-menu lists, for slots 0 to 5 in order, the command of the slot's loaded row (FUN_001bbee8, the
  player's list at +0x9d4). A slot is loaded when its `magc` item is owned (FUN_001a4c78 returns no row for count 0);
  the row is the first row of the slot's id, replaced by each following row of the same id whose `Level` is not above
  the count and whose `World` is 0 or the current world (`DAT_0033e830`). [V] Port: `magic_rules::pick_row`,
  `magic_list`.
- A command id of a spell resolves to the loaded level's command (FUN_001b6f48 for a magic command whose id is not 2). [V]
- Availability (FUN_001ba2c0, `Execute` 2): 5 (not usable) while MP charges (FUN_001a05d0: charge time > 0); with MP
  below the cost, 2 (usable) when the cost is 255 or the spell's bit at 0x340eac is clear, else 5 (FUN_001a4bb8; the bit
  is toggled only by the pause-menu code FUN_00274e50 [I: a per-spell switch of that menu], clear in a new game);
  otherwise 0. The command locks (FUN_001b70b8) and `DisableForm` (FUN_001b7290: the mask of the command whose id is the
  `Execute`, so command 2's mask, which hides Magic in Valor and Anti) give 5. A status below 4 is usable
  (FUN_00178a78). [V] Port: `magic_rules::availability`. So a spell can be cast with too little MP: it takes what is
  left and starts the charge.
- Menu art and layout are `hud_menu.rs` (WP-4): the sub-menu is page kind `cmd(2).SubMenu`; an unusable entry is drawn
  in style 1 [I: the style byte +0x11 of a list entry is set by FUN_001ba748 from other conditions; the mapping of
  status 5 to style 1 is the app's].
- [GAP] The menu's input code (cursor movement, wrap, what closes the sub-menu after a cast) was not traced. The app
  wraps the cursor and keeps the sub-menu open after a cast; B goes back.

## Casting

Executing a magic command (FUN_00174cf8, `Execute` 2) [V]:

1. The command's target record is copied to the player (+0x9f4..); FUN_00177a80 (players).
2. FUN_00176b00(player, 1): the attack-button press logic with the hit latch forced: a cast continues a combo in an
   allow-combo window, restarts the chain from an idle range, or is refused. Port: `Chain::press(windows, true)`.
3. +0x490 and +0xaf4 take words of the command request; +0xaf4 is the byte FUN_001769e0 later subtracts from MP. [I: the
   writer of the request's byte 5 for a menu command was not found; FUN_001ce518 stores `cmd.Cost` there for command
   0x1c4, and the cost 255 of Cure gives the game's "all MP" behaviour through FUN_00186368, so it is taken to be the
   command's `Cost`.]
4. FUN_00179400 -> FUN_001a4798(player, command, finisher): finisher = FUN_00176908(player, 0): the combo counter
   (+0xb04) has reached the ground or air combo length of the ability block (+0x28 / +0x29) (not in form 6; FUN_001768b8
   and flag 0x20 of +0x168 are [GAP]). Then:
   - the slot whose row has this command;
   - with the ability bit tested at block +0xc & 0x80000 (bit 115, Magic Lock-On: the disc's ability row 403 owns
     it) and a slot below 3 or 4, FUN_00179470: see "The magic turn";
   - airborne (FUN_0019aa28): the mover's method +0xc with `sstm` MagicJump (+0x48) [I: a jump start], the air motion;
     on the ground: the finisher motion when it is a finisher and the row has one (`FinishMotion` not 0xffff), else the
     ground motion (FUN_0017e220 with the row's blend); FUN_0017e378(8, 0, ..., 4) in the air [GAP: meaning];
   - the voice: `Voice`, `VoiceFinisher` for the finisher motion, `VoiceSelf` when the command's target is the caster,
     through the player's method +0x54, FUN_00175d18 (the player vtables at 0x35c9a8, 0x3601e0, ... hold it there):
     the voice table picks a line against the NeoStatus of the target record's object (FUN_001ae3c8) and
     FUN_001970b8 plays it. [V] Port: `audio::SoraVoice` -> `audio::sora_voices`, the same pick as the motion's
     voice triggers (`audio::sora_voice`). The app's Cure targets Sora, so it plays `VoiceSelf`; the other spells
     `Voice` / `VoiceFinisher` [I: which record the menu puts in the request for a spell was not traced];
   - the kind: `2 * Level - 2`, + 1 for the finisher motion, + 10 / 20 / 30 in forms 2 / 3 / 4;
   - a script thread on the slot's program (FUN_0016c9b8), linked into the magic list at 0x1c95518 (FUN_001a42b8), and its
     event 20 set as the thread's code (FUN_0016c500) with (caster, the slot's effect file, kind, `magi` row of the kind).
   - FUN_00179400 stores the instance at +0xb28 and sets flag 0x40000 at +0x10c.
5. The task FUN_001a4428 runs every thread of the list once per frame and destroys the ended ones (FUN_001a43b8).
6. Frame trigger 17 of the cast motion (FUN_00188498 case 0x11: with no parameter always, with one only when it equals
   a word of the trigger context) calls FUN_001a4a60 on +0xb28: for a party member (+0x588 & 4) FUN_001769e0 pays the
   MP, then FUN_0016c500 restarts the thread at event 21. [V] The word is the motion id the cast asked for [I from the
   data: spells share cast slots (Fire's ground motion 56 and Reflect's 71 play one slot) and each slot has one
   trigger 17 per motion id: Reflect's at frame 0, Fire's at 22]. Before WP-7A the app fired on any trigger 17, so
   Fire cast at frame 0 through Reflect's trigger.
7. Event 20 of every spell script waits while the caster's `motion_state` (host 1.15, +0x154) is what it was: a cast
   whose motion is left before the trigger ends its thread there and costs nothing. [V]

Port: `magic::run` accepts a request after `sora::tick` (Ground/Air: free; Attack/Magic: the motion's windows), plays the
motion and puts Sora in `State::Magic` (sora.rs: no input until the motion ends or its blend-to-idle range opens with a
press or a direction; gravity unless a no-gravity range is open). The trigger is read from the motion's frame triggers
each tick. A request waits up to 20 ticks for a window [app rule].

## MP and the MP charge

- Paying (FUN_00186368(player, -cost)): nothing for a dead object (+0x7f8 & 4); if MP + delta < 0, FUN_00186440 starts
  the charge (FUN_001a0530: MP = 0, +0x1bc = +0x1c0 = MpDrive * `DAT_0037a974` (1.0)); otherwise FUN_001a0180 adds the
  delta (clamped 0..max). Paying exactly the MP left leaves 0 MP and no charge. [V] Port: `magic_rules::pay`.
- The charge, each frame (FUN_00175f48, for a living object, not while FUN_001b3c00 holds for the player): +0x1bc -=
  frame step * the ability block's +0x34 (MP Haste family); at 0 FUN_001a05a8 clears both and refills MP to the maximum
  (FUN_001a0210). [V] Port: `Charge::tick` (step 1.0 per 60 Hz tick, the app's convention for `DAT_00356f40`).
- MP gained while charging (FUN_001a0180 with a positive delta) shortens the charge by delta * `sstm[0x7c]` instead. [V;
  not ported: nothing in the app gives MP yet]. A refill (FUN_001a0240, from the drive form start FUN_001a04b0) ends a
  running charge (FUN_001a05d0: total +0x1c0 above 0; FUN_001a05a8 clears both times) and then raises MP by its maximum
  (FUN_001a0210 -> FUN_001a0180). Port: `magic::refill_mp`, called by `drive.rs` at the form start (WP-7A; the earlier
  stand-in that ended the charge whenever MP rose from outside is gone).
- HUD: FUN_001a0030 copies +0x1bc / 60 and +0x1c0 / 60 to the HUD record; `hud_gauge::MpRecharge` gets +0x1bc and
  +0x1c0. [V]

## Fire (FIRE_*.mag)

Event 20: `kind / 10` of 1 or 3 (Wisdom, Final) starts no charge effect, 2 (Master) clears the finisher bit of the kind;
otherwise element 0 of the effect file starts on the caster ([g]+0x1c). Then it waits (above).

Event 21 [V, script; corrected in WP-7A by running the script in the interpreter]:
1. Every kind but Final's: stop [g]+0x1c, start element 1 (R001a: `call R01f9; i.iszero; jz` skips R017d for
   Final, which started no charge effect). The earlier port had this the other way round.
2. Airborne (host 1.57): element 5 (and 7), else element 2 (and 6) bound to the caster; the first is the one the attack
   follows ([g]+0x24). The second starts for Wisdom finishers, Master, and Final finishers.
3. Final: element 10 (a second attack follows it) and 11, 12 (finisher) or 8, 9.
4. Yield one frame. Attack object: `attack_create(ctx, 502, -1, kind)` (host 2.10 -> FUN_001a2ba0, a new hit group),
   anchored to the effect file (2.30), size `magi` +0 / +4 (host 2.11 -> FUN_001a30e0: radius, and height = the second
   value when positive, else the radius). Final: host 2.31 copies it with id 502 (FUN_001a2c58), 30 smaller; a Final
   finisher widens the first to 450.
5. For `magi` +8 frames: `attack_move_to` the followed element's position (host 0.53 -> FUN_001d9d00), yield, minus
   `frame_time`. Then `attack_arm` (host 2.13: flag 0x800, the attack object is finished [I]).

Port: `magic::event21` and the attack loop in `magic::run`. The element position is the owner's matrix times the
element's own translation (the bind record's position) [I: FUN_001d9d00 read as the bind record's position vector].

## Attack objects and hits

- FUN_001a2fc8 (host 2.12): the previous position is kept; a move of more than two frame steps sets the swept flag.
  FUN_001a2cc0 (each frame): the volume (FUN_00189610: kind 3 sphere when radius = height, else kind 0, radius and half
  height around the position) is tested at the position and, when swept, at steps of `min(radius, height)` back toward
  the previous position while more than half a step is left (`DAT_0037b460` 0.5). [V] Port: `sweep_points`,
  `attack_volume` (the app's capsule: a vertical segment of `height - radius` either side [I]).
- The victim's hit memory (+0xa50; FUN_001be108, FUN_001be190, FUN_001be300) [V]: a hit marks the attack's group;
  rows of type 5 and 6 always hit, others not while the group is marked. Consecutive hits by the same attack count up;
  the count picks the row variant (FUN_0019e7e0: following rows of the same id and `SubId` high byte whose low byte is at
  most the count, and whose `Switch` is 0 or matches the parity). When the row used has an `Interval`, one of four timers
  clears the group after that many frames, so the attack can hit again. Port: `HitMemory`, `variant`.
  For Fire level 1 (kind 0) the first two hits use `SubId` 0 and the third `SubId` 2; see the test
  `magic_rows_from_the_disc`.
- A spell hit is a `combat::Hit` with `row: Some(SubId)` (the magic damage entry point); `GameData::hit_row` gives the
  row. The damage is the game's formula (FUN_001a0610) with the Magic stat when `CriticalAdjust` is 1, then the element
  boost of the ability block (FUN_001a0a18: +0x40/+0x42/+0x44 for elements 1/2/3), as `enemy::receive_hits` already
  applies through `abilities::attacker_bonuses`.

## Cure (CURE_*.mag)

Event 20: element 0 on the caster ([g]+0x1c); the target is the command's target record when it is valid (host 2.23,
1.140), else the caster. With host 1.343(caster, 114) true and a target that is not the caster, a request thread is
started (routine at word 0x233) [GAP: not ported; the app always casts on Sora].

Event 21 [V, script]: stop [g]+0x1c, start element 1; the centre is the target's bone 0x4001 (host 1.83: its body
shape's centre, `fx::object_bone`; the app's target is Sora); element 2 is placed there (host 1.138 + 0.38). For party members 0..3 (host 1.93): a valid,
not dead (host 10.31 [I]) member whose horizontal distance to the centre is below `magi` +0x38 gets element 3 and atkp
708 if it is the target, else element 4 and atkp 709 (and host 0.100); the attack object hits it directly (host 2.17)
and is finished. Atkp 708/709 are type 6: the "damage" `(Magic * Power + 99) / 100` (FUN_001a0610) is added to HP
(FUN_00185a68: types 5 and 6 are positive) and clamped to the maximum (FUN_001a0100). Port: `magic_rules::heal_amount`,
`magic::event21` (Sora only: the app has no party members).

## Blizzard (BLIZZARD_*.mag)

The shot is a request thread (host 2.20 -> FUN_001a4738) at word 0x319, which the disassembler's flow walk does not
reach on its own (`cargo run -p kh2-bdx --example listing` with root 319 after pointing it at the `.mag`). [V, script]

Event 21, kind / 10 = 0 (level casts without a form): stop [g]+0x1c, start element 1 (bound to bone 0x4003, the weapon of
hand 0: FUN_0016aa18 -> FUN_0016ae50), yield; host 1.146 with the facing turned by pi and `magi` +0x1c (FUN_001857b0: a
push of the caster backward [I]; not ported, GAP); then R02b9 -> R02d7 -> R04d6 start the shot with a 112-byte block: the
caster's context, a copy of its target record (+0x30), the start position = element 1's position (+0x20), the direction
= the facing (+0x50), the turn rate `magi` +0xc times the scale 1.0 (+0x60) and a flag 1 (+0x64). Wisdom, Master and
Final send three to seven shots at other angles (not ported).

The shot (R035b): element 2 unbound (host 1.138); `attack_create(ctx, 248, -1, kind)` sized `magi` +0x18 both ways (a
sphere); the attack first moves to the caster's lock-on point (host 1.88) [GAP: the app starts it at the shot's position,
so the first swept segment is missing]; life `magi` +0x10 - 12. Each frame while the effect lives: it ends when the
attack's flags 8 / 0x10 (host 2.29) or 0x200 (2.39) are set, or with flag 0 of the block when host 2.14 (FUN_0018c890)
holds; otherwise the effect's second vector takes the direction (0.41), the effect and attack move to the position
(R045b), yield; with a valid target and a rate above 0 the direction turns toward the target (host 0.46 -> FUN_0017c0f8:
by the angle between them clamped to the rate, about their cross product); the position moves `magi` +0x14 along the
unit direction (host 0.45 -> FUN_0016d098); at the end of the life the effect fades over 12 frames and is destroyed
(host 0.52). Cleanup (R0487): destroy the effect, finish the attack, release the target record, free the block.

Port: `magic::step_shot`, `turn_toward`. The flags: 8 / 0x10 are taken to mean "hit a target" and 0x200 "hit terrain"
[I: their writers were not found; FUN_0018c8e0 (from FUN_001a2cc0 for an attack that moved more than two units) casts the
segment from the last position against terrain with mask 4 for rows with BG Hit and calls the wall handler FUN_0018ca78].
Atkp 248 has BG Hit. The fade is the effect's loop end in the app (`fx` has no handle for a timed fade) [I].

## Thunder (THUNDER_*.mag)

Event 21, kind / 10 = 0: stop [g]+0x1c, start element 1; the place is the target record's position (host 1.120) when the
caster has a valid target, with a probe height of 100; otherwise 300 ahead of the caster along the facing, 80 up, with
330. R02b5 starts the strike thread (word 0x2da) with the place (+0x20) and the probe height (+0x48). Master, Final and
the Wisdom finisher make more strikes (not ported). [V, script]

The strike (R031c): host 1.141 (FUN_001a5c30 with mask 4, distance 10000) probes from the probe height above the place;
ground found less than `magi` +0x20 below the place moves the strike onto it and picks element 2 and atkp 250, else
element 3 and atkp 251. The element is started there unbound. After `magi` +0x30 frames (R029f: count down, yield) the
attack (atkp 250/251, radius `magi` +0x24) is made at the place, and a copy (host 2.31, atkp 249) sized `magi` +0x2c /
+0x28 is raised by +0x28; both live `magi` +0x34 frames, then both are finished. [V, script; the probe's direction and
the copy keeping the original's group are I]

Port: `magic::step_strike`; the probe is the app's floor test `World::ground_m`.

## The magic turn (WP-7A)

- FUN_00179470 (Magic Lock-On, for Fire, Blizzard, Thunder and Magnet): when the command's target record (+0x9f4,
  copied from the request by FUN_00174cf8) is not valid (FUN_0018ed58), FUN_0018fb98 picks one into it with a range of
  0.0. Its first test `range <= 0.0` then lets every candidate through (no distance or height window): every target
  shape (type 3, records without bit 0 of +2) of every targetable object, scored `(d + |dy| * plyr+0x6c) * ((1 -
  facing . dir) * 0.5 * (1 - plyr+0x70) + plyr+0x70)`, the lowest wins. When one is found the lock-on controller
  (+0xb40) goes to mode 1 on it (FUN_0019b3f0: the target marker, FUN_0013e528 with 5 or 1) and FUN_0019bcb8 sets its
  timer from the player's +0x130 row +0x68. [V] Port: `magic_rules::lock_on_pick`, `magic_lock_on_applies`; the mode-1
  target is kept for the cast motion. [GAP: the marker and the controller's mode-1 timer count-down are not ported.]
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

`kh2_bdx::Runtime::start_thread` / `restart_thread` give the spell thread of FUN_001a4798 (docs/bdx.md "A thread
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
request threads (already run by the runtime), 1.88, 1.120, 1.138, 1.141, 1.146, 2.14, 2.17, 2.29, 2.39, 0.38, 0.41,
0.46, 0.52, 1.93, 10.31, 0.100 for the other three.]

## Not ported / gaps

- The menu's input code (cursor, wrap, closing after a cast), above.
- Running Blizzard, Thunder and Cure in the interpreter (above); Wisdom / Master / Final variants beyond Fire's script
  branches, Magnet, Reflect, levels 2 and 3 (the rules handle them; the effects and scripts are the same code paths but
  were not looked at).
- The lock-on controller's mode-1 marker and timer (above). Which target record the menu code puts into a spell's
  request (FUN_00174cf8 copies it; its writer was not traced) [I: Sora's target, Cure's the caster].
- The HUD's MP charge bar: `hud_gauge::MpRecharge` gets +0x1bc (time left, which counts down) and +0x1c0 as its doc
  names them; whether the game's bar fills with the left time or with the elapsed time was not traced in the HUD code
  (0x1556a0), and in a run the recharge bar is drawn at the right edge of the screen (hud_gauge layout, not checked).
- `enemy::receive_hits` looks the row up with `GameData::atkp` (the first row of the id): until it uses
  `GameData::hit_row`, every Fire hit uses `SubId` 0 for damage (the hit spark already uses the hit's row, WP-7A).

Done in WP-7A: element bones 0x4001..0x4009 (docs/effects-damage.md "Object bones"), so the cast effects start at
the keyblade's tip; the spell's own hit spark (`fx::SpellHit`); the cast voices; the form-start refill; the magic
turn; the interpreter entry point.

## Decompile index

| address | what |
|---|---|
| 0x193610 -> 0x1a4c68 | `magc` table pointer (0x1c95520) |
| 0x1a4c78 | `magc` row of a slot |
| 0x1a45c0, 0x1a4530, 0x1a4f00 | load / free the six magic resources (table 0x35e810) |
| 0x1a4798, 0x179400, 0x174cf8 | cast start, its caller, the command executor |
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
