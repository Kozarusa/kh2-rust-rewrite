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
   - with the ability bit tested at block +0xc & 0x80000 and a slot below 3 or 4, FUN_00179470 (turns to the target
     [I]) [GAP: not ported];
   - airborne (FUN_0019aa28): the mover's method +0xc with `sstm` MagicJump (+0x48) [I: a jump start], the air motion;
     on the ground: the finisher motion when it is a finisher and the row has one (`FinishMotion` not 0xffff), else the
     ground motion (FUN_0017e220 with the row's blend); FUN_0017e378(8, 0, ..., 4) in the air [GAP: meaning];
   - the voice: `Voice`, `VoiceFinisher` for the finisher motion, `VoiceSelf` when the command's target is the caster
     (vtable +0x54) [V; not played: the app has no voice hook for it, GAP];
   - the kind: `2 * Level - 2`, + 1 for the finisher motion, + 10 / 20 / 30 in forms 2 / 3 / 4;
   - a script thread on the slot's program (FUN_0016c9b8), linked into the magic list at 0x1c95518 (FUN_001a42b8), and its
     event 20 set as the thread's code (FUN_0016c500) with (caster, the slot's effect file, kind, `magi` row of the kind).
   - FUN_00179400 stores the instance at +0xb28 and sets flag 0x40000 at +0x10c.
5. The task FUN_001a4428 runs every thread of the list once per frame and destroys the ended ones (FUN_001a43b8).
6. Frame trigger 17 of the cast motion (FUN_00188498 case 0x11: with no parameter always, with one only when it equals
   a word of the trigger context) calls FUN_001a4a60 on +0xb28: for a party member (+0x588 & 4) FUN_001769e0 pays the
   MP, then FUN_0016c500 restarts the thread at event 21. [V]
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
  not ported: nothing in the app gives MP yet]. A refill (FUN_001a0240, e.g. the drive form start) ends the charge; the
  app ends it when Sora's MP is raised from outside while charging.
- HUD: FUN_001a0030 copies +0x1bc / 60 and +0x1c0 / 60 to the HUD record; `hud_gauge::MpRecharge` gets +0x1bc and
  +0x1c0. [V]

## Fire (FIRE_*.mag)

Event 20: `kind / 10` of 1 or 3 (Wisdom, Final) starts no charge effect, 2 (Master) clears the finisher bit of the kind;
otherwise element 0 of the effect file starts on the caster ([g]+0x1c). Then it waits (above).

Event 21 [V, script]:
1. Final: stop [g]+0x1c, start element 1.
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

Event 21 [V, script]: stop [g]+0x1c, start element 1; the centre is the target's bone 0x4001 (host 1.83) [I: the app
uses the target's root]; element 2 is placed there (host 1.138 + 0.38). For party members 0..3 (host 1.93): a valid,
not dead (host 10.31 [I]) member whose horizontal distance to the centre is below `magi` +0x38 gets element 3 and atkp
708 if it is the target, else element 4 and atkp 709 (and host 0.100); the attack object hits it directly (host 2.17)
and is finished. Atkp 708/709 are type 6: the "damage" `(Magic * Power + 99) / 100` (FUN_001a0610) is added to HP
(FUN_00185a68: types 5 and 6 are positive) and clamped to the maximum (FUN_001a0100). Port: `magic_rules::heal_amount`,
`magic::event21` (Sora only: the app has no party members).

## Blizzard and Thunder [GAP]

Their event 21 starts effects and then makes the projectiles in request threads (host 2.20 -> FUN_001a4738) whose
routines are not reached by the disassembler's flow walk (Blizzard: routine at word 0x319 with arguments set by
R02d7; Thunder similar). They also read the target (2.23, 1.120), the facing (1.201) and 1.146 / 1.248 (not named).
These routines are not ported: the app plays the cast, pays the MP and logs that the projectile is missing. No attack is
made.

## Not ported / gaps

- Running the spell scripts in the BDX interpreter: `kh2_bdx::Runtime` can run an event only to its first yield or
  start events 0 and 1; the game starts a script's event as the thread's persistent code (FUN_0016c500 -> FUN_0016c4d8).
  With such an entry point the scripts could run as they are (the host functions they use are listed above).
- The voices of a cast, the magic lock-on turn, Wisdom / Master / Final variants beyond Fire's script branches, Magnet,
  Reflect, levels 2 and 3 (the rules handle them; the effects and scripts are the same code paths but were not looked at).
- The hit spark of a spell: `fx::hit_sparks` takes the keyblade's effect file for every player hit; for a spell the
  attacking object's effect file is the spell's (`EffectOnHit` category in the `.mag` PAX) [not changed: fx.rs].
- `enemy::receive_hits` looks the row up with `GameData::atkp` (the first row of the id): until it uses
  `GameData::hit_row`, every Fire hit uses `SubId` 0.

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
