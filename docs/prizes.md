# Prizes: HP, MP and drive orbs and munny (WP-7B)

Module `crates/kh2-app/src/prize.rs`. Status words: **verified** = read in the decompilation or the
disassembly of the user's `SLUS_210.05`, or checked against the game's own code in `kh2-ee`;
**inferred** = follows from what was read; **gap** = not found or not ported. Every number the port
uses is read from the executable at start (`prize::Consts::from_exe`); the code's own immediates
(pool size 0x100, recheck 0x1e, two bounce sounds) are rules.

## From the drop to the pool (verified)

- Host 1.92 -> `FUN_0016b738` -> `FUN_001a3cb0(point, row)` (docs/enemy-ai.md "Drops"). For each of the nine
  kinds `FUN_001a3ef0` calls `FUN_001a3b28(manager, 0x37b4b0 + 16 * kind, point)` once per prize.
- The manager `DAT_0035e7a0` is an object of `PRIZE` (object 0x5c3 in `00objentry`, made by `FUN_001a3f98`:
  `FUN_001a34f8(.., 0x100, 0x60)`, vtable 0x35e7b0 over the pool class 0x35fd38). Its pool (`+0x9f4`,
  `FUN_001bf120`) holds 0x100 entries of 0x60 bytes. `FUN_001c4d78` takes a free entry or returns 0 (pool full:
  no prize and no generator steps).
- `FUN_001a3b28` passes the update `FUN_001c5288` (a3 = 0x1c5288 in the disassembly; the decompiler drops the
  arguments) and fills the velocity with three steps of the generator at 0x3716a0 (`seed * 0x10dcd` each, read
  unsigned): `+0x30 = -20 + u1 * 40`, `+0x34 = -40 + u2 * 20` (Y down: upward), `+0x38 = -20 + u3 * 40`, with
  `u = seed * 2^-32` (floats 0x37b480..0x37b490). Oracle test `launch_matches_fun_001a3b28` runs the game's
  routine in `kh2-ee` (the allocator `FUN_001c4d78` stubbed to hand out a fixed record) and compares velocity and
  generator for five seeds.
- `FUN_001c4eb0` sets the entry: position = the point, kind byte `+0x14` = the entry's first byte, life `+0x40`
  = 900 (0x37bf34), bounce count `+0x44` = 0, recheck `+0x45` = 0x1e, size `+0x16` = 0xff, frame `+0x17` = 0xff
  (`FUN_001bf560`), flags `+0x28` = 0.

The kind entries (0x37b4b0 + 16k): `+0` kind number, `+4` radius, `+8` bounce sound, `+0xc` pickup sound.

## The update (verified)

The pool's per-frame walk (`FUN_001c4d50` = vtable slot 5 -> `FUN_001bf2f8`) calls each entry's `+0x10`
function; 0 frees the entry.

- **Falling** `FUN_001c5288` -> `FUN_001c5248` -> `FUN_001c5060(-0.8, entry)`, then `FUN_001c52a8(entry, 0)`:
  - `FUN_001c4fb0` sweeps the entry's low point (position + radius in Y, Y down) by the velocity through the
    character mover `FUN_00143840` (query block 0x1d5e2d0, result 0x1d5e300, flags 0x1d5e340).
  - Velocity Y >= 0 and the flags' bit 32 clear (taken as "on a floor" [inferred from its two uses]): faster than 5
    (0x37bf44) -> Y velocity times -0.8 (0x37bf58), the bounce sound (`FUN_002c32b8(entry +8, entry)`) while the
    count is below 2, count + 1; else velocity 0 and update `FUN_001c5388` (rest).
  - A sideways axis whose movement differs from its velocity by more than 0.1 (0x37bf48) reverses (walls).
  - Position = the mover's result minus the radius; Y velocity + 3 (0x37bf4c) capped at 50 (0x37bf50); X, Z
    velocity times 0.95 (0x37bf54); Y velocity >= 0 sets flag bit 0 (pickable).
- **`FUN_001c52a8(entry, rest)`**: life < 12 (0x37bf5c): size = `life * 255 / 12` (0x37bf60) and nothing else
  (no pickup while shrinking). Otherwise at rest the recheck counts down; at 0 the sweep runs again and without
  a floor the update goes back to falling; then the manager's vtable +0x58 = `FUN_001a3848` (pickup check).
  Then `FUN_001c5020`: life - 1 unless flag 4; 0 or less ends the entry.
- **Pickup check** `FUN_001a3848`: for the three party slots (`FUN_00175b68`): the member must exist and not be
  down (+0x7f8 bit 4). The player (+0x588 bit 0x80) takes any kind except the HP kinds 0 and 1 in Anti form
  (+0xb50 == 5); a party member takes any kind except the drive kinds 7 and 8. `FUN_001c57b8` (flag bit 0 set)
  -> `FUN_001a3700(0, 0, member, low point)`: vertical distance to the member's position (`FUN_0016b960`) at most
  30 (0x37b47c) and horizontal at most 120 (0x37b478), plus the ability block's +0x30 (Draw) when +0x588 bit 4 is
  set, plus status +0x1c4. Then `FUN_001a3a88` = `FUN_001c5808(entry, member, 0)` + `FUN_001a39b0`.
- **Draw-in** `FUN_001c5808` mode 0: `+0x30` = unit vector from the member's centre (`FUN_0016b7d0`: its first
  shape of kind 1, else +0x540) to the entry, `+0x3c` = the distance, life = 40 (0x37bf7c), update
  `LAB_001c5568` -> `FUN_001c53a8`, the pickup sound at the centre. Each frame: life - 1 (0 or less, or the
  member gone: end); `frac = life / 40` (0x37bf64); the direction is turned about the vertical axis by
  `min(30 / (distance * frac), 0.59341)` (0x37bf68; the limit is 0x37bf88, copied to 0x1d5e2c4 by
  `FUN_001c59b0`); position = centre + direction * distance * frac; size = frac * 255. The turn's sign is pinned
  by `absorb_turn_matches_the_game_rotation` (the game's `FUN_0011b4d8` in `kh2-ee`).
- Mode 1 (collect-all, ported since WP-7Bb): `FUN_001a40b8` walks the pool and, for every entry whose flag bit 0
  is set (whatever its update: one already drawn in starts again and pays again, `FUN_001a3ad8` does not look),
  runs `FUN_001c5808(entry, player, 1)` and the pickup `FUN_001a39b0`. Mode 1 is mode 0 with its own numbers:
  80 frames (0x37bf80), update `LAB_001c5748` -> `FUN_001c5588` with length 80 (0x37bf70), turn numerator 150
  (0x37bf74), size scale 255 (0x37bf78) and the largest turn 0.5236 (0x37bf84, copied to 0x1d5e2c0 by
  `FUN_001c59b0`). Its only caller is `FUN_002eece8` (which also runs `FUN_001bda98`, below), the wrapper of
  host function 6.31 (no arguments; table entry 0x370f58 of the group-6 table 0x371078 named by 0x35bce8).
  No BDX script on the disc calls 6.31 (`kh2 bdx --scan` lists no 6.31); other script systems were not
  searched. The app maps 6.31 in `enemy_host` and sends `prize::CollectAll`; test
  `collect_all_draws_in_over_its_own_frames`.

## The pickup (`FUN_001a39b0`, verified; table at 0x37b540, 8 bytes per kind)

| byte | effect | port |
|---|---|---|
| +0 float | above 0: `FUN_00175c28` starts system effect 6 of 0x1c954d8 on the player (`FUN_001db868(.., 6, 0, 1, 0x2714, player)`) for that many frames when none runs (+0xae4 the handle, +0xae8 the frames) and the player's +0x160 bit 0x4000 is clear; otherwise the frames left are raised to the new value. `FUN_00175cb0` (called from `FUN_00174b68`) counts +0xae8 down and stops the effect (`FUN_001d9f48`) at 0 or with bit 0x4000 | done since WP-7Bc: the timer (`prize::Glow`, test `the_pickup_glow_starts_once_and_extends`) and the effect, `fx::StartBoundSystemEffect { id: 6, owner: Sora's body }` with its `fx::FxHandle` kept in `PickupGlow` (+0xae4) and `fx::StopEffect` (`FUN_001d9f48`, loop end) when the frames run out (test `fx::tests::a_bound_system_effect_carries_its_handle_and_stops_on_request`); logs `glow starts for N frames` and `pickup glow (system effect 6) ends` |
| +4 | HP: the player's method +0x78 (`FUN_001792f0` -> `FUN_00176c98` -> `FUN_00186238`, HP + amount held at the maximum) | done |
| +5 | MP: `FUN_00186368` -> `FUN_001a0180`: during the MP charge the charge time drops by amount * `sstm[0x7c]` (`FUN_001a0540`), else MP + amount held at the maximum | done (the charge end comes on `magic`'s next tick [inferred one tick late]) |
| +6 | drive: `FUN_001788d8(units)`: gauge mode adds `(int)(units * [sstm[0x78] * block +0x38 while charging] * FUN_001ba040())` to the gauge (`FUN_001a0290`); form mode adds `units * sstm[0x70]` to the form timer (`FUN_001a0358`); `FUN_001d4148` gives Master Form (+0xb50 == 3) form experience (`FUN_001d3f88`) | gauge and timer done; `FUN_001ba040` is 1.0 (0x37bc04) outside the Colosseum manager `DAT_01d5dd20`; Master Form experience done since WP-7Bb (below) |
| +7 | munny: `FUN_0014f2d8(amount)` -> `FUN_00151970` on the HUD object `DAT_00358534` (the "OBTAINED MUNNY!" strip, group 0x11 with digits; while it is up the amount is added to the one shown, `FUN_00151c40`), then `FUN_0023e620(amount)`: the word at save + 0x1600 (0x33fe60) + amount, 999,999 when the sum is 1,000,000 or more (`FUN_0023e5e0`) | done (`Progress::add_to_wallet`, `hud_banner` strip) |

The rows on the user's disc (test `real_prize_tables`): kinds 0-1 HP, 2-4 munny, 5-6 MP, 7-8 drive.

**Correction to earlier docs.** The word at 0x340e48 that `Progress::experience` (renamed from `munny` in WP-7Bb) reads and `FUN_001b51e0` raises
(capped at 9,999,999) is followed in that routine by the level-up check of every party member
(`FUN_001b52f8` levels up while the level table's value is at most that word), so it is the experience total;
the digit popup 0x15b198 that shows the value added there (from `FUN_001b25d0` and `FUN_001c46a8`) is an
experience popup. Munny is the save word +0x1600 (`Progress::wallet`). `hud_banner`'s `StripKind::Munny` (was `LevelUp`) is the
munny strip (0x1a39b0 is the pickup), and its `ExperiencePopups` (was `MunnyPopups`) is the 0x15b198 digit popup.
The `KH2_STATE` key `munny` sets the wallet; `experience` sets the 0x340e48 word.

## The look (verified unless marked)

`obj/PRIZE.mdlx` holds only a PAX (BAR type 18): one element, one particle data with the program
`pppRyjMegaBirthPrize` (table index 203; handler table at 0x362b10 with 0x48-byte entries, draw at 0x203378).
The pool's draw (`FUN_001bf848`) writes per entry `position, kind, 0x80, size, frame` into the buffer
`DAT_003625a0`; the handler draws for each entry the shape its kind names in the program's parameters
(`+4 + 12 * kind`: shape, colour bytes, scale), scaled by the kind's scale times size / 255, and steps the
frame by one per draw (a looping shape starts at a random frame when the frame byte is 0xff). On the disc:
HP shape 1 (scale 0.7, 1.5), munny shape 0 (1.2, 0.8, 0.4), MP shape 2 (0.5, 0.9), drive shape 3 (0.9, 1.7).
The sprite faces the camera [inferred: the handler multiplies by the matrix at 0x1db1000]. A sprite's texture rectangle is
`kh2_fx::shape::Sprite::st` (raw / 4096, the normalised ST the game's draw hands to the GS, WP-FIX-9; test
`prize_looks_name_real_shapes` checks it lies inside the texture). Since WP-7Bb each sprite of a frame has its own
material: the texture of the frame's TEX0 plus the sprite's palette offset (`Sprite::tex0`) and the sprite's own blend
(the HP orb's shape mixes an additive 0x48 sprite and a blend 0x44 one); the sprites keep their draw order.

## Tail calls of FUN_001a3cb0 (verified: read, not ported)

`FUN_001c5b10`, `FUN_001c8590` and `FUN_001d54d8` make prizes in the pools of the mini-game prize objects
`PRIZE_MU` (sub type 4, `DAT_0035fd98`, entries 0x37bfb0 by table 0x37bf90), `PRIZE_HE` (sub type 7,
`DAT_0035ff68`, entries 0x37c0b0) and `PRIZE_COLOSSEUM` (sub type 13, `DAT_003607e4`, `FUN_001d55d8` with cup
points 0x37c5d0). Their managers exist only while such an object is loaded (the object factory `FUN_001c0400`
switches on the entry's sub type), so they do nothing in Twilight Town fights.

## Master Form experience (verified, ported in WP-7Bb)

`FUN_001d4148(units, player)`: when the player's form (+0xb50) is 3 (Master), `FUN_001d3f88(player, units)`: unless the
minigame flag `FUN_001b9bd8` is set (the manager `DAT_01d5dd20`, absent in the app: taken as clear) the form record
(`FUN_00187f60(0x33fec0, form)`) +4 grows by the units; then while `FUN_001d3c38` gives a row (the record's level +2 below
the cap `FUN_001d3bd0` = 2 + the owned forms 1..4, 7 instead of 6; the row `FUN_001d3b90(form, level)` of `fmlv`) and the
experience reaches the row's +4, the row's amount is taken off and `FUN_001d3da8` raises the level; without a row the
experience is set to 0. After a level-up `FUN_001b5528` starts system effect 0x10 on the player. `FUN_001d3da8`: level + 1,
the growth ability upgrade in the form's 16 slots and Sora's 64 (`FUN_001d3f18`, table 0x37c556) when the display level
rises, the new row's ability (+2) given to Sora (`FUN_001a18d8(ability, 1, 0)` and the obtained log `FUN_001d0828`), the
level-up popups (`FUN_001b5788`), the stat rebuild (`FUN_0019fb98`) and `FUN_001d5718`.

Port: `prize::add_form_exp` (test `master_form_experience_levels_up_to_the_cap`) and `master_form_exp` on
`Progress` (record level and experience, the ability through `Progress::give`) and `DriveState::level`. Harness:
`KH2_DRIVE=3 KH2_DRIVE_AT=3 KH2_PRIZETEST=8,8,..(40)@8 KH2_PRIZETEST_DIST=60 KH2_PRIZETEST_STILL=1 kh2-game tt02` logs
`(Master Form experience 57->0, level 1 -> 2 (ability 0x183), cap 7)`.

**The level-up side effects (WP-7Bc).** `FUN_001d3da8` step by step (disassembly checked where the decompiler drops a
return value):

1. The record's level byte +2 rises; the row of the new level is `FUN_001d3b90(form, level)` (8-byte rows; byte +1 the
   display level, +2 the ability, +4 the experience).
2. When the row's display level is above the previous row's (`row[-7] < row[+1]`): `FUN_001d3f18(record +8, 16, form,
   display)` on the form's 16 slots and `FUN_001d3f18(Sora's record +0x54, 64, form, display - 1)` on Sora's 64.
   `FUN_001d3f18(list, n, form, level)` reads the chain at `0x37c556 + 10 * form + 2 * level` (five ids per form for the
   display levels 0..4, 0 = none; rows 1..4 on the disc are High Jump, Quick Run, Aerial Dodge, Glide): the first slot
   whose id (low 15 bits) is the chain's id one level below takes the chain's id at the level, its bit 15 (equipped)
   kept; at most one slot changes, an empty slot matches a 0. It returns the chain's id at the level whether or not a
   slot matched (the disassembly at 0x1d3f3c loads it into v0 before the search). The second call's id goes to the
   form popup's line and to the obtained log (`FUN_001d0828(id, 1, 5)`).
3. The row's ability (+2): `FUN_001a18d8(ability, 1, 0)`, the popup line, the obtained log (`FUN_001d0828(id, 1, 5)`).
4. Two popups (`FUN_001b5788`): the form's (title: the form's item name via `FUN_001a1e38`; lines: message 0xc3f2 and
   the form's new growth id) and Sora's (title: message `0x37ba90[1]`; lines: Sora's new growth id, the ability), each
   queued by `FUN_00152ed0` in the four-entry queue at `DAT_003587e8 + 0x24f8` (taken while fewer than three wait).
5. `FUN_0019fb98` rebuilds the status block from the records (`FUN_001a0030`, `FUN_0019f920(status, 0)`).
6. `FUN_001d5718`: progress flags (`FUN_001a5328` reads, `FUN_001a52b0` sets the bit array at 0x33fdb0): 0x6a once flag
   0x65 is set and forms 1..3 are all at level 5 or more; 0x67 once 0x65 is set and the summon level (`DAT_00340c88`
   byte 6) is above 4; 0x68 once the eleven ids of 0x37c5d8 all pass `FUN_001802e0` (bit array 0x33f6b0), forms 1..4 are
   all at 7 and the summon level is 7.

After the loop (any level-up) `FUN_001d3f88` calls `FUN_001b5528(player)`: system effect 0x10 bound to the player
(`FUN_001db868(.., 0x10, 1, 1, 0x2715, player)`).

Port: `prize::form_level_up`, `growth_upgrade`, `GrowthChains` (read from the executable), `form_exp` and
`apply_form_exp` (tests `form_level_ups_upgrade_the_growth_chain`, `master_form_experience_levels_up_to_the_cap`):
the level, the experience and the ability through `Progress`, the two obtained log records, the popups' content logged
(`prize: form level-up popups (not drawn): Master Form LV2: [msg 0xc3f2, ..]; Sora: [..]`), the effect 0x10 through
`fx::StartBoundSystemEffect`, and, while the player is in the form, `drive::refresh_form_list` rebuilds the form list at
the new level, which recomputes the ability block (test `drive::tests::a_form_level_up_in_the_form_rebuilds_its_list`;
log `(level-up effect 0x10, form list rebuilt [..])`). The status rebuild beyond the ability block is [inferred] to be
what `Abilities::set_form` recomputes. Not ported: the two record lists themselves (`Progress` has no setter for an
ability slot; the form list in use is derived from the level, which gives the same growth id), the popups' draw, the
progress flags of `FUN_001d5718` (no flag setter in `Progress`).

**Valor, Wisdom and Final experience.** `FUN_001d4078(victim, hit)` is called from the enemies' hit method at 0x1849d8
(no decompiled file; read in the disassembly: it adds the hit's atkp byte +0x2a to the victim's +0xaf0, calls
`FUN_001d4078`, then jumps to the receive function `FUN_001b21f0`; the method sits in the vtables at 0x35ce24,
0x35ed54, 0x35eddc, 0x35ef0c, 0x35f5e4): when the hit's attacker (hit +0x1c, then +8) is the player in Valor Form, Valor
gets 1. `FUN_001d40e0(id, player)` is called from the defeat method `FUN_00184a30` when the defeating object (+0xae4) is
the player: in Wisdom Form an object id (`FUN_001699a8`) in 1000..=2011, in Final Form one in 3000..=4008, gives 1.
Port: the rules `prize::valor_hit_exp`, `prize::defeat_exp` (test `form_experience_from_hits_and_defeats`) and the
message `prize::FormExpGain { form, amount }`, applied by `run_prizes` while the player is in that form (log
`prize: form experience +1 (Valor Form experience ..)`). The senders belong to the hit and defeat code (`enemy.rs`): not
wired (outside this package's files).

## Dropped items (verified, ported in WP-7Bb)

`FUN_001bd490(item, point)`: the item row (`FUN_001a66e8`) byte +0x16 (`Icon1`) picks a row of the box table 0x37bcb0
(12 rows of 8 bytes: the `00objentry` object and a scale; objects 0x3b8..0x3bb are `PRIZE_BOX_YELLOW`, `_RED`, `_BLUE`,
`_GREEN`, scales 1.6, 2.3, 3.2 each); an object of 0x9f0 bytes is made (`FUN_00104fa8`, `FUN_0016e208`) and set up by
`FUN_001bd0d0`: one generator step for its yaw (`-pi + u * 2 pi`, 0x37bc5c/54/58, given to the constructor `FUN_00168d30`),
the item id at +0x9b4, the scale at +0x3c and +0x9b8, +0x588 bit 0x40000 (an item object), +0x108 bit 1 (not takeable), the
class 0x35f2e8, then `FUN_0016b698(obj, 0xc)`. That runs state 12's enter at once (the state objects are global,
`FUN_00179868` reads 0x35ca48; `FUN_00179880` gives state 12 the vtable 0x35f2b8 and state 13 the vtable 0x35f288):

- **State 12 (fall)**, enter `FUN_001bd580`: the generic `FUN_0017b328` and three generator steps: X = -5 + u * 10
  (0x37bc74, 0x37bc70), Z the same, Y = -15 + u * 5 (0x37bc7c, 0x37bc78; Y down: upward), written X, Z, Y. Each frame
  the generic state code (`FUN_0017b440`, not ported) asks slot 7 `FUN_001bd700` for the step: Y velocity above 0 clears
  +0x108 bit 1 (the item can now be taken), Y + 1.5 (0x37bc84) held at 25 (0x37bc80), X and Z times 0.97 (0x37bc88), the
  step is the velocity times the frame unit. On landing slot 5 `FUN_001bd7a0`: faster than 2.5 (0x37bc8c) the Y velocity
  is multiplied by -0.8 (0x37bc90) and sound 0xe plays at the item for the first two bounces (+0x9e4 counts them); slower,
  +0x9e0 bit 0 (rested) and the velocity is cleared. [inferred: the app's sweep, as for the prizes, stands in for the
  generic mover.]
- **Pickup** (vtable slot 5 `FUN_001bd358`, decompiled with Ghidra here; [inferred] run once per frame): the player
  (`DAT_0035c998`) exists (`FUN_0019a3f8`) and its +0x588 bit 0x2000000 is clear, the item is not taken (+0x9e0 bit 2) and
  `FUN_0019a4b0` lets it (+0x108 bit 1 clear, +0x7f8 bit 0x1000 clear, +0x828 record above 0), and `FUN_001a3700(0, 0,
  player, position)` (the prizes' reach: 120 plus Draw horizontally, 30 vertically). Out of reach it is taken only with
  the instant-collection flags (`FUN_001905e8`, `FUN_00190610`), or, for an object 0x883 (`FUN_001bdb10`), by any party
  member (`FUN_00184ac8`). Then (the same tail as `FUN_001bd1d8`): `FUN_001a1768(item, NeoStatus of the player = 1, 1)`
  finds the room, `FUN_001a18d8` gives it and `FUN_001d0828` logs it; with no room nothing is given and the item is drawn
  in all the same (object 0x883 instead counts +0xacc on the member and plays `FUN_00197b70(0x51, 1)` for the player).
  +0x9e0 bit 2; +0x9d0 = position minus the member's centre (`FUN_0016b7d0`); frames = `|offset| * 0.05` (0x37bc60) held
  in 30..60 (0x37bc64, 0x37bc68) at +0x9bc and +0x9c0; state 13.
- **State 13 (draw-in)**, enter `FUN_001bd848`: +0x108 bits 0 and 1, sound 0x12 at the item. Update `FUN_001bd878`: the
  frames drop by the frame unit; with the member gone or the frames out, `FUN_001a1f40(item, -1)` builds the item's
  message (byte 9, the item's +0x17, its name `FUN_00190c40(+8)`) and `FUN_00138498` queues it (16 entries of 0x1d0 bytes
  at `DAT_00354ecc` +0x2a7c), then the object ends (vtable +0x20). Otherwise `g = sin((1 - left / total) * pi * 0.5)`
  (`FUN_00300278` is sine; 0x37bc94, 0x37bc98, 0x37bc9c), `f = 1 - g`: the position is the member's centre plus the
  offset times `f` raised by `sin(f * pi) * 50` (0x37bca0); the yaw target (`FUN_0016a7f0`, +0x66c) grows by
  `g * 0.5236` (0x37bca4) [inferred: the object turns to it at once]; below `f` 0.1 (0x37bca8) the scale is the start
  scale times `f * 10` (0x37bcac).

Port: `prize::ItemConsts`, `item_launch`, `item_fall`, `item_take`, `item_absorb_step`, `give_item`, the systems
`spawn_items`, `run_items`, `show_items` (the box model drawn with `assets::spawn_model`); tests
`an_item_launch_takes_yaw_then_x_z_y`, `an_item_bounces_then_rests`, `a_taken_item_arcs_into_the_collector`,
`real_item_objects` (box rows, the four models, a consumable given to the 99 cap then refused), and
`a_drop_follows_fun_001a3cb0` for the four generator steps. Since WP-7Bc the item message and the obtained log (below).
Not ported: the instant-collection flags, object 0x883, party members as collectors.

## Item messages and the obtained log (WP-7Bc)

**The message.** `FUN_001a1f40(item, -1)` fills the buffer 0x1c954b8 through `FUN_001a1ee0`: byte 0 = 9 (the text
system's icon code), byte 1 = the item's +0x17 (`Icon2`; a non-negative second argument replaces it), then the item's
name message (`FUN_00190c40(+8)` copied by `FUN_00191870`). `FUN_00138498` queues it in the manager `DAT_00354ecc`
(made by `FUN_00138240`, 0x45d0 bytes): 16 entries of 0x1d0 bytes from +0x28cc (the text at entry +0x1b0, so slot `w`'s
text is +0x2a7c + 0x1d0 w), write index +0x28c4, read index +0x28c8; it copies the text in only while `(w - r) mod 16`
is below 15, else the message is dropped. The manager's update `FUN_00137de0` (vtable 0x354ed8 slot 1) starts the
entry at the read index (`FUN_001379c0` -> `FUN_00137648(0, entry, 0x4c, text, -1)` -> `FUN_00138fc8`: the sequence
`FUN_0013a998(1)` = `gaug`, group 0x4c, the text texture `FUN_0013d6f8`, colour 0x80e5e5e5 at +0x198, font 0 at +0x1a0,
size 0x10 at +0x19c, flags 4, 8, 0x10 at +0xc); when the entry has left the draw list (`FUN_00139fe0` no longer finds
it) the read index moves on and the next entry starts. The entry (vtable 0x354f18) plays its group (`FUN_001376e0`: with
its delay +0x1a8 at 0 it plays system sound 10, `FUN_002c14a8(10, 0)`, once) and draws the text (`FUN_00139048` ->
`FUN_001a80d8`) at the player's place plus the group record's x, y (`FUN_00237618`, `FUN_00237638`) with the group's
colour alpha and its own colour; `FUN_00138aa8` hides it (vtable +0x14 = `FUN_00139778`) once a group that does not loop
forever (record +4 non-zero, `FUN_002369e0`) has played (`FUN_002369c0`). The other queue of the manager (+0x30, 8
entries of 0x510) uses group 0x4f with a delay of 60 (0x378d24); it is not the item message.

Port: `prize::ItemObtained { item, icon, name }` is sent when a taken item's draw-in ends (`run_items`, log
`prize: item 0x.. reached Sora: its message (icon N, name 0x..) goes to the queue`); `hud_banner::ItemMessages` keeps
the queue (test `the_item_message_queue_takes_fifteen`) and shows the front entry: a `gaug` node playing group 0x4c with
the label `[icon, name]` (font 0, size 16, colour 0xe5e5e5) at the group's x, y; it goes when the group has played (log
`hud: item message 0x.. shown: ..`). [inferred] The group's alpha is not applied to the label (taken as 0x80); the text
width limit (`FUN_001a8cc0` with the group record's +0x1c) is not applied; system sound 10 is logged, not played (the
system sound list is not wired, as in `items.rs`).

**The obtained log.** `FUN_001d0828(item, to, source)` -> `FUN_001d0760(0x341518, ..)`: the word at 0x341518 is the
index, 32 records of 6 bytes follow; `FUN_001d0688` writes the record at the index and the index moves on modulo 32.
Record: +0, +1 the world and area bytes (0x33e830, 0x33e831), +2 the item id (15 bits) with bit 15 set for a key item
(`Flag0` bit 0) or an item whose "obtained once" bit (0x3414f0 by its `Slot`) is clear, +4 `to`, +5 `source`. Callers:
`FUN_001bd1d8` (item objects, source 0, `to` = the room `FUN_001a1768` found, only when the gift went through),
`FUN_001b53d0` (3), `FUN_001be500` (4), `FUN_001d3da8` (5, `to` 1), `FUN_001d4fa0` (6), `FUN_001a6910`. 0x341518 is save
+0x16b8. Port: `prize::ObtainedLog` (a resource beside `Progress`, which has no field for it), `obtained_record`,
`place_bytes` (test `the_obtained_log_is_a_ring_of_32`); written by the item objects and the form level-ups. No reader of
the log was searched for.

## Collect-all (`FUN_002eece8`, host 6.31)

`FUN_001a40b8` (mode 1 above) and `FUN_001bda98`: every object with +0x588 bit 0x40000 (the item objects) is handed to
the player by `FUN_001bd1d8`, which checks nothing (an item already drawn in is given again and starts again). Ported:
`run_prizes` and `run_items` read `prize::CollectAll`.

## Harness

`KH2_PRIZELOG=1` logs rest, re-fall and timeout; drops, draw-ins and pickups always log:
`prize: drop of row .. at ..: n objects, kinds [..]`, `prize: kind k drawn toward Sora from d away`,
`prize: picked up kind k: HP +.. MP +.. drive +.. munny +.. -> ...` (plus the glow, the charge, the gauge or form timer,
Master Form experience and munny notes). Items: `prize: item 0x.. dropped at ..`, `prize: item 0x.. taken by Sora: given to
..`, `prize: item 0x.. reached Sora`, and with `KH2_PRIZELOG` `prize: item 0x.. came to rest`. `KH2_PRIZETEST=<kind>,..[@<s>]`
launches kinds in front of Sora (`KH2_PRIZETEST_DIST` cm, `KH2_PRIZETEST_STILL=1` without the sideways launch);
`KH2_ITEMTEST=<id>,..[@<s>]` (hex with `0x`) drops item objects the same way; `KH2_COLLECTALL=<s>` sends one `CollectAll`.
All three are harness aids, not game behaviour. The collision of `tt00` has floors above the arena floor in places:
prizes and items come to rest in mid-air there (seen at y 150 to 188); use `tt02` or `tt04`.

## Decompile index

| address | what |
|---|---|
| 0x1a3cb0, 0x1a3ef0, 0x1a4240, 0x1a3b28 | drop, prizes per kind, count, one prize |
| 0x1a3f98, 0x1a34f8, 0x1a34b8, 0x1a3538, 0x1c4cc0, 0x1c01c0, 0x1bf120 | manager and pool construction |
| 0x1c4d78, 0x1c4eb0, 0x1bf560 | entry allocation and set-up |
| 0x1c4d50, 0x1bf2f8, 0x1bf6e8, 0x1bf750, 0x1bf848, 0x1bf4b8 | per-frame walk and draw buffer |
| 0x1c5288, 0x1c5248, 0x1c5060, 0x1c4fb0, 0x143840 | falling update, physics, sweep |
| 0x1c5388, 0x1c52a8, 0x1c5020 | rest, shrink/check, life |
| 0x1a3848, 0x1c57b8, 0x1a3700, 0x16b960, 0x16b7d0, 0x16b558 | pickup check |
| 0x1a3a88, 0x1a3ad8, 0x1c5808, 0x1c5568, 0x1c53a8, 0x11b4d8, 0x2fb628, 0x1c59b0 | draw-in |
| 0x1a39b0, 0x175c28, 0x1792f0, 0x176c98, 0x186368, 0x1a0180, 0x1788d8, 0x1a0290, 0x1a0358, 0x1ba040, 0x1b9550, 0x1d4148, 0x1d3f88, 0x14f2d8, 0x151970, 0x151c40, 0x23e620, 0x23e5e0 | pickup effects |
| 0x1b51e0, 0x1b52f8, 0x1c46a8 | the experience word (correction above) |
| 0x203378 | `pppRyjMegaBirthPrize` draw |
| 0x1c5b10, 0x1c8590, 0x1d54d8, 0x1d55d8, 0x1c0400, 0x1c5a30 | mini-game pools |
| 0x1bd490, 0x1bd0d0, 0x1bd580, 0x1bd700, 0x1bd7a0, 0x1bd358, 0x1bd1d8, 0x1bd848, 0x1bd878, 0x1bdb10, 0x1bda48, 0x1bd518, 0x1bcfc8, 0x179868, 0x179880, 0x16b698, 0x16b648, 0x17b328, 0x17b440, 0x17b410, 0x17bad0, 0x19a4b0, 0x1699a8, 0x169958, 0x16a890, 0x16a7f0, 0x300278, 0x1a1f40, 0x1a1ee0, 0x138498, 0x1d0828, 0x1d0760, 0x2c32b8, 0x2c3290, 0x2c2380 | item object |
| 0x2eece8, 0x1a40b8, 0x1a3ad8, 0x1c5808, 0x1c5748, 0x1c5588, 0x1bda98 | collect-all (host 6.31, table 0x371078 entry 31 at 0x370f58) |
| 0x175c28, 0x175cb0, 0x174b68, 0x1db868, 0x1a21d0 | pickup glow |
| 0x1d4148, 0x1d3f88, 0x1b9bd8, 0x187f60, 0x1d3c38, 0x1d3bd0, 0x1d3b90, 0x1d3da8, 0x1d3f18, 0x1b5528 | Master Form experience |
| 0x175c28, 0x175cb0, 0x1d9f48, 0x1da0e8, 0x1e0298, 0x1da028, 0x1da080, 0x1db5f0, 0x1dc138, 0x1dd0d0 | glow start and stop (WP-7Bc) |
| 0x1d3f18 (disassembly 0x1d3f18..0x1d3f80), 0x37c556, 0x1b5578, 0x1b5588, 0x1b55f8, 0x1b5788, 0x1b5868, 0x37ba90, 0x152ed0, 0x19fb98, 0x1d5718, 0x1d5740, 0x1d57c8, 0x1d5820, 0x1a5328, 0x1a52b0, 0x1802e0, 0x37c5d8 | form level-up side effects |
| 0x1d4078, 0x1d40e0, 0x1849d8 (disassembly), 0x184968, 0x184a30, 0x1acfe0 | Valor, Wisdom, Final experience |
| 0x1a1f40, 0x1a1ee0, 0x138498, 0x138240, 0x137da8, 0x1383c8, 0x137de0, 0x1379c0, 0x137770, 0x137648, 0x138fc8, 0x138948, 0x138a60, 0x1389b0, 0x139430, 0x1394e8, 0x139468, 0x1376e0, 0x139328, 0x138aa8, 0x138bf0, 0x139048, 0x138f38, 0x139fe0, 0x139778, 0x1397d8, 0x237618, 0x237638, 0x237658, 0x237678, 0x2369c0, 0x2369e0, 0x2375c0, 0x13a998, 0x13d6f8, 0x2c14a8 | item message queue and entry |
| 0x1d0828, 0x1d0760, 0x1d0688, 0x341518, 0x3414f0 | obtained log |

## Gaps

- The mover `FUN_00143840` is not ported: the app sweeps over its own collision (`World::slide` sideways as a
  ball of the kind's radius, `World::ground` downward) and keeps only the end point and the floor contact.
- The drop point is the dropping enemy's lock-on point; `FUN_0016b738`'s line test from the raised position
  (+0x540, +0x5fc) is not ported.
- +0x588 bit 4 (Draw applies) is taken as set for Sora; status +0x1c4 in the reach is taken as 0 (writers not
  searched); the flags 0x35d268 (0x200, 0x400) and `FUN_00190610` / `FUN_001905e8` (instant collection, flag 8)
  are taken as clear.
- The middle arguments of `FUN_001db868` for the glow (0, 1, 0x2714) and the level-up effect (1, 1, 0x2715) are not
  modelled (`fx` ignores them for every system effect).
- Party members as collectors (the app has none). The level-up popups' draw (`DAT_003587e8`, `FUN_00152ed0`), the
  record-list writes of `FUN_001d3f18` and the progress flags of `FUN_001d5718` (no `Progress` setters); the senders of
  `FormExpGain` (Valor hits, Wisdom / Final defeats) in the hit and defeat code. The obtained log's "obtained once" bit is
  inferred from the call order (`Progress` does not expose the bits). The item message's alpha, width limit and sound.
- The item object's generic state code (`FUN_0017b440` and the mover) is replaced by the prize sweep; the yaw sign
  against the app's mirrored Z is [inferred].
- Where and how long the munny strip shows: it uses the sequence's own placement and goes when its group ends
  [inferred]; the popup registry `FUN_0014f8b0` was not followed.
- The sprite's colour combination (sprite colour times the kind's colour, 0x80 = 1.0) and the camera-facing
  orientation are inferred; the VU1 draw was not read. Nobody compared the look with the original.
