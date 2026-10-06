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

### Not ported / gaps
* Landing in the middle of an air swing: the jump mover's landing (`FUN_0017b440` -> `FUN_0017b9e0`
  plays the landing motion) is not ported; the swing plays to its end on the floor.
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
* Harness check: see "Harness runs" below.

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
* The writer of +0x108 bit 0x800000 other than `FUN_00169ee8` (scan of `lui 0x80` / `lui 0x100`
  followed by a store to +0x108: only `FUN_00169ee8`).
