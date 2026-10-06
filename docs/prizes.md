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
- Mode 1 (`FUN_001a40b8` -> `FUN_001a3ad8`: every pickable prize at once, 80 frames, 0x37bf80) is not ported;
  its caller was not traced.

## The pickup (`FUN_001a39b0`, verified; table at 0x37b540, 8 bytes per kind)

| byte | effect | port |
|---|---|---|
| +0 float | above 0: `FUN_00175c28` starts system effect 6 of 0x1c954d8 on the player for that many frames (+0xae4/+0xae8) | **gap**: no owner-bound system effect with an end in `fx` |
| +4 | HP: the player's method +0x78 (`FUN_001792f0` -> `FUN_00176c98` -> `FUN_00186238`, HP + amount held at the maximum) | done |
| +5 | MP: `FUN_00186368` -> `FUN_001a0180`: during the MP charge the charge time drops by amount * `sstm[0x7c]` (`FUN_001a0540`), else MP + amount held at the maximum | done (the charge end comes on `magic`'s next tick [inferred one tick late]) |
| +6 | drive: `FUN_001788d8(units)`: gauge mode adds `(int)(units * [sstm[0x78] * block +0x38 while charging] * FUN_001ba040())` to the gauge (`FUN_001a0290`); form mode adds `units * sstm[0x70]` to the form timer (`FUN_001a0358`); `FUN_001d4148` gives Master Form (+0xb50 == 3) form experience (`FUN_001d3f88`) | gauge and timer done; `FUN_001ba040` is 1.0 (0x37bc04) outside the Colosseum manager `DAT_01d5dd20`; **gap**: Master Form experience |
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

## Dropped items (gap)

`FUN_001bd490(item, point)` makes an item object (0x9f0 bytes, class 0x35f2e8, object id and scale from
0x37bcb0 by the item's byte +0x16; `FUN_001bd0d0` takes one more generator step, ported in `compute_drop`).
The item object (its flight, pickup and inventory entry) is not ported: items are logged only.

## Harness

`KH2_PRIZELOG=1` logs rest, re-fall and timeout; drops, draw-ins and pickups always log:
`prize: drop of row .. at ..: n objects, kinds [..]`, `prize: kind k drawn toward Sora from d away`,
`prize: picked up kind k: HP +.. MP +.. drive +.. munny +.. -> ...`. `KH2_PRIZETEST=<kind>,..[@<s>]` launches
kinds in front of Sora (`KH2_PRIZETEST_DIST` cm, `KH2_PRIZETEST_STILL=1` without the sideways launch); a
harness aid, not game behaviour.

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
| 0x1bd490, 0x1bd0d0 | item object |

## Gaps

- The mover `FUN_00143840` is not ported: the app sweeps over its own collision (`World::slide` sideways as a
  ball of the kind's radius, `World::ground` downward) and keeps only the end point and the floor contact.
- The drop point is the dropping enemy's lock-on point; `FUN_0016b738`'s line test from the raised position
  (+0x540, +0x5fc) is not ported.
- +0x588 bit 4 (Draw applies) is taken as set for Sora; status +0x1c4 in the reach is taken as 0 (writers not
  searched); the flags 0x35d268 (0x200, 0x400) and `FUN_00190610` / `FUN_001905e8` (instant collection, flag 8)
  are taken as clear.
- The pickup glow (system effect 6), Master Form experience, item objects, the auto-collect mode 1, party members
  as collectors (the app has none) are not ported.
- Where and how long the munny strip shows: it uses the sequence's own placement and goes when its group ends
  [inferred]; the popup registry `FUN_0014f8b0` was not followed.
- The sprite's colour combination (sprite colour times the kind's colour, 0x80 = 1.0) and the camera-facing
  orientation are inferred; the VU1 draw was not read. Nobody compared the look with the original.
