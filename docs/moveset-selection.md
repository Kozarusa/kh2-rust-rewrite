# Sora's attack selection (ground combo, air chain, gated actions)

Work package WP-2G. Everything below was read from the main executable's own code (function
addresses in SLUS_210.05, file offset = VA - 0xff000) and from the `ptya` table of `00battle.bin`
as it sits on the user's disc. Code: `crates/kh2-anim/src/moveset.rs` (pure rules, unit tests) and
`crates/kh2-app/src/sora.rs` (the controller that feeds it). Set `KH2_MOVELOG=1` to log one block
per attack press: the press result, every record of the table with the reason it was or was not
flagged, and the chosen motion.

## Where the table comes from

`ptya` (table 1 for P_EX100, 30 records of 0x44 bytes; layout in `moveset.rs`). `FUN_00175ff8`
returns the table pointer (`actor + 0xaf0`) and looks a record up by key through `FUN_001d5ec0`
(the first record whose byte 0 equals the key).

## The attack press (FUN_00174cf8, command type 1)

The party controller keeps a pending command (`actor + 0xb44`, run by `FUN_001778e8` once a tick).
`FUN_00174cf8` runs it. For the attack command (cmd table row 1, Execute 1) the order is:

1. `FUN_001758b0`: key 40 (A344, Retaliating Slash, ability bit 101). Needs a request flag (bit 3)
   and a charge value (`actor + 0xf8` against `stat + 0x58`) that were not traced. NOT PORTED.
2. When the request has flag bit 3: `FUN_00177ea0` (vtable +0x80). If the actor is in an idle range
   (grounded, airborne or blend-to-idle: motion flag word bits 4, 8, 0x10) it first restarts the
   combo counters (`FUN_00176a70`: combo 1, finishers 1). Then the explicit actions, each through
   `FUN_00177fa8(key)`: key 0x25 (37, A316 Finishing Leap, bit 95), key 0xc (12, A310 Upper Slash,
   bit 18), key 0x26 (38, A342 Horizontal Slash, bit 99). Before them it tries two actions that
   come from other abilities (`FUN_001783d8`, `FUN_00178290`, status slots, not traced) and after
   them `FUN_00178108` and `FUN_00178080` (key 0xb, guard).
   `FUN_00177fa8` accepts a key when the record passes the availability test (below), a normal
   attack only while no finisher is possible, and the playing motion has an allow-combo window open
   (motion flag bit 0x400) or a finisher window (actor +0x168 bit 0x20). It then runs the counter
   update of step 3 and starts the action (`FUN_00176050`).
3. The counters, `FUN_00176b00(actor, hit_landed)`:
   * finisher window open, a hit landed, and finishers < the status block's finisher count:
     finishers + 1 (`FUN_00176ac0`), accepted;
   * otherwise, if there is no landed hit or no allow-combo window: accepted only when the actor is
     in an idle range, and then the counters restart (combo 1, finishers 1); otherwise refused;
   * otherwise (combo window and a landed hit): combo + 1 (`FUN_00176a80`, capped at 100).
   `hit_landed` is actor +0x108 bit 0x2000000. It is set by `FUN_0018c6d0` for every hit that lands
   and by the wall recoil (`FUN_00175668`), and cleared when an action starts (`FUN_00176050`).
   Consequence: a swing that hits nothing cannot be followed by the next combo hit; the press is
   refused while the motion plays and a new chain starts once an idle range opens.
4. `FUN_00177d58` (vtable +0x7c): when the playing motion is 0x2a (the guard-hit reaction) the
   action of key 0x27 (39, counter) is taken if available. Otherwise the table selection, below.
   The chosen record is started with `FUN_00176050`.

The app takes the attack button as carrying request flag bit 3. That is an INFERENCE: it is what
makes the three explicit actions reachable from the X button, and all three need abilities the
game does not give Sora at the start. Without the flag only the table selection (step 4) runs.

## The table selection (FUN_001cb928, FUN_001cbcf8, FUN_001cba68, FUN_001cbf48)

`FUN_001cb928` builds the context: the actor's target (actor +0xb40, the lock-on target or the
automatic target), the actor position (`actor + 0x540`, or `+0x70` for riders), and the reference
direction `ctx + 0x30`: the horizontal unit vector to the target point, or the actor's facing when
there is no target.

A target point is a collision shape of kind 3 (the BAR type 23 `ObjectCollision`, fields in
`kh2-model/src/collision.rs`). `FUN_001894d0` gives its position: the bone's world matrix times the
shape offset, moved up by the short at shape offset 0xe (`lift`), and its radius and half height
(shape offsets 0x10 and 0x12). Shadow's target point is bone 24 with radius 45 and half height 45;
Sora's own is on the root, lifted 70. Only shapes whose argument has bit 0 clear count
(`FUN_0018f348`).

`FUN_001cbcf8` then marks one bit per record:

1. `FUN_001cbc58` marks group 2 records. It walks every object that has the enemy-class flag
   (+0x588 bit 0x20000, set by the constructor `FUN_001d5c70`) and every target point of it, and
   calls `FUN_001cba68` with group 2.
2. With a target, `FUN_001cba68` for the target's point with group 1.
3. Every group 3 record that passes the availability test is marked.
4. On the ground, with a wall probe hit in front (`FUN_0019a8e8`, distance `actor + 0x5b0` plus
   the status radius; NOT PORTED, its inputs were not identified) the record with key 1 is marked.
5. `FUN_001cbf48` picks, and when nothing was picked the default attack (key 5 on the ground, key 9
   in the air) is marked, whatever its availability, and the pick runs again.

`FUN_001cba68(group)` for each record of that group: the record must pass the availability test
and the window test `FUN_0019f090` with
* `dist`: horizontal distance to the point less the point's radius, at least 0;
* `dy`: point height minus the actor's, Y down (a point above the actor is negative);
* `dot`: the horizontal unit direction to the point dotted with the reference direction, at most 1.

Window (`FUN_0019f090`): `min <= dist < max` (record 0x20, 0x24), `angle_min < dot <= angle_max`
(0x30, 0x34), and `lo < dy <= hi` where `hi` is record 0x28 and `lo` record 0x2c, adjusted by the
record flags: bit 1 lowers `lo` by the weapon reach (`FUN_0016be20`, see gaps); bit 3 clear adds
the point's half height to `hi`, set subtracts it; bit 4 clear subtracts it from `lo`, set adds it.

Group 1 records have the angle window (0.99996, 1]: only the point in the reference direction, that
is the target itself. Group 2 records have (-0.707, 0.99996] (or (0.966, ...] for the far ones):
points NOT in the target's direction, up to 135 degrees to either side, so group 2 means "another
enemy near me". The target's own point never passes a group 2 test.

### Availability (FUN_0019f1b0)

A record is available when: it has a motion; its ability bit is set in the actor's status block
(`status + 0x1cc`, bit 0 means no gate); its airborne-only flag equals the actor's airborne state
(`FUN_0019aa28`: the object class says airborne, or a no-gravity range (flag bit 0x20, 0x1000000)
is active); and a finisher record needs `FUN_00176908(arg)` (a finisher window is open, or
`max_hits <= combo + arg`) while a normal record needs NOT (a finisher is possible and
finishers > 1).

`max_hits` is status byte +0x28 on the ground and +0x29 in the air; both start at 3
(`FUN_001c0d88`), the finisher count (+0x2a) at 1. Combo Plus (ability id 162, bit 21) adds one on
the ground, Air Combo Plus (163, bit 22) one in the air, Finishing Plus (393, bit 105) one finisher,
Negative Combo (394, bit 106) takes one from both (`FUN_001c0f40`). The combo counter counts the
attack being chosen (the counter update runs before the selection), so with the base values the
third press is a finisher on the ground and in the air: two normal hits and a finisher.

### The pick (FUN_001cbf48)

Over the marked records in table order: return the first whose alternative key is its own or whose
motion differs from the playing motion (`actor + 0x154`); remember the first marked record's
alternative (the record with that key) as a fallback; return the fallback at the end.

## What this gives for Sora's table (keys; motion ids; tags are the motion names)

Ground combo (record order matters, as the pick takes the first marked):
* Press 1 from idle: key 5 (A300, id 151, group 3). Group 1 and 2 records of abilities Sora does not
  have are rejected.
* Press 2 (combo 2): key 6 (A302, id 153, group 1) when the target is 180 to 300 away and between
  roughly 20 and 110 above plus the half heights; key 4 (A301, id 152, group 2) when another enemy
  is within 180 and not more than 135 degrees off; otherwise key 5 is marked and, while A300 plays,
  its alternative (key 6) is returned: A302. While A301 plays and an enemy is close, A300 comes next
  (key 5 is the first marked record with another motion).
* Press 3 (combo 3, finisher ready): the finishers come first in the table: key 2 (A303, id 154,
  group 2, enemy within 180) else key 3 (A304, id 155, group 3, alternative key 2).
  Gated finishers: key 26 and key 42 (A315, Explosion, bit 94), key 27 (A314, Guard Break, bit 93).
* Ability-gated normal records: keys 35 (A311, bit 90, within 120), 44/34/45 (A312, bit 91), 28/43
  (A313, bit 92, 180 to 650), 12 (A310, explicit only), 37 (A316, explicit only), 39 (A317 counter).

Air chain:
* Press 1: key 9 (A330, id 181, group 3, alternative key 10), or key 10 (A331, id 182, group 1) when
  the target is at least 70 above and within 260.
* Press 2 while A330 plays: key 10 through the alternative: A331.
* Press 3 (finisher ready): key 7 (A333, id 184, group 2) when another enemy is within 260 and
  its height is between 40 (plus half height) and -140 (less half height); else key 29 (A343,
  bit 100) if set, else key 8 (A332, id 183, group 3). So A333 is chosen by an enemy being near,
  not by repeating the chain.

Range and height gated records: key 33 (A340, bit 97, grounded, target 110 to 250 above within 260;
the flag bit 1 widens it by the weapon reach), key 36 (A341, bit 98, group 1: any height within
650; group 2: another enemy 300 to 650 away), key 38 (A342, bit 99, explicit), key 29 (A343,
bit 100, group 3 finisher), key 40 (A344, bit 101, explicit key 40, not ported).

## Not ported or not resolved (GAPS)

* The request flag bit 3 of the attack command (inferred set, see above) and the whole of
  `FUN_001758b0` (key 40, A344): the charge test `actor + 0xf8 < stat + 0x58` and what fills it.
* `FUN_001783d8`, `FUN_00178290`, `FUN_00178108` (other abilities started by the attack button).
* The wall probe that marks key 1 (`FUN_001cbcf8`, `FUN_0019a8e8`, actor +0x5b0 and +0x810).
* The weapon reach `FUN_0016be20` adds for record flag bit 1 (keys 33 and 0): it is the first float
  of the equipped weapon's record (`FUN_001c13b8`, `FUN_001cc1a0`) or `actor + 0x118`. The app uses 0.
* The status flag 0x800 case of `FUN_001768b8` (makes finishers impossible).
* Who sets actor +0x168 bit 0x20 other than range trigger 50 was not looked for.
* How long the game keeps a pending attack command that was refused (the app keeps it until the
  motion ends and retries every tick).
* Target point choice: the game stores which of an object's target points is the target
  (`target + 4`); the app uses the first.
* Which enemies count as group 2 candidates beyond "alive": the game's per-point visibility mask
  (`FUN_0019a6a0`) and `FUN_0019a5d0` (not for a few object kinds, not while a state flag is set).
* The ability system is not ported: bits come from `KH2_ABILITIES` (and Guard). Combo Plus and the
  others are one bit each, the game counts equipped copies.

## Checks

Unit tests (`cargo test -p kh2-app moveset`, `-p kh2-anim`): the pure rules against real rows of
the user's `ptya` (skipped without `KH2_ISO`): ground hits 1 to 3, A301 vs A302, A303 vs A304, air
chain, A333 vs A332, the gated records and the explicit actions, and the press counters.
Log runs: `KH2_MOVELOG=1`, see the work package report for the scripted runs.
