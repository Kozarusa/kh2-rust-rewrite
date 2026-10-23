# Magic (work package 6B, phase 1: Fire, Blizzard, Thunder, Cure at level 1; WP-7I, phase 2: levels 2 and 3, Magnet, Reflect, drive forms)

[Contract](contract.md) | [Index](../../README.md)

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
  is toggled only by the pause-menu code FUN_00274e50: the Customize page's MP Charge switch, a set bit is "Don't MP Charge"; clear in a new game; the bit is `cmd.Argument`, the command's magic slot, FUN_001ba2c0 passes `cmd +4`);
  otherwise 0. The command locks (FUN_001b70b8) and `DisableForm` (FUN_001b7290: the mask of the command whose id is the
  `Execute`, so command 2's mask, which hides Magic in Valor and Anti) give 5. A status below 4 is usable
  (FUN_00178a78). [V] Port: `magic_rules::availability`; `MagicData::status` reads the bit from
  `Progress::spell_switch` (WP-7I; test `mp_charge_switch_follows_the_spell_slot`). So a spell can be cast with too little MP: it takes what is
  left and starts the charge.
- Menu art and layout are `hud_menu.rs` (WP-4): the sub-menu is page kind `cmd(2).SubMenu`; an unusable entry is drawn
  in style 1 [I: the style byte +0x11 of a list entry is set by FUN_001ba748 from other conditions; the mapping of
  status 5 to style 1 is the app's].
- The menu's input is FUN_001ba9b0 / FUN_001bb930 (docs/systems/ui/items.md section 4): the cursor wraps both ways; an issued spell closes the menu to the deck with the cursor on Attack; Cure (SubMenu 4) first opens the target list. Port: items.rs.

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
   MP, then FUN_0016c500 restarts the thread at event 21. [V] The word is the motion id the cast asked for [V, WP-7I:
   FUN_00188498 was decompiled with ghidra (it is not in the decompilation folder): case 0x11 compares the
   trigger's u16 parameter with the context's +0x14 when the parameter count byte +3 is not 0; the context is the
   motion controller (player +0x140, FUN_0017f588 -> FUN_001889d0 -> FUN_00188cc0 with the handler), and +0x14 is
   written only by FUN_0017e000 (from FUN_0017e220, the motion request FUN_001a4798 makes with the row's motion id)
   when the request succeeds. The data agrees: spells share cast slots (Fire's ground motion 56 and Reflect's 71 play
   one slot) and each slot has one trigger 17 per motion id, Reflect's at frame 0, Fire's at 22]. Before WP-7A the app fired on any trigger 17, so
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

### The heal rule (WP-FIX-13) [V: decomp]

The user reported "cure does not heal". What the code of the original says, and what the app does:

- **Amount.** FUN_001a0610 on an `atkp` row of type 6: `(stat * Power + 99) / 100` (integer division), `stat` = the
  caster's stat block +0x188 (Strength) when the row's `CriticalAdjust` is 0, +0x18a (Magic) when it is 1, and 0 for any
  other value. Type 5 rows use a percentage of the target's maximum instead (`max * stat * Power`, rounded up over
  10000); no spell row has it and it is not ported (the port answers 0). Cure's rows (disc, `atkp` 708 for the target,
  709 for another member) are all type 6, `CriticalAdjust` 1: Power 400 / 500 / 600 for levels 1 / 2 / 3, 800 / 1000 /
  1200 in Wisdom, 3000 in Master and Final; 709 has about half of them. So level 1 at Magic 7 heals 28 (Sora's maximum
  at the start is 20). Test `cure_heals_for_every_kind_from_the_disc`, `heal_amount_and_clamp_rules`.
- **Which rows exist.** A cast's kind is `2 * level - 2` (+ 10 / 20 / 30 in forms 2 / 3 / 4, + 1 for a finisher
  motion). Cure's `magc` rows have no finisher motion, so its kind is always even, and rows 708 / 709 exist for every even
  kind the game can cast (0, 2, 4 and +10 / 20 / 30). The same test asserts it, and that `magi` +0x38 (the range) is above
  0 for each, since a range of 0 would skip the heal.
- **Who.** Every party member 0 to 3 that is valid, alive, and whose horizontal distance to the cast point (the target's
  body centre, bone 0x4001) is below `magi` +0x38 (280 / 600 / 1200 at levels 1 / 2 / 3, in game units). The target
  record names the caster, so Sora is always inside (distance 0). The app heals Sora only.
- **When.** At the cast trigger (frame trigger 17 of the cast motion, FUN_001a4a60): the MP is paid, then event 21 starts
  element 3, creates the attack and hits the target directly (host 2.17), which applies the heal at once. Ground cast:
  trigger at frame 18 (0.3 s after the cast starts, A364); air cast: frame 30 (A372).
- **Apply.** FUN_00175340 (the receive handler) passes types 5 and 6 through even when the object is KO'd or untouchable
  for other hits, FUN_00185a68 hands the signed amount to the object's HP method (vtable +0x78 = FUN_00186238), which
  adds it with FUN_001a0100 (clamp to the maximum, slot 0 = HP), posts HUD event 4 (FUN_0013de10; the panel's heal
  handler FUN_00157108 is in docs/systems/ui/contract.md) and revives a KO'd member instead of healing it. Port: `magic_rules::healed`.
- **MP.** FUN_001769e0 pays the cost byte at the trigger through FUN_00186368: Cure's cost is 255, always more than the
  MP held, so MP becomes 0 and the MP charge starts (`prty` MpDrive times 1.0 = 3000 frames; MP Haste shortens it).
  While it runs every spell is unavailable (FUN_001ba2c0, status 5). Verified by log: `menu: command 52 not usable
  (status Unavailable, MP 0, charging true)` in the second cast attempt of a run.

**Finding.** The app's Cure heals. Reproduced with `KH2_MAGIC_HURT=<s>:<hp>` (below) in tt00, tt02 and tt04, on the
ground and in the air: `magic: Cure heals Sora: atkp 708 SubId 0 Power 400 x magic 7 -> 28; HP 6 -> 20`, and the HUD ring
grows (screenshots in the WP report). The earlier accept scenario `magic-cure` showed nothing because Sora starts at full
HP (`HP 20 -> 20`). The cases where a player sees no heal and the original behaves the same:
1. The first cast was at full HP and used all MP; for the next 3000 frames (50 s at 60 Hz) every spell is unavailable and
   the menu entry does nothing.
2. A hit that flinches Sora between the button and the trigger ends the cast: event 20 stops when the caster's motion
   changes (host 1.15), so no MP is paid and nothing heals. None of the four spells' cast motions carries a range trigger
   that arms the caster (36 / 37 set bits of the status mask the reaction table `patn` reads, see
   docs/systems/combat-effects/hit-reaction.md; the test `magic_turn_data_from_the_disc` checks it), and the app sets no status bit, so a Dusk
   or Shadow hit during the 0.3 s wind-up cancels it. What else sets the status bits (abilities such as a cast that cannot
   be interrupted) was not found [GAP: addresses searched: 0x188030 cases 0x24 / 0x25, writers of +0xabc by the host
   calls 1.211 / 1.212].
3. The cast request is dropped when Sora is in a state with no window (`magic: cast of command 52 dropped (state Hurt ...)`).

Fixed in WP-FIX-13: the stat choice (Strength for `CriticalAdjust` 0, nothing for other values, type 5 not treated as a
flat heal) and the heal / clamp as pure functions with disc tests. The MpDrive value is read from the `prty` row through
`+0x12c`, which is inferred from the field name.

**Harness aid.** `KH2_MAGIC_HURT=<seconds>:<hp>` sets Sora's HP once, that many seconds after the start (60 Hz ticks), and
logs `magic: KH2_MAGIC_HURT: Sora's HP 20 -> 6`: the first areas start at full HP, so a Cure has nothing to heal without
it. Display-independent: the HUD reads the same HP.

## Blizzard (BLIZZARD_*.mag)

The shot is a request thread (host 2.20 -> FUN_001a4738) at word 0x319, which the disassembler's flow walk does not
reach on its own (`cargo run -p kh2-bdx --example listing` with root 319 after pointing it at the `.mag`). [V, script]

Event 21, kind / 10 = 0 (level casts without a form): stop [g]+0x1c, start element 1 (bound to bone 0x4003, the weapon of
hand 0: FUN_0016aa18 -> FUN_0016ae50), yield; host 1.146 with the facing turned by pi and `magi` +0x1c (FUN_001705e8 ->
FUN_001857b0: the caster is pushed backward with the decaying ground push of the wall recoil, docs/systems/combat-effects/hit-reaction.md; every
kind reaches it, L00ab; ported in WP-7M2, below); then R02b9 -> R02d7 -> R04d6 start the shot with a 112-byte block: the
caster's context, a copy of its target record (+0x30), the start position = element 1's position (+0x20), the direction
= the facing (+0x50), the turn rate `magi` +0xc times the scale 1.0 (+0x60) and a flag 1 (+0x64). Wisdom, Master and
Final send three to seven shots at other angles (not ported).

The shot (R035b): element 2 unbound (host 1.138); `attack_create(ctx, 248, -1, kind)` sized `magi` +0x18 both ways (a
sphere); the attack first moves to the caster's lock-on point (host 1.88: FUN_00172f40 -> FUN_0016b6e0 builds a target record
of shape 0 on the caster, FUN_0018eec0 gives its point: the centre of the caster's first target shape through
FUN_0018ee20 / FUN_001894d0, else its position, FUN_0016b960) [V, WP-MG2: `caster_lock_point`; the attack is made there
and the first loop pass moves it to the shot's place, so that pass sweeps the segment between them; test
`shot_attack_first_segment_runs_from_the_caster`]; life `magi` +0x10 - 12. Each frame while the effect lives (L039d): it ends when
the attack's flags 8 / 0x10 (host 2.29) or 0x200 (2.39) are set, or when host 2.14 (FUN_0018c890: flag 2, 8, 0x10 or
0x200) holds and the block's flag (+0x64) is 0; otherwise the effect's second vector takes the direction (0.41), the effect and attack move to the position
(R045b), yield; with a valid target and a rate above 0 the direction turns toward the target (host 0.46 -> FUN_0017c0f8:
by the angle between them clamped to the rate, about their cross product); the position moves `magi` +0x14 along the
unit direction (host 0.45 -> FUN_0016d098); at the end of the life the effect fades over 12 frames and is destroyed
(host 0.52). Cleanup (R0487): destroy the effect, finish the attack, release the target record, free the block.

Port: `magic::step_shot`, `turn_toward`, `shot_ends`. The flags (WP-7M2, from docs/systems/combat-effects/hit-reaction.md): 8 / 0x10 are set
by the wall handler FUN_0018ca78 (FUN_0018c8e0, from FUN_001a2cc0 for an attack that moved more than two units, casts
the segment from the last position against terrain with mask 4 for rows with BG Hit), 0x200 by the clash handler
FUN_0018d430, and 2 marks an attack that hit a target (FUN_0018c8e0 skips the wall test for it). So a shot with flag 1
(every cast but the Wisdom finisher's five and Master's seven, `blizzard_plan`) goes through the targets it hits and ends
only at a wall, a clash or the end of its life; a shot with flag 0 ends at its first target. The app's spell attacks
do not clash with enemy attacks (only Reflect's guard does), so the clash term is always false. Before WP-7M2 every
shot ended at its first target. Atkp 248 has BG Hit. The fade is the effect's loop end in the app (`fx` has no handle
for a timed fade) [I]. Test `blizzard_shot_flag_follows_host_2_14`. Harness (the `magic-blizzard` keys, tt00): `magic:
Blizzard shot over at ... (hit true, wall false, flag 1, faded true)` after hits on all four Shadows.

The push (WP-7M2): `magic::run` keeps it in `MagicState::push` (`reaction_rules::Push` with `magi` +0x1c along minus the
facing, stat term 0) and steps it after Sora's tick, since Sora's own push field belongs to `sora.rs`; a hit reaction
(Hurt, Blown, Down) drops it. Harness: `magic: Blizzard push of the caster (host 1.146 -> FUN_001857b0): power 50 ...
speed Some(5.0)`, then `magic: Blizzard push over: Sora moved 40.7 units` (the 41 units of the wall recoil's power 50).
[I: the script calls the ground push also for an air cast; the app applies it in the air as well.]

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

