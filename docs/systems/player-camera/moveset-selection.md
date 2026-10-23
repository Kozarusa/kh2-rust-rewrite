# Sora's attack selection (ground combo, air chain, gated actions)

Work package WP-2G. Everything below was read from the main executable's own code (function
addresses in SLUS_210.05, file offset = VA - 0xff000) and from the `ptya` table of `00battle.bin`
as it sits on the user's disc. Code: `crates/kh2-anim/src/moveset.rs` (pure rules, unit tests) and
`crates/kh2-player/src/sora.rs` (the controller that feeds it). Set `KH2_MOVELOG=1` to log one block
per attack press: the press result, every record of the table with the reason it was or was not
flagged, and the chosen motion.

## Where the table comes from

`ptya` (table 1 for P_EX100, 30 records of 0x44 bytes; layout in `moveset.rs`). `FUN_00175ff8`
returns the table pointer (`actor + 0xaf0`) and looks a record up by key through `FUN_001d5ec0`
(the first record whose byte 0 equals the key).

## The attack press (FUN_00174cf8, command type 1)

The party controller keeps a pending command (`actor + 0xb44`, run by `FUN_001778e8` once a tick).
`FUN_00174cf8` runs it. For the attack command (cmd table row 1, Execute 1) the order is:

1. `FUN_001758b0`: Aerial Recovery (jump flag) or key 40 (A344, Retaliating Slash, ability bit 101,
   Square flag) while blown; see "WP-2G2" below.
2. When the command has flag bit 3 (the SQUARE button, see "Command flags" below): `FUN_00177ea0`
   (vtable +0x80); when it starts something the command ends there. It does nothing while the mover
   has flag 0x800. If the actor is in an idle range (grounded, airborne or blend-to-idle: motion
   flag word bits 4, 8, 0x10) it first restarts the combo counters (`FUN_00176a70`: combo 1,
   finishers 1). Then, in this order (verified, the `&&` chain of FUN_00177ea0):
   * `FUN_001783d8`: on the ground, only when actor +0xb50 is 6, no rider and status +0x14 bit
     0x40, with stick input: mover 0x16. Not identified, not ported.
   * `FUN_00178290`: Glide (growth slot 3 of `FUN_001c13b8`, airborne, idle range, height over the
     floor above the `fmab` row's +0x34): mover 0x11 / 0x12.
   * the explicit actions, each through `FUN_00177fa8(key)`: key 0x25 (37, A316 Finishing Leap,
     bit 95), key 0xc (12, A310 Upper Slash, bit 18), key 0x26 (38, A342 Horizontal Slash, bit 99);
   * `FUN_00178108`: Quick Run (growth slot 1, on the ground, idle range, stick input): mover 0x10;
   * `FUN_00178080`: the guard, key 0xb (record available and an idle range).
   Growth slot 0 is High Jump (`FUN_0016bd98`); slots 1 and 3 are named from what the functions do
   [inferred: the app's `Growth` order High Jump, Quick Run, Aerial Dodge, Glide].
   `FUN_00177fa8` accepts a key when the record passes the availability test (below), a normal
   attack only while no finisher is possible, and the playing motion has an allow-combo window open
   (motion flag bit 0x400) or a finisher window (actor +0x168 bit 0x20). It then runs the counter
   update of step 3 and starts the action (`FUN_00176050`). So from idle the explicit actions never
   start (no window): they follow an attack whose combo window is open.
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

### Command flags (verified)

`FUN_001babf0` builds the command's flag word (+8) from the pad's newly-pressed word (`FUN_001670d0`
keeps current / pressed / released at +0, +8, +0x10): logical bit 0x100 sets flag 2, logical bit
0x200 sets flag 8. `FUN_00167310` reads the physical pad and adds 0x1000000 for one of Cross (0x4000)
/ Circle (0x2000) and 0x2000000 for the other, swapped by `FUN_00105cd8` (a setting switch, not
traced); the shift-4 review traced 0x2000000 to logical 0x100 through the field table at 0x379768 and
Square (0x8000) to logical 0x200 (not re-read here). Flag 2 is the jump (verified): `FUN_00174cf8` calls the mover's method
+0x20 for it (for the vtable at 0x35cbb0 set by FUN_00179880: FUN_0017ae50 -> FUN_0017ade0 ->
FUN_0016bd98, the jump height). So:
* the attack button (the command menu's Attack, command type 1) does NOT carry flag 8: an attack
  press runs only steps 3 and 4;
* flag 8 is Square: the explicit actions, Glide, Quick Run and the guard;
* flag 2 is jump: Aerial Recovery while blown.

App controls: J = attack, Space = jump, K or E = Square. The Square press is tried on its own tick
(from the newly-pressed word) and before a pending attack. Deviation kept from the earlier port: the
attack path restarts the counters in an idle range (FUN_00177ea0's restart), so the ground combo and
air chain stay as they were; the game's attack path calls FUN_00176b00 alone. It only differs when an
idle range and the allow-combo range are open together with a landed hit.

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
4. On the ground, when the ledge probe in front is set (`FUN_0019a8e8`: no floor at the end of a
   sweep of the body radius plus `plyr` AttackJFront, see "Ledge probe" below) the record with key 1
   is marked.
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

* `FUN_001783d8` (the Square action with mover 0x16, actor +0xb50 = 6) is not identified or ported.
  Glide and Quick Run are the app's own states (WP-2E); they are reached from Square on the ground
  and in the air, not from an idle range of an attack motion.
* Whether a refused Square command survives its tick (the app tries it on the press tick only).
* The ledge probe's sweep end when a wall is in the way, the sweep's start-overlap behaviour and
  polygon handler index, the `FUN_001654f8` branch of FUN_00143840 (it skips the floor test), and
  the source of actor +0x118 (see WP-2G2).
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

## The keyblade at A343 frame 18 (closed by WP-6A: not a bug)

Cause (WP-6A): the blade is hidden behind Sora's own body. From motion frame 18 to 20 it points
forward and down from his hip (in front of him), and the run's camera looks from behind him. Evidence:
- `kh2 rig` against the game's own evaluator (kh2-ee): Sora's slot 776 at frames 17 and 19 (max error
  0.008 over 228 bones) and the keyblade's `W_EX010.mset` slot 776 at frames 16 to 22 (max error 0.002
  over 12 bones). So both poses are the original's.
- Test `skel::tests::a343_keyblade_is_whole_and_in_front_of_sora`: the blade mesh (mesh 0, 518 vertices,
  bones 2 and 11) skinned on the CPU as the GPU does it, on hand bone 178: whole at frames 16 to 22 (extent
  119 to 133 units), never below the floor, and at frame 19 the hand is at (28.7, 79.2, 55.4) with the
  blade from z 22 to 126 (Sora faces +z).
- Screenshots looked at (WP-6A, `KH2_VIEW_SLOT=776 KH2_VIEW_AFTER=4 KH2_FXONLY=9999 KH2_NO_ENEMIES=1
  KH2_SHOT_AT=4.63`, frame 19 confirmed by a per-tick log): from the default camera (behind) no blade;
  with Sora turned 90 and 180 degrees (a temporary viewer override of his facing, not committed) the whole
  keyblade is drawn, pointing forward and down. `KH2_YAW` did not turn the camera in these viewer runs
  (yaw 1.57 and 3.14 gave identical images), which is why WP-5A's "six camera yaws" all showed the back
  view. Nothing to fix; whether the original's camera shows this frame from behind is the camera's
  business, and A343 is an air finisher (Sora is airborne in play, not on the floor as in the viewer).

The earlier notes, kept for the record (WP-5A, see "A343 keyblade" at the end for the
runs). Frame numbers here are the motion's own 30 fps frames; the trigger frames below are global
(60 Hz) frames, twice as large. What was checked earlier: the keyblade's own motion for the A343
slot (776) is non-degenerate at every frame; Sora's right hand bone (178) is finite with determinant
1 at every frame (it moves about 140 units in two motion frames around frame 16 to 18, a fast
swing); the weapon motion carries no hide trigger (frame triggers are sound id 8 at global 18, 32,
50, 62, 78; range 12, the swing trail, global 17 to 90; range 10, the attack volume atkp 916 group 6,
global 31 to 40); Sora's body motion starts effect 35 on the caster at global frame 18 (range 3, no
gravity, 16 to 100); effect 35 is billboard quads only (no model), so it cannot replace the blade.

## Checks

Unit tests (`cargo test -p kh2-player sora`): the pure rules against real rows of the user's `ptya`
(skipped without `KH2_ISO`): ground hits 1 to 3, A301 vs A302, A303 vs A304, air chain, A333 vs
A332, the gated records and the explicit actions, the ledge probe on synthetic floors (no ISO
needed), and the press counters. `cargo test -p kh2-anim`: the blown press (jump / Square).

Scripted runs with `KH2_MOVELOG=1` in tt00 against Shadows (hold W with `KH2_YAW=1.57`, Q, then J
every 0.3 s). These were recorded before WP-5A; the scripted timing depends on where Sora starts
(at the tt00 start he faces a wall, the first swing may bounce off it):
* ground: A301 (an enemy within 180 sets key 4 first), A300 (A301 is playing), then the finisher
  A304 (key 3; no other enemy near enough for key 2): combo counters 1, 2, 3, the third press a
  finisher, and the Shadow dies from it;
* air (Space, then J): A330, A331 through key 9's alternative, A333 (key 7, another enemy inside its
  window) with a landed hit each time; with no hit landed the press is refused in the combo window;
* `KH2_ABILITIES=18,95` on the ground: the explicit actions come from Square (K), pressed inside the
  combo window of a swing that landed: `KH2_KEYS=2:F3,3:S:1.6,4.7:Q,5:J,5.12:K,5.2:K,...` (K every
  0.08 s after each J) gives A300 (key 5, J), then "Continue (combo 2, finishers 1): chosen key 12
  motion 161 (A310) by explicit key (Square)". J alone never starts an explicit action now;
* `KH2_ABILITIES=98,100,99` in the air: A341 (key 36, group 1), then A342 (explicit key 38, now from
  Square in the combo window); with `KH2_ABILITIES=100` key 29 (A343) is flagged on the third press
  but key 7 (A333) comes first in the table when another enemy is in its window.
* the ledge probe (key 1) in tt00's open area: "ledge probe clear" on every ground press of the
  combo (the floor is level, the lowest floor that counts is 24 below Sora).

## WP-2G2 / WP-5A: key 40, ledge probe, jump-height reach, hit point, A343

Addresses are SLUS_210.05 function addresses. Labels: verified = read in the code, inferred = follows
from it but a name or meaning is a guess.

### A344, key 40: a blown actor's press (FUN_001758b0), ported
It is not an attack-button rule. FUN_00174cf8 runs it first, for every pending command, and it only
acts while the actor's mover (`actor + 0xc`, word +8) has flag 0x40 and not 0x800 (verified; the movers
are the blow movers, modes 4, 8, 0x15, 0x21 of FUN_00179798's table; mode 0x2d carries 0x800 and allows
nothing). Gates, in order (verified): `actor + 0xf8 >= plyr + 0x58` (the record at actor +0x130 is the
`plyr` row, offset 0x58 is field 22, BlowRecoveryTime; `+0xf8` is the blow's tick counter, zeroed by
FUN_0019e468 at launch and +1 per tick by FUN_0019e4d8); `actor + 0x7f8` flag 4 clear (FUN_00186580
sets it with the dead flag; inferred: the actor is out of play). Then command flag 2 (jump) with ability
block bit 20 (`block & 0x100000`; ability item 158, Aerial Recovery, verified in the `item` table)
runs FUN_001759b8: motion 40 (blend 4) and the mover's launch method +8 with a vector built from
BlowRecoveryV / BlowRecoveryH (`plyr` +0x50/+0x54), not decoded. Otherwise command flag 8 (Square) runs
FUN_00176a70 (counters to 1/1, even if nothing starts), looks key 0x28 (40) up and, when it passes
the availability test, starts it with FUN_00176050. The record is A344, motion id 172, group 0,
ability bit 101 (items 523 and 273, type 19; USA `ptya`, checked by a test).

App: `Moveset::recovery(blow, jump, square, chain)` (pure, tested) and `sora::blown_press`, called
from the Blown state on a jump (Space) or Square (K/E) press; the attack button does nothing while
blown. Flag 2 is the jump and flag 8 Square ("Command flags" above). When both are pressed on one
tick Aerial Recovery wins, as in the game's order. Aerial Recovery's launch is a stand-in (the flight
ends and Sora falls). Nothing sets ability bit 20 or 101 unless `KH2_ABILITIES` does.

### Ledge probe, key 1 (FUN_001cbcf8 step 4, FUN_0019a8e8): ported (WP-5A)
It is a LEDGE probe, not a wall probe (the shift-4 port had it as a wall probe: wrong bit).
Verified: only on the ground by FUN_0019aa28, for an actor whose mover lacks flag 2 and whose
`+0x160` lacks flag 0x40, with `+0x810` zero. The vector is the reference direction (to the target,
else the facing) scaled by `actor + 0x5b0 + plyr[0x2c]` and re-projected into the ground plane by
FUN_00182e38 (two cross products with the ground normal at `+0x630`, length kept). `+0x5b0` is the
radius of the body shape (first shape of kind 0 or 4 in the model: FUN_00169af0 via FUN_00188e70 with
mask 0x11), `+0x5b4` its height field and `+0x5fc` its lift: Sora's are 90, 90 and 115 (the height
read from the game's run log, see below). `plyr[0x2c]` is field 11, AttackJFront, -5.0 in the USA
data. FUN_0019a8e8 stores the start (`+0x590` = position) and the vector (`+0x5a0`), clears bit 0 of
the 64-bit word at `+0x600`, runs FUN_00143840 and returns `+0x604 & 1`: bit 32 of that word.

FUN_00143840 (verified, Y down in the game):
1. clears bit 34 and sweeps (FUN_00164138: the sphere, start raised by the lift, stepped through
   FUN_00163f98 against the polygons of the swept box; the shift-4 review found the contact set in
   bit 34 by FUN_00163f98, not re-read here; in any case FUN_0019a8e8 does not return it);
2. unless FUN_001654f8 takes the other branch (not read; it sweeps again without a floor test),
   FUN_001423d8: a ray at the sweep's end point (`+0x5c0`) from `lift + clamp(|v.y|, 0, 100)` above it
   to `3 * lift` below it (DAT_00378f60 = 100.0, DAT_00378f64 = 3.0; the clamp applies when bits 3..6
   of `+0x600`, the mask index, are 0). No floor polygon: bit 33 set, floor height = 320000
   (DAT_00378f68), and FUN_00141910 with no polygon sets bit 32;
3. FUN_00141910 with a floor polygon (and `+0x600` bit 0 clear, as FUN_0019a8e8 leaves it): bit 32 is
   cleared when `hit.y <= start.y + max(0, v.y) + (lift - +0x5b4) + +0x610 - 1.0` (DAT_00378f50 = 1.0),
   set otherwise. `+0x610` is 0 here: FUN_00183e90 (called first by FUN_0019a8e8) sets it to 30 only
   when `+0x810` is non-zero, and the probe runs only with `+0x810` zero.
So bit 32 = no floor ahead, or a floor more than `lift - height - 1` (24 for Sora) below the start
(plus the downward travel on a slope). FUN_00182690 reads the same probe to stop movement at edges.
A set bit flags the key 1 record (motion 181, A330, without the air bit, so it is a ground action).

App (`sora::ledge_probe` / `ledge_at`, synthetic-world tests): the sweep is `motor::sweep_wall`
(PARTY_COLLIDE polygons of the wall class); the floor ray is `World::ground` (highest floor in the
ray's span). [inferred] The sweep's end point: where a wall ahead stops the sphere (contact distance
less the radius), else start + v. A contact not ahead of the centre is ignored (kept from the
shift-4 port; the game's start-overlap case, FUN_001632b8, was not read). The polygon filter of the
floor ray (FUN_00141b30 with 0xf, 4) is not applied. The probe runs when the selection is grounded
(the chain's airborne flag, FUN_0019aa28), not by the motor's floor contact.

Check, tt00, `KH2_MOVELOG=1` (WP-5A build): "ledge probe clear: sweep end (-605 0 -36), floor 0.0,
lowest floor that counts -24.0" at the start (facing the wall: the sweep stops at once, the floor is
there), and "ledge probe clear" on every ground press of the combo; key 5 (A300) is the first ground
attack again, as on the integration build. The shift-4 wall probe flagged key 1 at the start ("wall
probe HIT at (-647 115 0), key 1 flagged").

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
volume point, height clamped (rule in docs/systems/combat-effects/effects-damage.md). The first overlapped hurt shape is used
(FUN_001b5a58 returns the first matching entry).

The attacker point (WP-5A, verified layout): FUN_0018d988 walks the attack entries (header 0x10,
entries of 0x30: +0 the attacking object, +0x10 the attack volume record) and passes `entry + 0x10`
both to the overlap test FUN_00189208 (which dispatches the attack and hurt records alike on the
kind word at +0) and, through FUN_0018cf50 / FUN_0018d090, to FUN_0018ce18, which reads `+0x20` of
it: the volume record's centre at +0x10, the same field FUN_001894d0 fills for hurt records (bone
matrix x shape offset, raised by the lift). [inferred] The attack records are filled the same way
(their writer was not traced). App: `combat::live_attack_shapes` gives the attack shape's centre
raised by its lift, and `detect_hits` places `Hit.point` with `victim_point(hurt, centre)`. Tests:
`victim_point_matches_fun_0018ce18_worked_by_hand` (C (0,1,0), r 0.3, h 0.6, A (0.8,1.3,-0.6) ->
(0.2078461, 1.3, -0.1558846); clamped pole; equator). docs/systems/combat-effects/effects-damage.md "Hit point" still says
`Hit.point` is `vol.a` and suggests `(vol.a + vol.b) / 2` for the attacker; both are superseded here.

### A343 keyblade at frame ~18 (WP-5A): reproduced, not culling; cause found in WP-6A (occlusion, see above)
* Hand socket lag: rejected by code (shift 4). `Sim::Pose` (animation) runs before `Sim::Sora`
  (`follow_hand`) in the same tick and `sync_weapon` copies the body's frame before the pose step.
* Frustum culling: REJECTED by screenshot. Runs: `KH2_VIEW_SLOT=776 KH2_VIEW_AFTER=4 KH2_FXONLY=9999
  KH2_NO_ENEMIES=1`, `KH2_SHOT_AT` 4.33 to 4.73 (motion frames 10 to 22 at 30 fps, read from a
  temporary per-tick frame log). The keyblade is drawn at frames 10, 14, 16, 17 and 21, 22; at frames
  18, 19 and 20 the blade is missing. With `NoFrustumCulling` on every skinned mesh the frames 17 to
  21 look the same (blade missing at 18 to 20), so the change and the unused import were removed.
* Seen from six camera yaws (`KH2_YAW` 0 to 5.5) at frame 19: the guard and the chain are drawn at
  Sora's hand, the shaft is not, from every side.
* The weapon's bone matrices at frames 16 to 22 (logged): bones 0 to 2 are constant (determinant 1),
  bones 3 to 11 (the chain) move a little; nothing collapses.
* Not ruled out: the shaft hidden inside Sora's body or legs (the hand is at the hip at frame 19), and
  the renderer's skinning of the shaft's vertices (shader work). Nobody compared with the original game
  at these frames, so whether the original draws the shaft there is not known either. Kept as a gap.
