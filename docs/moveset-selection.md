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

1. `FUN_001758b0`: key 40 (A344, Retaliating Slash, ability bit 101); the blown press, see "WP-2G2" below.
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
record flags: bit 1 lowers `lo` by the actor's jump height (`FUN_0016be20`, see WP-2G2); bit 3 clear adds
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
the flag bit 1 widens it by the jump height), key 36 (A341, bit 98, group 1: any height within
650; group 2: another enemy 300 to 650 away), key 38 (A342, bit 99, explicit), key 29 (A343,
bit 100, group 3 finisher), key 40 (A344, bit 101, explicit key 40, not ported).

## Not ported or not resolved (GAPS)

* The request flag bit 3 of the attack command (inferred set, see above).
* `FUN_001783d8`, `FUN_00178290`, `FUN_00178108` (other abilities started by the attack button).
* The wall probe's start-overlap behaviour and polygon handler index, and the source of actor +0x118 (see WP-2G2).
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

## The keyblade at A343 frame 18 (open)

Not reproduced as a missing mesh, and no cause found. What was checked: the keyblade's own motion
for the A343 slot (776) is non-degenerate at every frame (unit scales, finite matrices, a small
swing of the blade bones); Sora's right hand bone (178) is finite with determinant 1 at every
frame (it moves about 140 units in two motion frames around frame 16 to 18, a fast swing); the
weapon motion carries no hide trigger (frame triggers are sound id 8 at 18, 32, 50, 62, 78; range
12, the swing trail, 17 to 90); Sora's body motion starts effect 35 on the caster at frame 18 (range
3, no gravity, 16 to 100); effect 35 is billboard quads only (no model), so it cannot replace the
blade. In the motion viewer (`KH2_VIEW_SLOT=776`) the blade is drawn in the frames sampled, edge-on
and thin when the hand is at the top of the swing, with and without effects (`KH2_FXONLY=9999`).
The remaining candidates are a hand socket lagging the fast hand movement or the skinned mesh's
rest-pose bounds, neither verified.

## Checks

Unit tests (`cargo test -p kh2-app sora`): the pure rules against real rows of the user's `ptya`
(skipped without `KH2_ISO`): ground hits 1 to 3, A301 vs A302, A303 vs A304, air chain, A333 vs
A332, the gated records and the explicit actions, and the press counters.

Scripted runs with `KH2_MOVELOG=1` in tt00 against Shadows (hold W with `KH2_YAW=1.57`, Q, then J
every 0.3 s):
* ground: A301 (an enemy within 180 sets key 4 first), A300 (A301 is playing), then the finisher
  A304 (key 3; no other enemy near enough for key 2): combo counters 1, 2, 3, the third press a
  finisher, and the Shadow dies from it;
* air (Space, then J): A330, A331 through key 9's alternative, A333 (key 7, another enemy inside its
  window) with a landed hit each time; with no hit landed the press is refused in the combo window;
* `KH2_ABILITIES=18,95` on the ground: A301, then A310 (Upper Slash, explicit key 12), then A316
  (Finishing Leap, explicit key 37 one attack early);
* `KH2_ABILITIES=98,100,99` in the air: A341 (key 36, group 1), then A342 (explicit key 38); with
  `KH2_ABILITIES=100` key 29 (A343) is flagged on the third press but key 7 (A333) comes first in the
  table when another enemy is in its window.

## WP-2G2: key 40, wall probe, jump-height reach, hit point, A343

Addresses are SLUS_210.05 function addresses. Labels: verified = read in the code, inferred = follows
from it but a name or meaning is a guess.

### A344, key 40: a blown actor's press (FUN_001758b0), ported
It is not an attack-button rule. FUN_00174cf8 runs it first, for every pending command, and it only
acts while the actor's mover (`actor + 0xc`, word +8) has flag 0x40 and not 0x800 (verified; the movers
are the blow movers, modes 4, 8, 0x15, 0x21 of FUN_00179798's table; mode 0x2d carries 0x800 and allows
nothing). Gates, in order (verified): `actor + 0xf8 >= plyr + 0x58` (the record at actor +0x130 is the
`plyr` row, offset 0x58 is field 22, BlowRecoveryTime; `+0xf8` is the blow's tick counter, zeroed by
FUN_0019e468 at launch and +1 per tick by FUN_0019e4d8); `actor + 0x7f8` flag 4 clear (FUN_00186580
sets it with the dead flag; inferred: the actor is out of play). Then command flag bit 1 with ability
block bit 20 (`block & 0x100000`; ability item 158, Aerial Recovery, verified in the `item` table)
runs FUN_001759b8: motion 40 (blend 4) and the mover's launch method +8 with a vector built from
BlowRecoveryV / BlowRecoveryH (`plyr` +0x50/+0x54), not decoded. Otherwise command flag bit 3 runs
FUN_00176a70 (counters to 1/1, even if nothing starts), looks key 0x28 (40) up and, when it passes
the availability test, starts it with FUN_00176050. The record is A344, motion id 172, group 0,
ability bit 101 (items 523 and 273, type 19; USA `ptya`, checked by a test).

App: `Moveset::recovery` (pure, tested) and `sora::blown_press`, called from the Blown state. The
mapping of the command flags to buttons is inferred: bit 3 (also the attack's request flag, see above)
is the attack button; the app has one button for both flags, so Aerial Recovery wins when its ability
bit is set. Aerial Recovery's launch is a stand-in (the flight ends and Sora falls). Nothing sets
ability bit 20 or 101 unless `KH2_ABILITIES` does.

### Wall probe, key 1 (FUN_001cbcf8 step 4, FUN_0019a8e8): ported with two approximations
Verified: only on the ground by FUN_0019aa28, for an actor whose mover lacks flag 2 and whose
`+0x160` lacks flag 0x40, with `+0x810` zero. The vector is the reference direction (to the target,
else the facing) scaled by `actor + 0x5b0 + plyr[0x2c]` and re-projected into the ground plane by
FUN_00182e38 (two cross products with the ground normal at `+0x630`, length kept). `+0x5b0` is the
radius of the body shape (first shape of kind 0 or 4 in the model: FUN_00169af0 via FUN_00188e70 with
mask 0x11): Sora's is a sphere of radius 90 lifted 115 (`+0x5fc`). `plyr[0x2c]` is field 11,
AttackJFront, -5.0 in the USA data. FUN_0019a8e8 stores the start (`+0x590` = position) and the
vector (`+0x5a0`), then FUN_00143840 -> FUN_00164138 sweeps that sphere (start raised by the lift,
stepped in radius-sized steps through FUN_00163f98) against the polygons of the swept box; the result
is bit 0 of `+0x600`. So: a sphere of radius 90 swept 85 units forward. A hit flags the key 1 record
(motion 181, A330, without the air bit, so it is a ground action).

Polygon filter (FUN_00141b50): `(attr & 0xf) != 4`, and FUN_0032e3b8 (table 0x378f7c, assembly) tests
the polygon word against a mask picked by bits 3..6 of `+0x600`: index 0 and 6 test 0x10
(PARTY_COLLIDE). Inferred: the handler's third argument is the polygon word and Sora's index is 0.
The app uses `kh2_map::flags::PARTY_COLLIDE` and the wall class (normal more than 60 degrees from
vertical), `motor::sweep_wall`.

Approximation (the app's): a contact that is not ahead of the sphere's centre (dot with the direction
not above 0) is dropped. Without it Sora, who stands 35 from a wall in tt00, touches the 90-radius
sphere sideways and takes key 1 on every press. What the game's sweep does with polygons it already
overlaps at the start was not read (FUN_001632b8 is 800 lines).

Check, tt00 start, `KH2_MOVELOG=1`: "wall probe missed" and keys 5, 4, 5, 2 for presses 1 to 4 (the
ground combo as before). `KH2_MOVELOG` now also logs position, yaw, jump height and the probe result.

### Weapon reach for record flag bit 1 (FUN_0016be20 = FUN_0016bd98): resolved
It is the actor's jump height, not a weapon value (verified): `actor + 0x118`, replaced when the object
is a party member (`+0x588` flag 4) whose mover lacks 0x800 by the first float of the `fmab` row for
the High Jump level (`FUN_001c13b8(status + 0x1cc, 0)` > 0; FUN_001cc1a0 indexes the table named at
0x37c288, "fmab"). So Sora with High Jump level n gets that row's HighJumpHeight; otherwise `+0x118`
(inferred to be `prty` JumpHeight, 185 in the USA data; its writer was not found). The window
subtracts it from the lower bound (FUN_0019f090) and FUN_00176050 adds it to the action's leap range
(record +0x10). App: `Scene::jump_height`. The earlier wording "weapon reach" was wrong. The action's
own leap (FUN_00176050 for record +0xc > 0) is not ported (the app's lunge uses `plyr` V0).

### Hit.point (FUN_0018ce18): ported
`combat::victim_point`: the point of the victim's spheroid (centre = bone matrix x offset raised by
the shape's lift; radius; the shape's height field as half height, FUN_001894d0) toward the attacker's
volume point, height clamped (rule in docs/effects-damage.md). The first overlapped hurt shape is used
(FUN_001b5a58 returns the first matching entry). Deviation: the attacker point is the attack volume's
base point (`vol.a`, the offset without a lift), not the shape centre the game keeps at record +0x20.

### A343 keyblade at frame ~18: candidates tested
* Hand socket lag: rejected by code. `Sim::Pose` (animation) runs before `Sim::Sora` (`follow_hand`) in
  the same tick and `sync_weapon` copies the body's frame before the pose step, so socket and weapon
  pose come from the same frame; there is no interpolation to lag.
* Rest-pose bounds: skinned meshes were left to Bevy's frustum culling, which uses the mesh's rest-pose
  box. `skel::spawn_skinned` now adds `NoFrustumCulling`. Not confirmed by eye (no run showed the
  missing blade, nobody looked). If the blade still vanishes the next candidate is the renderer's skin
  transform, i.e. shader work.
