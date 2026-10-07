# Magic (work package 6B, phase 1: Fire, Blizzard, Thunder, Cure at level 1; WP-7I, phase 2: levels 2 and 3, Magnet, Reflect, drive forms)

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
- The menu's input is FUN_001ba9b0 / FUN_001bb930 (docs/items.md section 4): the cursor wraps both ways; an issued spell closes the menu to the deck with the cursor on Attack; Cure (SubMenu 4) first opens the target list. Port: items.rs.

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
  handler FUN_00157108 is in docs/hud.md) and revives a KO'd member instead of healing it. Port: `magic_rules::healed`.
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
   docs/hit-reaction.md; the test `magic_turn_data_from_the_disc` checks it), and the app sets no status bit, so a Dusk
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
it, element 1; Final sets bit 3 of the global flag word 0x35d268 (host 1.187) over a yield [GAP: its readers]. The
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
ends it when +0xaac is cleared (FUN_00186600). [V for the functions; GAP: not ported, the victim side lives in the
enemy reaction code (`reaction_rules::ReactionClass::Special5` is "not decoded")]. So in the app Magnet hits each
enemy once (atkp 658 has no `Interval`) without pulling.

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

### Drive forms

- Fire: `fire_plan` (event 21 elements per kind; test `fire_hand_port_matches_the_script_for_every_kind` against
  the script for every kind of levels 1 to 3, forms included).
- Blizzard (`blizzard_plan`, R0017): elements 1; Final non-finishers 1 and 7; Final finishers by the program's
  global +0x2c (casts counted at L022b, kept in `MagicState` [I: the globals live as long as the slot's resource]):
  7 for 1 and 3, 1 and 7 for 4, else 1. Shots: the Wisdom finisher five (turn scale 0.5 ahead, 1.0 at +-35 degrees,
  2.0 at +-70 degrees, flag 0); Master seven (scale 1, flag 0: ahead, with the height -0.96 and 0.96, then 0.4 and
  -0.4 each turned +-45 degrees); otherwise one ahead with flag 1 and one more from element 7 when it started. The
  turn rate of a shot is `magi` +0xc times its scale. Host 0.13 (FUN_0016cf08 -> FUN_0017c2e8 with FUN_0011b4d8 ->
  FUN_002fb628) is taken as libvu0's Y rotation [I]. Test `blizzard_forms_follow_the_script`. [GAP: the block's
  flag (+0x64) with host 2.14 (FUN_0018c890) is not ported, nor the push 1.146.]
- Thunder (`thunder_plan`): Master strikes at the caster's position and 0.47 and 0.78 of the way to the place, 16
  frames apart, then the place (without a target 800 ahead turned 45 degrees); the Wisdom finisher three more
  strikes 8 frames apart around the place (`magi` +0x24 * 1.5 along the facing turned 120, 240, 360 degrees).
  Final strikes 2 (7 for a finisher) more times 8 frames apart at places from host 1.248 (FUN_00172730 ->
  FUN_001a5ee0 -> FUN_001a6068 / FUN_001a63c8: a random radius and angle bucket around the place with a ground
  test, the game's generator 0x3716a0) [GAP: not ported]. Test `thunder_forms_strike_counts`.
- Cure: the kind alone (rows 708 / 709 of +10 / 20 / 30 exist, `cure_heals_for_every_kind_from_the_disc`).
- Magnet, Reflect: their scripts' own branches in the interpreter.
- Valor and Anti cannot cast (`DisableForm`, unchanged). The form is the player object's (`DriveState::object_form`
  in the command menu and in `magic::run`).

### The interpreter host (WP-7I additions)

`magic::SpellHost` now also answers: 1.138 (an element placed in the world), 0.38 (its position), 0.49 (the
instance still runs), 0.51 (destroy), 1.60, 2.23 / 1.140 / 1.120 (the cast's target record), 2.21 (the persistent
block, read and written through the script's memory), 2.66 (the attack's clash count), 1.187 / 1.188 (nothing).
Effect handles are kept in one table for all casts (`VmWorld`), since Reflect keeps a handle in its block from
cast to cast. [GAP: 1.141 and 0.39 are not named in kh2-bdx, so their pointer arguments cannot be read: the ground
probe answers "no ground" and Magnet's sphere keeps scale 1. Naming them in `kh2_bdx::names` (1.141 `vpf`,
0.39 `pv`) would let the host answer them.]

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
- Running Blizzard, Thunder and Cure in the interpreter (above). Phase 2 gaps: Magnet's pull (victim side), Final
  Thunder's random strikes, Blizzard's shot flag with host 2.14 and the push 1.146, hosts 1.141 / 0.39 (see "Phase 2").
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
| 0x185668, 0x1bf920, 0x1bfc10, 0x1bfba8, 0x1bfdf8, 0x186600, 0x184dd0 | reaction class 5: Magnet's pull (victim side, not ported) |
| 0x18d430, 0x18d808, 0x18e818 | clash handler counting byte +0xa4, the pair test, host 2.66 |
| 0x1729f8, 0x18e928, 0x16dad0, 0x16cf08, 0x17c2e8, 0x11b4d8, 0x11be08 | hosts 1.60, 2.21, 0.39, 0.13 and the Y rotation |
| 0x172730, 0x1a5ee0, 0x1a6068, 0x1a63c8 | host 1.248: Final Thunder's random places (not ported) |
