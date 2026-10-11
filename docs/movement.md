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
* The wall recoil that ended every harness swing before the landing (WP-F1's gap) is resolved in section 3
  (WP-F2 item A): the swings crossed real attack-blocking terrain next to the spawn points, and the app's
  wall test was a capsule-with-radius sweep instead of the game's segment cast. With the cast ported, the
  air swing in open ground ends in `move: landed during A330 f16: the landing motion replaces the swing
  (FUN_0017b9e0: slot 5, then idle)` (run `KH2_NO_ENEMIES=1 KH2_MOVELOG=1 KH2_HITLOG=1
  KH2_KEYS=5:S:0.6,6:Space,6.4:J` in tt00).

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

## 3. The wall test of a swing (WP-F2 item A; user report 2026-10-10: a lag after every aerial attack)

What the user saw after air attacks in the app was the wall recoil A350 (ground: A320), not the landing:
every harness swing logged `wall recoil: atkp 131 (BG Hit, keyblade volume) -> motion id 43 slot 172 A350`
(ground: `atkp 126 ... motion id 41 slot 164 A320`) in tt00 and tt04 (base build, runs `6:Space,6.4:J` and
`6:J,6.6:J,7.2:J` with `KH2_NO_ENEMIES=1 KH2_MOVELOG=1 KH2_HITLOG=1`). Two causes, both fixed or explained:

### The original's test (FUN_001a27e8 -> FUN_0018c8e0 -> FUN_00143aa0)
* Each tick the attack entry walks the owner's shapes of kinds 6 and 13 (`FUN_0016b5d0(obj, 0x2040, ..)`)
  whose group byte equals the entry's group (+0xc4) and takes each one's centre (`FUN_001894d0`: bone
  matrix x offset, y minus the lift). The centres of the last tick are kept at entry +0xd0 (16 bytes each,
  +0xc8 counts them); entry flag 4 is set once they exist, so the first tick of a window casts nothing.
* Only the FIRST centre of the group (index 0) is cast against terrain: `FUN_0018c8e0(entry, &prev, &cur)`
  runs when the row has BG Hit (`atkp` +0x12 bit 0), the entry has neither 8 nor 0x10 (a wall already hit) and
  either has 0x100 or lacks 2 (no target hit). It is a SEGMENT from the previous centre to the current one
  (`FUN_00143aa0(from, to, .., 4)`): no radius. When it misses and the row's `FloorCheck` (+0x28) is not 0,
  a second segment from the current centre with y + FloorCheck (the game's y, the app's -y [inferred]) is
  cast; every Sora combo row has 0.
* The cast (`FUN_00141f08`, map `DAT_00355934`): the polygons of the swept box pass the mask handler
  (`FUN_00141b50` -> `FUN_0032e3b8(.., 4)`; the table at 0x378f7c gives handler 4 = `FUN_0032e420`:
  `attribute & 0x80`, the COCT ATTACK_COLLIDE bit, any orientation: floors and ceilings count), each is cut
  by `FUN_001623a0` only when the segment runs AGAINST its normal (`n . dir < 0`: one-sided) with the
  parameter in 0..=1, and `FUN_00141d90` keeps the hit nearest the start. The hit goes to `FUN_0018ca78`
  (flags, spark, sound 500010 for a weapon-owned volume, the owner's recoil method +0x70; docs/hit-reaction.md).
* The other shapes of the group (W_EX010 group 1: offsets -40, 0, -34, -68 on bone 2, radius 40) are only
  swept against targets (`FUN_0018d768` in steps of the radius); a swing that already hit a target skips
  the wall cast.

### What the app did, and does now
* Before: `combat::detect_hits` swept EVERY live shape as a capsule of its radius (40 to 140 units) against
  wall-like triangles (`World::touching_m`, floors filtered, both faces), so a wall within 40 units of any
  point of the blade, or a 150-unit step behind the blade's path, recoiled Sora. Now: `combat::wall_cast`
  = `World::cast_m` (one-sided segment, no radius, mask ATTACK_COLLIDE, every orientation) from the first
  volume's centre of the last tick to this tick's, with the FloorCheck probe, once per window; the previous
  centre is kept per (attack body, trigger start) in a `Local` of `detect_hits` and forgotten with the
  window. Logs (`KH2_HITLOG=1`): `wall cast: atkp 126 window 15 from Vec3(..) to Vec3(..) hits terrain at
  Vec3(..) (normal Vec3(..)); volume radius 0.40` before each `wall recoil:` line; `wall test not run: atkp N
  has no BG Hit, volume path crosses a wall at ..` for rows without the flag.
* The premise "no wall within 800 units of tt00's spawn" was wrong. `map/us/tt00.map`'s COCT (16
  polygons, all with ATTACK_COLLIDE) is the 20 m box plus a 150-unit STEP across z = 0: a riser facing -z
  (polygons 10, 11) and a raised floor at y = 150 for z > 0 (polygons 12, 13). Sora spawns at (115, 0, -35)
  facing +z, 35 units from the riser, and the first keyblade volume's centre (bone 2 offset -40) goes from
  (121, 187, 47) to (124, 115, 68) during A300: it crosses the raised floor from above, which the game's
  cast reports too. tt04's spawn (-2015, 114, 1190) faces a wall 20 cm away (z = 13.45 m) with another at
  z = 9.1 m behind; the first air swing's centre goes from z = 12.80 to 13.65. These are real walls.
  Test `world::tests::tt00_has_an_attack_blocking_step_in_front_of_the_spawn`.
* Runs (`KH2_NO_ENEMIES=1 KH2_MOVELOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1`, logs in the report):
  * tt00 at the spawn, `6:Space,6.4:J` and `6:J,6.6:J,7.2:J`: still `wall cast: atkp 131 window 12 from
    Vec3(1.0998299, 2.0402899, 0.98999876) to Vec3(1.1281766, 1.3007644, 0.826493) hits terrain at
    Vec3(1.1205397, 1.5, 0.87054306) (normal Vec3(0.0, 1.0, 0.0))` then the recoil: the step.
  * tt00 after walking 0.6 s away from the step, `5:S:0.6,6:Space,6.4:J,7.5:J,8.1:J,8.7:J`: no `wall cast`
    and no `wall recoil` line; the air swing ends in `move: landed during A330 f16: the landing motion
    replaces the swing (FUN_0017b9e0: slot 5, then idle)` and the three ground swings are plain A300s.
  * tt04 at the spawn: the wall at z = 13.45 (normal (0, 0, -1)) is crossed and recoils, as the game would.
  * tt04 after `5:A:1.5` (Sora at (-1500, 87, 752), behind a one-sided fence polygon at x = -14.6 m facing
    +x): the air swing's centre pokes through the front side and back (`from x -14.55 to -14.65`) and
    recoils; the ground swing after it does not. A back-to-front crossing is ignored (one-sided).
* Tests: `combat::tests::wall_cast_hits_only_terrain_the_centre_path_crosses` (open ground, a crossed wall,
  a wall without the flag, no movement, from behind, ending on the plane),
  `combat::tests::wall_cast_floor_check_probes_down_only_when_set`.
* Not ported / gaps: the hit point the game stores (entry +0xb0, the cast's point) is what the app uses;
  the spark variant by the polygon's type nibble and flag 0x100 (docs/hit-reaction.md) are unchanged. The
  sign of the FloorCheck step is inferred from the COCT convention (the game's y is the app's -y). Whether
  `FUN_001657d8`'s object pass (the first half of FUN_00143aa0: other objects' collision) can stop a swing
  was not read; the app casts against the map only.

## 4. Play-test follow-ups without a code change (WP-F2 items B and D)

### B. "The keyblade's faces look inverted" (WP-F1 item 11): not reproduced
* `sora::follow_hand` puts the weapon socket at the hand bone's full pose matrix (`Transform::from_matrix`),
  so a negative-determinant pose would mirror the blade. Checked on the disc (`skel::tests::
  hand_bone_poses_never_mirror_the_weapon`): no bone of P_EX100, W_EX010, P_EX020 or P_EX030 has a negative
  bind scale, every rest-pose matrix has a positive determinant, and bones 178 and 86 keep determinant
  1.000 through every frame of A000, A300, A330 and A343. The game's weapon attach (FUN_00184190 path,
  docs/drive-forms.md) was not re-read: with no mirror in the data there is nothing to counter.
* The render path cannot invert a face either: `assets::material_for` is unlit, `cull_mode: None`,
  `double_sided: true`, so winding changes nothing visible.
* Screenshots looked at (tt00, `KH2_DIST=2.5`, A300 at frame 7): from the side (`KH2_YAW=1.57`) the blade is
  raised behind Sora's head, whole, teeth outward; from behind (`KH2_YAW=3.14`) the grip and blade are
  above his head. Valor (`KH2_DRIVE=1 KH2_DRIVE_AT=4`, B300 frame 7): the right-hand keyblade is drawn,
  the left one is behind the body at that frame. Donald's and Goofy's weapons are only visible in battle
  (`party::member_hands`), and no run put them in frame with enemies present. The reference clips
  (4.mp4, 9.mp4) show the fighter too small to judge the blade's orientation.

### D. The dodge roll on the USA disc (WP-F1 item 12): no change
`cargo run -p kh2 -- mset obj/P_EX100.mset` on the USA disc: 835 slots, 84 with motion. The tags are
A000..A010 (idle, walk, run, jumps, landing), A100/A101 (summon), A110, A120, A130, A140/A141, A150/A151
(Quick Run start and stop), A160 (Aerial Dodge), A170..A173 (Glide), A200..A214 (hurt), A300..A304,
A310..A317, A320..A322, A330..A333, A340..A344, A350, A360..A372 (attacks and reactions) and B003: no A180
and no other roll. The Square handler chain (section 2) gives, on the USA disc, Quick Run at the press
while moving with Quick Run learned, else the guard. The app keeps the Final Mix rule (tap = the stand-in
roll played with A150) because the user asked for it; `KH2_GROWTH=0,1,0,0` shows the USA behaviour.

## 5. Which collision polygons stop the walking player (WP-G3; user report 2026-10-11: invisible walls)

The user met invisible walls in Tram Common and Station Heights. Place names (`msg/us/place.bin` through
`hud_card`, log `hud: area card for tt7: "Market Street: Tram Common"`): **tt07 = Tram Common, tt06 =
Station Heights** (tt04 and tt05 are both Sandlot, tt08 Station Plaza).

### The original's rule
* The player's body collider is swept by `FUN_00143840` -> `FUN_00164138` (the sphere, stepped through
  `FUN_00163f98`; `FUN_0019a8e8` is the actor's entry). The polygons come from `FUN_00141b50`, which walks
  the map's tree and keeps a polygon when BOTH hold:
  1. the filter armed by `FUN_00164138` itself (`FUN_00141b30(map, 0xf, 4, 1, 0)`: mask 0xf, value 4,
     inverted): the attribute's low nibble is **not 4**;
  2. the handler `FUN_0032e3b8(map, kind)` picks from the table at 0x378f7c accepts the attribute word.
     `kind` is bits 3..6 of the collider flag word (actor +0x600; `(word << 0x1d) >> 0x20 & 0xf`, the
     argument of the `FUN_00141b50` call at 0x164138). The handlers (code at 0x32e3dc..0x32e44c, read
     from the executable's instructions; only index 4 has a decompiled `.c`):

     | kind | handler | accepts |
     |---|---|---|
     | 0, 6 | 0x32e438 | attribute & 0x10 (PARTY_COLLIDE) |
     | 1 | 0x32e3dc | attribute & 0x20 (OBJECT_COLLIDE) |
     | 2 | 0x32e3f0 | attribute & 0x40 |
     | 3 | 0x32e3fc | attribute & 0x10 or attribute & 0x80000 |
     | 4 | 0x32e420 | attribute & 0x80 (ATTACK_COLLIDE; section 3) |
     | 5 | 0x32e42c | attribute & 0x100 |
     | 7..15 | 0x32e444 / 0x32e44c | everything |

* Party members have kind 0: their constructors `FUN_001746e0` and `FUN_00174800` clear bits 3..6 of
  +0x600 (`puVar4[0x180] & 0xffffff87`). Other classes set theirs: `FUN_001a5178`, `FUN_001a51e8` kind 1
  (objects), `FUN_00189858` kind 3 (townspeople), `FUN_001d5c70`, `FUN_001d5cf8` a computed kind.
* The solver `FUN_001632b8` then SKIPS a polygon whose attribute has bit 0x100000 (it tests it first, in all
  three of its passes). It is one-sided: `FUN_001625e0` takes the signed plane distance of the sphere centre
  and collides only for `0 <= d <= radius`, so a polygon cannot be walked into from behind.
* `FUN_00330c80` records, for a hit polygon with attribute 0x800 or 0x1000, the polygon index and normal in
  the collider (+0x62, +0x40 and +0x64, +0x50): the barrier bits, not a collision decision.

### What the app did, and does now
* Before: `World::slide` (the player's wall slide, `Motor::move_by`) stopped at EVERY wall-like triangle
  (normal more than 60 degrees from vertical) whatever the polygon's attribute. A map's walls with attack-
  only (0x82) or object-only (0x22, 0x62, 0x80002) attributes therefore blocked Sora although the game's
  walking filter drops them. `kh2_map::stops_party(attr)` is the filter; `World::slide` applies it to maps
  with COCT data (`slide_with(.., party)` takes the choice; the enemies' simple mover `step_m` keeps the old
  behaviour because their kind was not read).
* Survey of the disc (wall-like polygons by attribute): the real walls are 0x172, 0x1f2, 0x1172, 0x972,
  0x3f1, 0x3f2, 0x7f1, 0x803f1; the others are 0x82 (several hundred per Twilight Town area: tt06 352,
  tt07 480, tt10 861), 0x22 (tt07 114, tt06 7), 0x80002 (tt07 63), 0x62, 0x284. Test of the walk-through
  below counts them.
* Tram Common examples (tt07): the box z = 2100..2500, x = 550..1600 (0x80002 on the -z face, 0x22 on
  the +z face) and the volumes at x = 3700..4100 (0x80002 / 0x22) in the middle of the plaza; Station
  Heights (tt06): 0x22 slabs at x = -1250, 0x82 walls at x = 1400 and 450 on the lower level.
* Test `world::tests::invisible_walls_of_tram_common_and_station_heights_do_not_stop_sora` (reads
  `map/us/tt06.map` and `tt07.map`, needs `KH2_ISO`): five scripted runs of 30 ticks of 8 units into such
  walls. The old rule (every polygon stops) halts Sora in front of the plane (for example z = 2065 at the
  box face z = 2100), the walking filter lets him through; four real walls (0x172 at x = 313 in tt06,
  0x1f2 at z = -1700 in tt07) stop him at the same point under both. Unit test
  `kh2_map::tests::walking_filter_keeps_party_collide_only` covers the attribute rule.
* Harness (the log `KH2_TRACE=1` prints one `trace ... pos (x y z)` line per tick; build of the old rule =
  the integration build in the main checkout):
  * tt07 `KH2_ENTRANCE=2 KH2_KEYS=6:W:4.0` (Sora at (4650, 0, 1100) facing -x): old rule `pos (4538 0 1100)`
    and he stays there; walking filter `pos (4522 0 1100)`, `(4122 0 1100)`, `(3722 5 1100)` ... `(3128 -246
    1114)`: through the x = 4100 volume.
  * tt06 `KH2_ENTRANCE=0 KH2_KEYS=6:S:3.0` (Sora at (-1175, 0, -300), the wall at x = -1250 behind him): old
    rule stops at `pos (-1215 20 -300)`; walking filter `(-1223 20 -300)`, `(-1255 40 -300)` ... `(-1786 40 -305)`.

### Not ported / gaps
* The floor and ceiling probes keep their own rule: `World::ground` takes every upward polygon and
  `World::ceiling` every downward one; the game's floor ray (`FUN_001423d8`, filter armed by
  `FUN_00143840`'s callee) was not compared for attributes (PARTY_STAND bit 0x1, "player fall" 0x4).
* The mode bits of the collider (bits 1..2 of +0x600; `FUN_00169af0` sets bit 1 from byte +4 of the body
  shape) and the map flag +0xd1 select special passes of `FUN_001632b8` that treat attribute nibble 1
  differently; not ported (the app uses one pass).
* Kinds other than 0: enemies (`step_m` keeps stopping at every wall), the townspeople (kind 3) and objects
  (kind 1) were not matched to their app movers. `prize.rs` calls `World::slide` and therefore gets the party
  filter now: see the report's outside-ownership note.
* The wall/floor split by the 60-degree normal is the app's; the game's sphere solver pushes out of every
  accepted polygon whatever its slope.

## Decompile index
| address | read for |
|---|---|
| 0x1a27e8, 0x18c8e0, 0x18ca78, 0x1894d0 | attack entry tick, wall cast gate, wall hit, volume centre |
| 0x143aa0, 0x1657d8, 0x141f08, 0x141b30, 0x141b50, 0x32e3b8, 0x32e420, 0x141d90, 0x1623a0 | the segment cast and its polygon filter |
| 0x143840, 0x164138, 0x163f98, 0x1632b8, 0x1625e0, 0x330c80, 0x19a8e8, 0x378f7c table (0x32e3dc..0x32e44c) | the walking collider: polygon filter, kinds, solver (section 5) |
| 0x1746e0, 0x174800, 0x1a5178, 0x1a51e8, 0x189858, 0x1d5c70 | collision kind of each object class (+0x600 bits 3..6) |
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
