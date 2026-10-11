# Sora's movement during air swings, and Square on the ground (WP-FIX-10)

Read from the main executable (SLUS_210.05; decompiled functions in the local study folder, nothing
copied) and from the `ptya` and `fmab` tables on the user's disc. Code: `crates/kh2-app/src/sora.rs`
(`leap_launch`, `AttackLeaps`, `Dash`, `dash_step`, the `Attack` and `QuickRun` states) and
`crates/kh2-app/src/motor.rs` (`Jump::falling_at`, `Motor::start_leap`, `Jump::release_cuts`).
Log: `KH2_MOVELOG=1` prints one `move: airborne` / `move: air swing` line per tick with the velocity
and the real displacement.

## 1. Air swings keep the momentum (user report 2026-10-06: "all velocity stops")

The app zeroed the velocity at every attack start and replaced it by the plyr V0 approach every
tick, and held Sora at a fixed height through every air attack. The original does neither.

### Who moves the actor
* `FUN_00181e00` (per tick): displacement = the mover's slot 7 result + the motion's root motion
  (`FUN_00182370`, actor +0x1f0 turned by the facing) + the secondary velocity (+0x880 x +0x88c) +
  further terms; after the collision step it stores the real displacement at +0x860.
* The jump mover (mover 1, vtable 0x35cbe0, installed by `FUN_0016b648` from the table that
  `FUN_00179880` fills at 0x1c88eb0): slot 7 = `FUN_0017b6a0` -> `FUN_0017b620`: the clock +0xd0
  advances every tick; horizontal `FUN_001832b8(0.9, 0.95)`: with stick input
  `v = 0.9 v + 0.1 stick`, without it `v = 0.95 v` (DAT_0037a508, DAT_0037a50c); vertical
  `FUN_0017b6e0`: the eased jump curve capped by +0x120, but 0 while actor +0x160 has bit 0x20
  (range trigger 3), 0x1000000 (trigger 30) or 0x40 (trigger 24) (`FUN_00188030` sets those bits).
* `FUN_001778e8` (party command tick): outside an idle range (+0x160 bits 4, 8, 0x10 = triggers 0, 1,
  2) and with +0x160 bit 31 clear, the stick vector +0x10 is zeroed. So during a swing the jump
  mover brakes the velocity by 0.95 a tick; it never stops it.

### What an attack start does (FUN_00176050)
* No velocity write for a record whose +0xc is 0 (unless the mover has flag 2 or trigger 24 is
  active, with a target: then +0x20 = direction to the target x clamp(gap x sstm[0xa8], +0x18, +0x1c);
  not ported, Sora's movers lack flag 2). On the ground the ground mover's velocity follows the
  zeroed stick, so ground swings stop Sora (unchanged in the app).
* A record with +0xc above 0 (a **leap**) launches the mover (method +8 = `FUN_001796a8` ->
  `FUN_0017b278` / `FUN_0017b1e0`, mover 1 re-installed, `FUN_0017b328` copies +0xc0 into the
  velocity): a new jump of height h with horizontal velocity dir x speed. On the ground the combo
  counters restart first (`FUN_00176a70`).
  * With a target (actor +0xb40): `FUN_00176400` gives the target's point less Sora's (centres of
    their target shapes, `FUN_0018eec0` / `FUN_001894d0` on `FUN_0016b558(actor, 3, -1)`) and the
    sum of their radii. h = the target's height above Sora, at least +0x14, at most +0x10 (plus the
    jump height of `FUN_0016be20` for record flag bit 1). speed = (horizontal distance - radii +
    record +0x3c) / sqrt(2h / g) (`FUN_0017bee0`), clamped to +0x18..+0x1c. [inferred] the distance
    is the VU0 Q register read after the square root (`cfc2`).
  * Without a target: h = +0xc, along the facing (`FUN_0016a890`, +0x550), speed +0x1c.
  The leap clears the release-watch bit (`FUN_0017b328` clears +0x108 bit 0x20000 because the
  launch did not set 0x10000), so the jump button cannot cut it short (`Jump::release_cuts`).
* `ptya` leap fields (P_EX100 table; `AttackLeaps` reads every table): the ground combo A300..A304
  has none; A330..A333, A340..A344 have one (e.g. key 9, A330: +0xc 21, +0x10 71, +0x14 21,
  +0x18 -8, +0x1c 8, +0x3c 85); the ledge record key 1 (A330 from the ground) has one too.

### Gravity held and given back (FUN_00169ee8, FUN_0016a2f0)
* `FUN_00169ee8` latches +0x108 bit 0x800000 while trigger 3 or 30 covers the frame, and bit
  0x1000000 on the first frame of trigger 24 (zeroing +0x20 only when the mover is not airborne).
* `FUN_0016a2f0`: when a latch is set and none of the three triggers is active, the jump mover is
  restarted as a fall: +0xd0 = (downward part of +0x864) / g, rise 0, base so the curve passes the
  current height, and the velocity +0x20 = +0x860 (the last real displacement, root motion
  included). App: `Sora::hover`, `Sora::last_move`, `Jump::falling_at`, for the Attack state and
  the free fall after it.
* A330, A331, A332, A333 carry no trigger 3, 24 or 30 (`cargo run -p kh2 -- mset obj/P_EX100.mset`):
  gravity runs through them after the leap; A343 has trigger 3 over frames 16..100.

### Landing in the middle of an air swing (WP-F1 item 10; user report 2026-10-10: a landing lag after air attacks)
The app let an air swing play to its end on the floor, so Sora stood through the rest of A330..A344 (up to
58 frames) before he could act. The original: the jump mover's vtable (0x35cbe0: +0 `FUN_0017b328` start,
+8 `FUN_001796a8` launch, +0x14 `FUN_0017bab8`, +0x18 `FUN_0017b440` tick, +0x1c `FUN_0017b6a0` displacement,
+0x20 `FUN_0017bad0` aerial dodge) answers the landing with `FUN_0017bab8` -> `FUN_0017b9e0(mover, actor, 0)`:
when the landing motion id +0x104 is not negative (`FUN_0017b1b0` sets it to 5 at every jump set-up) and bit 0x10
of the controller's third trigger word (actor +0x168) is clear, motion 5 (`A005`, the landing) starts
(`FUN_0017f390` checks it exists, `FUN_0017e220(0, 0, ctrl, 5)`) with motion 0 queued behind it
(`FUN_0017e378(0, 0, ctrl, 0, 0)`); then the mover is re-installed (`FUN_0016b698`) and +0x108 bit 0x8000000 is
set. The base-actor variant `FUN_00183150` does the same without the +0x104 test. The +0x168 bit 0x10 is range
trigger type 0x31 (`FUN_00188030` case 0x31 sets `ctrl +0x28 | 0x10`): "no landing motion". None of
A330..A333, A340..A344 carries trigger 49 (`cargo run -p kh2 -- mset obj/P_EX100.mset A3`: ranges 3, 11, 14,
21, 23, 28, 50 only), so a landing during any air swing starts the landing motion and the swing is gone.
Port: `sora::tick`, the landing block (`TRIGGER_NO_LANDING_MOTION_49`): a landing while `State::Attack` restarts
`A005` and enters `State::Ground` (combo counters reset, the queued attack dropped); with trigger 49 active
the swing is kept. [inferred] that the attack command ends with its motion: the command object's reaction to
the motion swap was not traced. Logs (`KH2_MOVELOG=1`): `move: landed during A331 f12: the landing motion
replaces the swing (FUN_0017b9e0: slot 5, then idle)`. Harness: tt00, `KH2_NO_ENEMIES=1 KH2_MOVELOG=1
KH2_KEYS=6:Space,6.4:J`: the jump, an air attack near the apex, the landing cuts it.
* The landing's own length is the game's: `A005` runs frames 42..55 at 30 fps with its idle range (trigger 2)
  from global frame 12, as the app's ground state already plays it after a jump.
* NOT REPRODUCED IN THE HARNESS (gap, outside this package's files): in every run tried (tt00 at the arena
  centre, tt02, tt04, keys `6:Space,6.2:J`, also a ground swing with `KH2_GROWTH=3,0,0,0`) the swing ended in
  `wall recoil: atkp 131 (BG Hit, keyblade volume) -> motion id 43 slot 172 A350` (ground: atkp 126 -> A320)
  before any landing, although no wall is within 800 units of tt00's spawn (its COCT is a 2000-unit box).
  So the recoil motion, not the swing, is what the player sees after an air attack there. Next step
  (`combat.rs` line 327, `World::touching_m`): log the contact point and the volume's `a`, `b`, `r` for the
  keyblade volume and check the units (the volume is swept in metres against the world's centimetres through
  `touching_m`'s `M` factor) and the polygon filter (`n.y.abs() < FLOOR_NY`, mask ATTACK_COLLIDE).

### Not ported / gaps
* +0x160 bit 31 (keeps the stick during a swing): its trigger was not identified.
* The trigger-24 / mover-flag-2 velocity write of FUN_00176050 (above) and the field +0x104 = 5.

## 2. Square on the ground: guard, dodge roll, Quick Run

### The original (USA)
`FUN_00177ea0` (Square, idle range), in order: `FUN_001783d8` (actor +0xb50 = 6 only, mover 0x16,
motions 0xc9..0xcc and turns 0x97 / 0x98: a special non-drive mode, not a roll), Glide
`FUN_00178290` (air only), the explicit keys 37, 12, 38 (`FUN_00177fa8`, need a window), Quick Run
`FUN_00178108`, guard `FUN_00178080`.
* Quick Run (`FUN_00178108`): growth slot 1 level >= 1, on the ground, no rider, idle range, stick
  above 0. Velocity = stick x `fmab` AirSlideSpeed (+0x10); +0xf0 AirSlideBrake (+0x14), +0xf4
  AirSlideTime (+0xc), +0xf8 AirSlideStopBrake (+0x18); motion 0xcb (203, A150); mover 0x10; +0x10c
  bit 4 (button held); invincibility +0xb00 = AirSlideStarTime (+0x1c). Its tick `FUN_001c1660`
  counts +0xf4 down only while +0x10c bit 4 is set (`FUN_00174cf8` clears it when the command lacks
  flag 4); otherwise the time drops to 0 at once: brake +0xf8, motion 0xcc (204, A151), then 4.
* Guard (`FUN_00178080`): record key 11 available and an idle range; no stick test (the stick is
  zeroed after). So the original guards while moving when Quick Run is not learned.
* There is **no dodge roll** in the USA executable: no Dodge Roll ability row, no roll motion in
  P_EX100 (motions 201..208 are B003, A160, A150, A151, A170..A173), and the `fmab` rows are 16
  floats: the 17th column the sheet calls `DodgeRollStarTime` shows the next row's HighJumpHeight
  (310, 385, 585, 585). Dodge Roll is a Final Mix addition.

### The Final Mix rules (WP-FIX-10b; `docs/final-mix.md` "Player controls")
The user's rule (guard only when still, tap while moving = roll, hold while moving = Quick Run) is the
Final Mix behaviour. Its numbers were read from the Final Mix executable; only the small rule constants
are in code (`sora::square_ground`, `sora::TAP_TICKS`), no Final Mix data or assets.
* The Square handler (FM 0x178eb0) runs the USA chain first, then, only when the stick magnitude
  (actor +0x1c) is 0.0, the guard (FM 0x1790c0), then the Dodge Roll (FM 0x179530), then Quick Run
  (FM 0x179148), then the guard again.
* Dodge Roll is the **fifth growth kind** (growth slot 4 in FM; the USA table at 0x37bda0 has four).
  The app's `sora::Growth.dodge_roll` is read from `Progress::dodge_roll` (`abilities::Abilities::growth(4)`),
  `KH2_GROWTH=a,b,c,d,e` (fifth value) or F6; hidden in a drive form [inferred: a form's growth is its
  own ability]. The baseline gives Sora Dodge Roll level 1 (the user's wish, docs/progress.md);
  `KH2_ABILITY_START=data` drops it with the Guard so the USA rules stay reachable.
* Decision table (`sora::square_ground`, test `square_decision_table`), after the explicit keys:

  | stick | Dodge Roll | Quick Run | Square does |
  |---|---|---|---|
  | still | any | any | guard |
  | moving | learned | any | roll start: tap = roll, hold = Quick Run (only if Quick Run learned) |
  | moving | no | learned | Quick Run at the press (USA, no tap window) |
  | moving | no | no | guard (USA: the guard has no stick test) |

* **Threshold: 10 ticks** (`TAP_TICKS`). The roll's motion A180 (FM motion 209) has frame trigger type
  34 at tick 10; its handler (FM 0x189c78) starts Quick Run when Square is still held, else the roll
  plays on (test `square_while_moving_tap_rolls_and_hold_runs`). The earlier 12-tick number was an
  inference and is replaced.
* What the app shows for the roll (what the USA disc lacks): motion A180 and the 17th `fmab` column
  (roll invincibility, FM +0x40) exist only in Final Mix. The app starts the dash at the press with the
  Quick Run start motion A150 (203) and the Quick Run numbers (level 1 row when Quick Run is not
  learned) [inferred], runs it `TAP_TICKS` ticks, then the stop brake and motion 204. The original's
  roll installs no mover and no velocity (its travel is root motion of A180, FM 0x179530); that
  part is not reproduced.
* Logs: `sora square: moving, Dodge Roll level N, Quick Run level M: dodge roll pending ...` or
  `... quick run at the press (USA rule)`; `sora guard ... standing still` / `... stick pushed, neither
  learned`; `sora dodge roll: Square released after N ticks (tap, under 10)`; `sora quick run: Square
  held 10 ticks ...`; `sora dash stop: Roll|Run after N ticks`.
* Harness runs (tt00, `KH2_NO_ENEMIES=1`, keys `4.5:K,5.5:W:3.5,6.5:K,7.6:K:0.6` = guard standing, walk,
  tap while moving, hold 0.6 s while moving). Logs seen:
  * `KH2_GROWTH=0,1,0,0,1` (roll + Quick Run): `sora guard A322 (... standing still ...)`, `sora square:
    moving ... dodge roll pending`, `sora dodge roll: Square released after 3 ticks (tap, under 10)`,
    `sora dash stop: Roll after 10 ticks`, then for the hold `sora quick run: Square held 10 ticks, Quick
    Run level 1, 4 ticks left`, `sora dash stop: Run after 14 ticks`.
  * no `KH2_GROWTH` (baseline: Dodge Roll 1, Quick Run 0): the tap rolls as above; the hold logs `sora
    dodge roll: Square held 10 ticks but Quick Run is not learned (level 0)` and `sora dash stop: Roll
    after 10 ticks`.
  * `KH2_GROWTH=0,1,0,0` (USA rules, no roll): `sora square: moving ... quick run at the press (USA
    rule)` at every press; the tap stops with `sora dash stop: Run after 3 ticks`, the hold after 14.
  * `KH2_GROWTH=0,0,0,0`: `sora guard A322 (... stick pushed, neither learned ...)` at both moving presses.
  * `KH2_ABILITY_START=data`: no guard and no dash at all (no Guard ability, no Dodge Roll, Quick Run 0).

## Decompile index
| address | read for |
|---|---|
| 0x176050 | attack start: leap or no velocity write |
| 0x176400, 0x18eec0, 0x18ed58, 0x17bee0 | leap aim: points, radii, rise time |
| 0x1796a8, 0x17b278, 0x17b1e0, 0x17b328 | mover launch, jump set-up, jump mover start |
| 0x179880, 0x16b648, 0x16b698, 0x179798 | mover table and install |
| 0x17b620, 0x1832b8, 0x17b6e0, 0x17b440, 0x17bad0 | jump mover: clock, horizontal, vertical, tick, aerial dodge |
| 0x181e00, 0x182370 | per-tick displacement, root motion, +0x860 |
| 0x169ee8, 0x16a2f0, 0x188030 | gravity latch, fall restart, trigger bits |
| 0x1778e8, 0x174cf8 | stick zeroed outside idle ranges, command flags |
| 0x177ea0, 0x178108, 0x178080, 0x178290, 0x1783d8, 0x177fa8, 0x1b7e00 | Square actions |
| 0x1c1660, 0x1c18c0, 0x1c1840, 0x1c1a48, 0x1c1a78, 0x1c7018, 0x1c70f0, 0x1c7d20 | Quick Run mover, mover 0x16 |

## Gaps (searched without result)
* A dodge roll and a tap/hold threshold in the USA executable: the Square chain above, the mover table
  at 0x1c88eb0, the `fmab` reader `FUN_001cc1a0` users (`FUN_00178108`, `FUN_00178290`,
  `FUN_0017bad0`, `FUN_0016bd98`). They exist only in Final Mix (docs/final-mix.md); the app's rule is
  ported from there.
* The roll's length and travel (A180 is 27 frames at 30 fps with the idle range from tick 36 and root
  motion carrying Sora): not reproduced, the USA disc has no A180.
* WP-F1 item 12 (user report 2026-10-10: the dodge roll's animation differs from the original's). Checked
  again on the USA disc: `obj/P_EX100.mset` has 835 slots (`cargo run -p kh2 -- mset obj/P_EX100.mset A1`),
  so the Final Mix roll's slot 838 (motion 209, `A180`) does not exist, and no other A1xx motion is a roll
  (A100/A101 the summon call, A110, A120 the drive change, A130/A140/A141, A150/A151 Quick Run start and
  stop, A160, A170..A173 Glide). The roll the app shows is the Quick Run start A150 (motion 203) by design
  (above); it cannot match the original without the Final Mix motion data, which the project does not ship.
  Log: `sora square: moving, Dodge Roll level 1, Quick Run level 0: dodge roll pending (tap = roll, hold =
  quick run) (speed ...)`, then `sora dodge roll: Square released after N ticks (tap, under 10)`.
* The writer of +0x108 bit 0x800000 other than `FUN_00169ee8` (scan of `lui 0x80` / `lui 0x100`
  followed by a store to +0x108: only `FUN_00169ee8`).
