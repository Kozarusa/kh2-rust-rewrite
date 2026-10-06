# Effects: three ported programs, hit effects, effect 0x12 and the Heartless death (WP-1F, WP-1F2)

Findings from reading the game's main executable (`SLUS_210.05`, USA): the decompilation, plus
disassembly of the handlers the decompiler dropped, plus runs of the game's own handlers in the
`kh2-ee` interpreter. Addresses are virtual addresses in that executable. Nothing here is copied
code; the ports are in `crates/kh2-fx/src/prog_more.rs` and `crates/kh2-app/src/fx.rs`, and every
number the game uses comes from the user's disc at run time.

Status words: **verified** = read directly in the game's code or data, or checked against the game's
own handler in `kh2-ee`; **inferred** = follows from what was read but a name or meaning is a guess;
**unknown** = not found or not looked at.

## The three ported programs

Verified (program table at 0x362b10, handler addresses read from it):

| program | table index | frame | draw | init | reset |
|---|---|---|---|---|---|
| pppKeMvYpEff | 47 | 0x1f4628 | none | none | none |
| pppKeShpTail3X | 157 | 0x1fcf70 | 0x1fd388 | 0x1fdb90 | 0x1fdbf0 |
| pppSnoMdlDttNorm | 238 | none | 0x20a7e8 | 0x20a7b8 | 0x20a7d0 |

In the system effect file `00effect.bar` they are element ids 11 (Tail3X), 12 (Sno) and 13
(MvYpEff): `KH2_FXTEST=11`, `12`, `13` runs them. Other files that use them are listed in the
earlier scan: Tail3X in most `M_EX*` magic files, many bosses and `P_NM000`, `P_MU000`,
`SAVE_POINT`, `W_TR000` (over 100 files); MvYpEff in `B_EX170_LAST`, `P_EX100_*_ULTF`, `P_EX200`,
`P_EX220`; Sno in `P_EX100` variants, `P_EX120`, `AL14_*`, `G_EX900*`, `B_EX130/140`, several
`M_EX*`, `N_BB050_BTL`, `N_HB630`, `PO06-08_PLAYER`. Shadow and Soldier use none of the three.

### pppKeShpTail3X (verified)

A tail of randomly rotated sprites strung along the recent path of the object. The frame handler
and the draw handler were both read (the draw handler from disassembly, since the decompiler has
no function there) and both are checked against the game by `crates/kh2-fx/src/oracle_tests.rs`:
the test runs 0x1fcf70 and 0x1fd388 in `kh2-ee` on 14 ticks of a moving object in three
configurations (local space with random rotation, world space, zero spacing; 518 sprites) and
compares the work memory, the shape frame chosen, the color bytes and the whole matrix handed to the
shape-draw routine 0x21a660. All agree (floats within 0.2 percent).

Work memory (relative to the program's first work offset `useVal[0]`, which is added to the object's
0xa0): eight 16-bit channels at +0x00, their rates at +0x10 (channels 0-3) and +0x20 (4-7), the
rates of the rates at +0x18 and +0x28, a ring of 0x1c samples of 16 bytes at +0x30, a 32-bit time
accumulator at +0x1f0, a 16-bit random seed at +0x1f8 and the ring head at +0x1fa. `useVal[1]` is the
color work (the packed bytes at +8; only the alpha byte is used), `useVal[2]` an optional extra matrix.

Init clears head, accumulator and the 0x30 bytes of channels and sets the seed from the C library's
`rand()` (FUN_003117a8); reset clears only the 0x30 bytes.

Frame, once per tick: on the first frame of a run (object frame 0, not a loop restart) every ring
sample takes the current position; then the head moves back one slot and the new sample goes there.
The sample is the object's local position, taken through the bind matrix when parameter byte +0x53
is set (world space). Then for each channel rate += rate of rate and value += rate, and on the
frame the entry names its 0x30 bytes at +0x20 (24 i16: values, rates, rates of rates, in the work
layout) are added to the work area.

Entry layout (0x58 bytes): +0 frame, +4 shape index (0xffff draws nothing), +8 accumulator step
(i32), +0xc first size (f32), +0x10 last size, +0x14 random size shrink, +0x18 first spacing,
+0x1c sprite count (u8), +0x1d draw enable (u8), +0x1e random rotation enable (u16), +0x20..0x4f the
deltas, +0x50 / +0x51 / +0x52 / +0x54 / +0x55 flag bytes that only select bits of the draw
call's flag word (not interpreted), +0x53 world-space flag.

Draw, once per tick (the accumulator advances by the entry's step afterwards, unless paused):
the start color is channels 0-3 and the end color channels 4-7 (each >> 7; both alphas times the
object color's alpha), stepped from start to end over `count - 1` instances. The first sprite sits at
the newest ring sample; the rest are placed along the polyline of the ring at distances that start at
the entry's spacing and shrink in proportion to the size (spacing times size step over size). Every
sprite draws a shape frame `((accumulator + seed) / wait of frame 0) mod sequence length`, with the
seed stepped `seed * 0x80d + 7` (16 bit) per sprite and starting from the stored seed on every
draw (it is not saved), a size shrunk by `seed / 65535 * shrink`, camera-facing axes taken from the
object's local matrix rows scaled by the bind's scale, and with the rotation enable set a turn
about the view axis by the seed taken as an angle. The color bytes are the float color cast to int
and truncated to a byte (they wrap). Not interpreted: the draw call's flag word (bits 0x80, 0x10,
0x800, 0x400, which `pppDrawShapeX` also ignores in the port).

### pppKeMvYpEff (verified)

No parameters. Work 1 holds a velocity (floats), work 0 the object's angle triple (integers, one
turn = 0x10000). Nothing happens while the velocity is zero. The Z angle becomes
`atan2(vx, -vy)` and the X angle `atan2(-vz, hypot(vx, vy))`; Y is not touched. The game's atan2
(0x1f44d0) is quadrant logic around a double-precision `atan` and gives no result for (0, 0); the
conversion is `(int)(angle * 32768.0 / pi)` through the soft-float routines. 50 velocities (40
random, the axes, the zero and mixed cases) match the game within one unit
(`mv_yp_eff_matches_the_game`).

### pppSnoMdlDttNorm (verified)

A model that shows the screen shifted along the model's normals: a lens or shock-wave ripple.
Verified by the oracle tests `sno_amplitude_matches_the_game` and `sno_geometry_matches_the_game`
(the first runs the handler 0x20a7e8, the second also the model draw FUN_0020ab40 with its VU0 code,
on a small model with stored normals that differ from the radial directions, in all four combinations of
the two flag bytes; the 96 vertices' texture coordinates move by the screen displacement the port
computes within 3e-5, which is about 0.03 pixel):

* Work 0 holds an amplitude as three floats (value, rate, acceleration; init and reset zero them);
  every tick value += rate += acceleration, and on the entry's frame the entry's floats at +8, +0xc,
  +0x10 add to value, rate and acceleration. The draw is skipped for model 0xffff or a zero color
  alpha (byte +0xb of work 1).
* The model is drawn by FUN_0020ab40 with the matrix in work 2 (a draw-matrix program such as
  `pppKeDMatFr` copies its camera-space matrix there), the amplitude, and the GS ALPHA value of entry
  byte +0x1c (0 means 0x44). Entry byte +0x16 does two things: when set the amplitude is multiplied by
  the bind's x scale and the displacement direction of a vertex is the unit vector from the model's
  origin to the vertex (in view space); when clear the direction is the vertex's stored normal (4.12
  fixed, through the matrix's 3x3 part and **not** normalised, so the matrix's scale shrinks the
  displacement with it). Entry byte +0x15 (taper): the displacement's x and y are multiplied by the
  unit direction's view-space z. Byte +0x14 only picks which frame-buffer size globals the packet
  setup reads; byte +0x17 is copied into the draw's setup, its use was not found.
* The texture coordinate of each vertex is the projected screen position of (view-space position +
  displacement), in pixels, minus the frame buffer offset, divided by 1024; the triangle is drawn where
  the undisplaced vertex projects. The frame buffer is the texture, tinted by the vertex color times
  the object color, blended with the entry's ALPHA.

### Sno: clip, back-face test, depth bias (WP-1F3, verified)

The interpreter now has a model of the VU0 status and MAC flags for the one routine that reads them
(`oracle_tests.rs::install_vu_flags`: a probe on every flag-setting FMAC instruction of 0x20ab40 computes the
zero and sign bits of its result as the VU manual states them). With it the game's own model draw runs its
tests and `sno_clip_and_cull_match_the_game` compares the triangles it kicks with the port's, vertex by vertex
(screen x, y, z words, texture-coordinate displacement), for models in front of the camera, culled and not
culled, and for models that cross the near plane and the 4096-pixel window, with and without bias:

* **Clip test** (0x20af98..0x20afbc, repeated for each vertex): the vertex goes through the projection, then
  `vdiv Q = 1 / w`, `screen = clip * Q`; STATUS is cleared and `w - 1`, `x`, `y`, `4096 - x`, `4096 - y` are
  computed with vsub; a sticky zero or sign bit (STATUS & 0xc0) drops the whole triangle. So a triangle with any
  vertex nearer than w = 1, or off the 0..4096 window, is not drawn (the GS would clamp it; the game does not
  rely on that).
* **Back-face test** (0x20b0b4, only when the model flag word has bit 1 clear): `vopmula/vopmsub` of
  `(p2 - p1)` and `(p1 - p0)` on the screen positions, the MAC flag's sign bit of z (0x20) skips the triangle.
  The port's `(p1 - p0) x (p2 - p1)` with the game's y-down screen is the same sign.
* **Depth bias** (handler 0x20a7e8): the projection's z translation `p32` (0x1db1038) becomes
  `p32 - entry float +0x18 * 1e7 / (the model matrix's view-space z)` in double precision, converted back to a
  float **by truncation** (0x327230; Rust's `as` rounds to nearest, `f64_to_f32_trunc` is the port's), for the
  duration of the draw. GS z is `p22 + p32 / z`, so the port gives the batch `depth_scale = p32 / p32'` and
  tests depth as if every vertex sat at that factor of its distance (`Batch::depth_scale`, applied in the
  vertex stage of `fx_distort.wgsl`; the factor is the only thing the bias changes). The disc's Sno entries use
  +0x18 of 0, -20 (`P_EX100`, `P_EX120` model 5), -50 (`B_EX130`) and -80 (system effect 12 model 18, `B_EX140`).
  `sno_amplitude_matches_the_game` checks the biased translation the draw sees (and its restoration).
* **Depth test off** (entry byte +0x17, `Batch::depth_test`): the byte reaches the draw's setup; every Sno
  entry on the disc has it 0, so the depth test is on (ZTST GREATER). The mapping of the set case to ALWAYS is
  an **inference** from the packet setup (the byte lands in the packet word that carries the TEST register).
* **The projection** is the game's (`kh2_fx::projection::Projection`, `projection_matches_the_game`): the 4x4 the
  camera builder FUN_001078f0 -> FUN_00106ba0/00106c48/00106a68 leaves at 0x3b21f0 for a given horizontal field of view,
  from the constants at 0x3780f0..0x378164 read from the executable: focal length
  `(512 / 0.8 * 0.5) / tan(hfov * 0.5)`, x and y scales 0.8 and 0.8667, centre 2048, z row
  `p22 = (near * c - far * near) / (c - near)`, `p32 = near * c * (far - near) / (c - near)` with c = 4e6,
  near 1, far 1.677e7. The stored matrix has 0 where the port keeps 1 in the z row's w (the model draw adds
  the w = z term itself); nothing else differs.

The port produces the displaced geometry as `Batch::offsets`; `fx.rs` draws it with the material
`DistortMaterial` (`fx_distort.wgsl`), which reads the scene drawn so far through Bevy's transmission
copy and blends by hand. What differs from the console: only what the opaque pass drew is seen (not
effects drawn before the lens), the sample is bilinear, and the displacement is interpolated per
fragment instead of the screen position per vertex.

## Hit effects (FUN_0018d090, verified)

`FUN_0018cf50` builds the hit record and calls FUN_0018d090, which starts the effects of a hit. The
fifth argument is the target type: 1 for a normal target (the victim's method +0x60, FUN_00186638,
returns 0 and the hit is accepted by FUN_001be108), else 4 (inferred: the victim is in a protected or
invulnerable state; the method tests flag bits of its state words).

* **Ineffective hit.** FUN_0018b618 sets hit-record flag 4 (and reaction 0) when the victim's
  multiplier byte for the attack's `Element` times its general multiplier (the `enmp` weaknesses by
  element, index 6 general) is 0. Then FUN_0018d090 starts **element id 0x12 of the system file**
  (`00effect.bar`, the global holder at 0x1c954d8; element 18: effect 17, bone 0, flags 0, sound 179)
  instead of the spark, and no `DamageEffect`. Shadow and Soldier have all weaknesses at 100, so it
  never plays on them with the disc's data; `KH2_FXINEFFECTIVE=1` forces it.
* **Hit spark (`EffectOnHit`, `RandomEffect`).** The category mask is `type | 1 << (32 + EffectOnHit - 1
  + roll)`; the roll is `rand % RandomEffect` when that is non-zero. The first element of the
  **attacking volume's** effect file whose category has all those bits is started (FUN_001db9e0 finds
  its id, FUN_001db868 starts the first element with that id). For Sora's keyblade attacks that is
  `W_EX010`; for an enemy's claws it is the enemy's own file: `M_EX020` has element 3 (category
  bit 32, "hit by EffectOnHit 1") and 4 (bit 33), `M_EX010` the same plus 7. The current code
  played the keyblade's spark for every attacker. For a spell's attack object (FUN_001a2ba0, made by
  the spell's script) the file is the spell's `.mag` PAX, which the script anchors its attacks to
  (host 2.30): `FIRE_1.mag` has element 3 (bit 32) and 4 (bit 33) for `atkp` 502's `EffectOnHit` 1
  and 2. WP-7A: `magic.rs` sends `fx::SpellHit` (target, `atkp`, the spell's file) with every spell
  hit and `hit_sparks` takes that file and the hit's own row (`GameData::hit_row`: the variant the hit
  count picked, whose `EffectOnHit` can differ from the first row's) [the anchoring is inferred from
  host 2.30 storing the file pointer at attack +0x24; the spark's reader of +0x24 was not traced].
* **Placement of a hit effect.** After the effect is created the game overwrites its position with the
  hit point (FUN_0018ce18: the point on the victim's volume nearest the attack) and, for the spark and
  effect 0x12, its rotation with `(atan2(-dy, hypot(dx, dz)), atan2(dx, dz), 0)` of the hit direction
  (FUN_0017be00; dy is 0, the direction is horizontal). The element's own translation and rotation
  are replaced, not added to. `Placement::hit` in `fx.rs` does this.
* **Victim-side `DamageEffect`.** Not in the ineffective case, and only when the victim is not in a
  state with flag 0x200 at +0x108: FUN_001a22a0 looks in the **victim's own** effect file (holder at
  victim +0x80) for the first element whose category has `1 << (32 + DamageEffect - 1) | 0x200`,
  and starts it at the hit point with its y one unit above (the game's y points down). In the USA
  `atkp` data `DamageEffect` is 0 or 1 (855 of 2326 rows are 1, including most Sora combo hits). The
  disc has 15 elements with the 0x200 category, all in boss and party files (`B_BB110`, `B_BB120`,
  `B_BB130`, `B_EX100`, `B_EX330`, `B_HE020`, `B_HE100`, `B_HE110`, `B_LK120`, `B_MU120`, `B_TR000`,
  `F_WI310`, `N_TR010_BTL_L`, `P_EX330`, `P_EX350`). **Shadow, Soldier and Sora have none**, so on them
  `DamageEffect` starts nothing; this is the game's own behaviour, not a gap. The path is wired and
  can be looked at with `KH2_FXTEST=B_HE100:1` (any object's file, any element).

## Heartless death (verified, effect id and mode; not the whole timeline)

Chain: the victim's damage method +0x78 (FUN_00186238, the same in all eight class vtables that use
the receive function FUN_001b21f0 that were checked) clamps HP (FUN_001a0100); when gauge 0 reaches 0 it calls
FUN_00186530, which calls the victim's method +0x5c: FUN_001b25d0 (or FUN_001d5df8, which then
calls it). FUN_001b25d0 looks up the script mode named **"dead"** (string at 0x37ba30) in the
object's mode table and switches to it; it also does munny and drop bookkeeping (calls to
FUN_0015b198 and FUN_0016b738 that were not decoded) and sets the dead flag (bit 2 of word +0x7f8,
FUN_00186580). So death starts at the
moment HP reaches 0, together with the hit reaction, not when the body lands.

The `dead` mode of both Shadow (`M_EX020`, routine 0x641) and Soldier (`M_EX010`, 0x793) runs the same
script: `ground_marker` (host 1.92 = FUN_0016b738) when the context field +0x1c is non-negative,
`spawn_attached(obj, 2, 1, 0)` (host 1.87 = FUN_0016a570, which starts the first element with
that id of the object's own effect file bound to it), release the handle, set object flags 3 and 6,
wait `ctx.+0x20 * 0.5` frames, `fade_out(obj, ctx.+0x20 * 0.5)` (alpha 1 to 0), wait the same, `hide`,
then yield forever. The context value is 30.0 in both scripts (set in the init routine), so the body
holds for 15 frames, fades over 15 frames and is hidden at frame 30, while effect 2 plays.

* **Death effect: element id 2 of the object's own effect file.** Shadow: element 2, bone 0x4000
  (the object's root matrix), flags 0x1 (bound), sound 201019, three groups that each run a
  `pppRyjMegaBirth` emitter. Soldier: element 2, bone 0x4000, flags 0x1, sound 109. Effect 2 is *not* a sink effect: Shadow's own `leave` mode spawns id 1, which its
  file does not contain (Soldier's id 1 is its leave effect), and Shadow's sink and rise motions
  have no effect trigger. `enemy.rs` starts id 2 on sink and rise; that looks wrong.
* The currently invented `Dying` state of `enemy.rs` (hold 0.6 s, sink into the ground, despawn at
  1.6 s) has no counterpart in the script above. What removes the hidden object afterwards (flags 3
  and 6, then an endless yield) was not found.

## Message type and hook points

`fx::GameEffect` (a Bevy message):

* `Damage { victim, effect, point }`: the victim-side `DamageEffect` above.
* `Ineffective { point, dir }`: system effect 0x12 at the point, turned along the hit.
* `Death { victim }`: element 2 of the victim's own file, bound to its body; a second `Death` for
  the same victim is ignored. `victim` is the fighter's root entity (the one with `Fighter`; its
  `hurt_body` carries the model).

`fx.rs` already does both halves until the other modules call it:

* `hit_sparks` reads `combat::Hit`, decides ineffective (target `Enemy.stats` or `Sora.stats`
  weakness for the attack's element times the general weakness is 0), starts the attacker's spark and
  sends `Damage` or `Ineffective`. **combat.rs needs no change**; it keeps sending `Hit` once per
  landed hit.
* `enemy_deaths` sends `Death` for any enemy whose `hp` is 0. **Hook in `enemy.rs`**:
  `receive_hits`, right after `e.hp = (e.hp - dmg).max(0)`, send `GameEffect::Death { victim: h.target }`
  when `e.hp == 0` and delete the watcher; delete the two `StartEffect { id: 2 }` writes in `think`
  (sink and rise); an ineffective hit (weakness product 0) must also skip the flinch, push and blow
  there, as the game sets reaction 0.
* `sora.rs` (Sora as the victim) needs nothing: his hits arrive as `Hit` too, and his file has no
  `DamageEffect` element.

## Test and harness hooks (all in `fx.rs`)

`KH2_FXTEST=<id>` (system effect), `k<id>` (keyblade), `<name>:<id>` (any `obj/<name>.apdx`, loaded on
first use); `KH2_FXTEST_EVERY=<ticks>`, `KH2_FXTEST_DIST=<m>` (negative: toward the camera),
`KH2_FXTEST_AT=x,y,z`, `KH2_FXTEST_BIND=sora` (follow Sora, for tails). `KH2_FXKILL=<s>[,<s>...]` defeats
every living enemy at those times (both `M_EX020` and `M_EX010` die with their effect 2).
`KH2_FXINEFFECTIVE=1` makes every hit ineffective. `KH2_FXLOG=1` logs the effect decisions.

## Bound effects: what follows the owner (WP-1F3, verified except where marked)

The game's effect record (the "bind record", created by FUN_001db5f0 from the PAX element, ticked by FUN_001dd0d0)
works like this, and `fx.rs::tick` follows it:

* **Creation** (FUN_001db868 finds the first element with the id; FUN_001db5f0 starts it): the start wait is the
  element's `start_wait`, plus `DAT_0037c778` (0.1) when element flag 1 (BIND) is set. The record's clock
  (+0x4c) starts at 0 and gets the frame step added at the end of every FUN_001dd0d0; nothing runs, draws or
  sounds while the clock is below the wait. **Inferred**: the step is one 60 Hz tick (the game adds `DAT_00356f40`,
  2.0 per update in its own units). On the disc only `N_HB630` effect 7 (wait 20) has a non-zero wait.
* **The sound** is started when the instance starts (FUN_001dd748 / FUN_002c32b8), so `effect_sounds` was
  folded into `tick` and a waiting element starts its sound at the wait.
* **Following the owner** (per tick, FUN_001dd0d0 -> FUN_001dcd08 for the object-bound mode): flag 1 clear: the
  matrix is read once (the first tick) and kept. Flag 1 set: it is read again every tick, except that an element with
  flag 2 (BIND_ONLY_START) keeps the first reading once the instance runs. The matrix is the owner's bone matrix
  (FUN_0016aa18: bone number below 0x4000 a bone, 0x4000 the object's own matrix, 0x4001 and above other object
  matrices, see "Object bones" below) times the element's own placement. `Bind::Owner` always followed the bone before; now the three
  cases differ. Element flag 0x20000 (a camera-relative bind, FUN_001dc978) is not ported (no element the app
  starts has it; **gap**).
* **The owner goes away** (FUN_001dcd08: FUN_0019a3f8 on the owner returns 0): the record is marked orphaned (flag 0x10),
  stops updating and keeps its last matrix; the effect plays on. FUN_001dd0d0 then ends it at once only for
  elements with flag 0x200 (DESTROY_WHEN_OBJECT_LEAVES) or flag 0x400 (checked through FUN_001dd670); FUN_001dd060
  ends it as flag 0x80 says (stop like a loop end, FUN_001d9f48) or flag 0x40 (fade out over the element's
  `fadeout_frame` frames, FUN_001da080), once. `fx.rs` did the opposite (despawned the effect with its owner);
  this matters for the death effect, which outlives the hidden and removed body. The fade multiplies the effect's
  alpha through `Env::tint.w` (**inferred**: the game scales the instance alpha).
* The attached owner of `spawn_attached` (the sixth argument of FUN_0016a570, defaulting to the object) is the
  record's +0x48; only the checks above read it.

### Object bones 0x4000..0x4009 (FUN_0016aa18, WP-7A, verified)

A bone number with bit 0x4000 names a frame of the object rather than a bone (`fx::object_bone`,
`fx::ObjectFrame`):

| number | frame |
|---|---|
| 0x4000 | the object's matrix (+0x40) with its position (FUN_0016b960: +0x70 or +0x540) |
| 0x4001 | the object's matrix moved to the centre of its first shape of type 1, the body volume (FUN_0016b7d0 -> FUN_0016b558(obj, 1, -1) -> FUN_001894d0: the shape's offset in its joint's frame, y minus its +0xe); without one, the position +0x540 |
| 0x4002 | the model's world matrix (+0x6b0); the app does not tell it from 0x4000 |
| 0x4003 / 0x4004 | the weapon of hand 0 / 1 (FUN_0016ae50): for a party member (+0x588 & 4) the weapon object in its list at +0xaf8 (FUN_00175df8), for any other object the object itself (FUN_0016ae90 on it; both hands alike). FUN_0016ae90 takes the weapon's first shape of type 15: FUN_0016aa18 on that shape's joint gives the blade's matrix, the shape's x offset (+8, as a float) is the weapon's length. 0x4003 / 0x4004 then move the matrix along its own x axis by the length (`row3 += row0 * length`, w set to `DAT_0037a1b8`, 1.0) |
| 0x4005 / 0x4006 | the blade's matrix unmoved |
| 0x4007 / 0x4008 | moved by the length times `DAT_0037a1bc` (0.5) |
| 0x4009 | the object's matrix moved to the centre of its first shape of type 9; without one, unmoved |

On the disc the keyblade `W_EX010` has one type-15 shape on bone 2 with a negative x offset (the blade
points along -x); Sora `P_EX100` has a type-1 and a type-9 shape on his own matrix (joint 0x4000), lifted
above his feet. The charge and cast elements 0 and 1 of the four level-1 spells are bound to 0x4003, so
they start at the keyblade's tip (tests `fx::tests::object_bones_follow_the_game`,
`cast_effects_bind_to_the_blade_on_the_disc`). [inferred] The first record of a type is taken: the
records' enable mask (FUN_0016b558 skips records FUN_0019a6a0 refuses) is not modelled, and FUN_001894d0
gives a zero centre for a record on a plain bone while flag 0x400 of +0x7f8 is clear, which the app
does not model either.

## Death, hide and remove (WP-1F3)

* The hide-then-remove step: the script's `hide` ends in `enemy_ai.rs` (`obj.hidden` -> the enemy entity is
  despawned, the app's own rule; the game's rule for deleting a hidden object was not found). Before this package the
  effects bound to that body were despawned with it, which cut the death effect and the Soldier's `leave` effect
  (element id 1 of `M_EX010`, started by `spawn_attached(1, 1, 0)` of its `leave` mode and routed by `enemy_ai`
  through `StartEffect`) short. With the orphan rule above both play to their end. The Shadow's `leave` asks for
  id 1, which its file does not have (nothing starts, as in the game: FUN_001db868 returns 0).
* **FUN_001b25d0 (the victim's method +0x5c, "defeated") step by step**: (1) find the mode named `dead` in the
  mode table at +0x300 (FUN_001870d8) and switch to it (FUN_00186db0); (2) when the object has an attacker record
  (+0x48c) with a stat block (+0x254): if FUN_001b5148 holds (inferred: the last attacker is Sora, no
  cutscene lock) the payout is `ceil(table[+10] * stat block ushort +0x56 / 100)` where `table` is
  `DAT_0035f33c`; when the player is valid and has bit 0x80 of his word +0x588 and his HP fraction is at or
  below the float at `DAT_00360130 + 0x144`, the amount is multiplied by the byte at player stats +0x2b
  (inference: a low-HP bonus ability); FUN_001b51e0 adds it to the munny at `DAT_00340e48` (capped at 9,999,999)
  and loops over the 13 party slots (FUN_001b52f8; HP-orb or experience bookkeeping, not decoded), and when it
  returns positive a pickup is created (FUN_0016bbe8 builds the position, FUN_0015b198 spawns the 0xae0-byte
  prize object); (3) when the stat block's short +0x58 is non-zero FUN_0016b738 drops the ground marker
  (the same call as the script's `ground_marker`; the point is the object's position with y above the floor
  found by a probe FUN_001d1648, handed to FUN_001a3cb3), and when its short +0x5a is non-zero FUN_001be6b8 sets a
  progress flag (bit in the array at 0x340e6c, count at 0x340e68) and runs the flag's side effects
  (FUN_00180450); (4) FUN_00186568 -> FUN_00186580 sets the dead flag. **Gap**: munny, the pickup object, the
  HP orbs and the flag are not ported (the app has no munny counter or pickup object to hand them to); the
  stat-block offsets are named by position only. The `enmp` Prize and Experience columns are the probable source.

## Hit point (FUN_0018ce18, for `combat.rs`; not edited here)

`Hit.point` is `vol.a`, the attacker volume's base point. The game puts the effect at the point on the victim's
volume in the direction of the attack: with `C` the victim's volume centre (record +0x10), `r` its radius (+4)
and `h` its half height (+8), and `A` the attacker volume's centre (record +0x20 of the attacker):

    d = A - C
    dy = clamp(d.y, -h, h)
    t = |dy / h|                              (FUN_00300188 is fabs)
    flat = normalise((d.x, 0, d.z))
    point = C + (flat.x * r * sqrt(1 - t*t), dy, flat.z * r * sqrt(1 - t*t))

(`DAT_0037ac84` is 1.0.) It is a point on the victim's ellipsoid-like volume at the attacker's height, clamped to the
volume's vertical extent. Needed edit in `combat.rs::detect_hits` (the `Hit { .. point: vol.a }` line): use this with
the overlapping hurt volume's centre, radius and half height (`shape_volume` has `a`, `b`, `r`; the centre is
`(a + b) / 2`, the half height `|b - a| / 2 + r`, in Bevy space with y up; the clamp is symmetric so the game's
y-down axis does not matter) and the attacker volume's centre `(vol.a + vol.b) / 2`. The Y axis of the placement is
the one `fx.rs::Placement::hit` and `GameEffect::Damage` already handle.

## Tail3X draw flags and the draw-call budget (FUN_0021a660, read; gap)

The shape-draw routine reads the record `a1` (word at +2 = flags) and the shape frame `a0` (word 0 bit 8 = "uses
the alternate GIF tag", +2 = sprite count). What the bits do, from the code:

* record flag 0x80: draws into the **caller's own packet memory** (`record +0x14`) and first checks the budget
  `DAT_01db11f0 <= DAT_01db11e8 + size`: when the packet budget set by FUN_001e0168 (called from FUN_001e4350 with
  the instance memory size) is spent the draw is skipped and the routine returns `DAT_01db11e4`. The budget
  is the per-frame effect memory; the port has no packet memory, so the check cannot fire and nothing is
  skipped. **Gap by construction** (a frame that overflows the console's memory would lose sprites there).
* record flag 0x10: a longer packet header (3 GIF tags, 0xee8 against 0xe8 bytes of header reservation) and an extra
  register write 0x47 (the register was not identified); not visible in the port.
* record flag 0x100: the second set of GS register words (`DAT_01dbb3b0`) instead of `DAT_01dbb320` (a
  different TEST/CLAMP state; the words were not decoded).
* record flags 0x400 and 0x800: sprites fade or shrink with depth: a clamp to the ranges at `0x367030..0x367060`
  applied to the alpha and the sprite size (`vminibc/vmaxbc` on a plane equation of the view z). Which of the
  two constants sets is in force depends on the bits; **inferred** meaning: near and far fade distances. Not ported;
  the Tail3X entry bytes that set these bits (+0x50, +0x51, +0x52, +0x54, +0x55) are not interpreted, as
  before. Of the elements in `M_EX*` the effect that would show it is the trail fade at long range, which is
  not visible within the arena sizes the app has.
* sprite clipping: each sprite's corners go through the same `vclip` window test as the Sno draw (the 2048 +- 2047
  window and the near test); sprites outside are skipped. Whether `pppDrawShapeX` (the app's port) clips was not checked; the GPU clips the rest, so the visible
  difference is limited to sprites that straddle the near plane or the 4096 window (**inferred**).

## Sprite texture coordinates, palettes and the weapon trail (WP-FIX-9, verified)

The user saw hit sparks, spell sparks and ability particles as plain triangles and flat quads, and the sword
trail as a choppy stripe. Two port errors caused it; both rules below are checked against the game's own code
in `kh2-ee` (`crates/kh2-fx/src/oracle_tests.rs`).

### Sprites (FUN_0021a660, the shape draw behind pppDrawShapeX, the tails and MegaBirth)

* **Texture coordinates are fractions of the texture in 4.12 fixed point.** For every sprite the routine loads
  the four 16-bit values after the corners (s, t, width, height), converts them with `vitof12` (divide by
  4096) and writes them as the ST register of the GS packet; the packet's GIF tag sets PRIM 0x56 (sprite) or
  0x54 (strip, for frames with flag 8) with FST clear, so the GS reads ST as normalised coordinates. The
  sprite's ST are (s, t) and (s + width, t + height); a free quad gets all four corners. The value therefore
  does not depend on the texture's size. The port divided raw / 16 by the texture's width and height, which
  is the same only for a 256-texel side: on the 256 x 512 common page (`effect/texcommon.dpd`, where most hit
  and spell sprites live) every sprite showed the top half of its 32 x 32 cell (the glow ball as a half disc,
  the star as a "Y"), and on 128 x 64 pages (PRIZE, the Thunder arcs) a quarter cell or a neighbouring
  texture's palette area. Data check: the hit-spark glow sprite of `W_EX010` has corners 31 x 30 units and
  the raw rectangle (528, 16, 480, 464): / 4096 on 256 x 512 that is a 30 x 29 texel square cell, the old
  reading a 30 x 14.5 strip. `Sprite::st` holds the game's reading; `Sprite::uv` keeps the old unit (raw / 16)
  for its readers.
* **Per-sprite palette.** Byte 7 of a sprite's 8-byte color record is shifted to bit 37 and added (`paddw`) to
  the frame's TEX0 before it is written, so it moves CBP by that many blocks (`Sprite::tex0`). Bytes 0-3 are
  the color (times the caller's color / 128), byte 4 the ALPHA register value, byte 5 its FIX (unchanged).
* Test `shape_draw_texture_coordinates_match_the_game` runs FUN_0021a660 on a sprite frame and a free-quad
  frame (three sprites each, one with palette offset 3) over a 256 x 512 TEX0 and compares the ST floats and
  the TEX0 words it writes with the port's quads. The interpreter lacks PEXTLB / PEXTLH; a probe in the test
  does them (R5900 manual semantics).
* Not changed (difference noted): a sprite without flag 8 is a GS SPRITE between its two projected corners,
  axis-aligned on the screen; the port draws the four corners through the full matrix, so a sprite drawn with
  a rotation (MegaBirth particles with an angle) turns in the port and does not in the game [inferred from the
  PRIM value and the two XYZ2 writes; not compared in the oracle].

### The weapon trail (pppRyjDrawShipolyBone: frame 0x200a18 empty, draw 0x200d58, init 0x200fd8)

The draw calls FUN_00200a20 first (before its own alpha test). That routine allocates the ring on first use
(entry byte +0xe samples of the two points at `*0x3625a0`, both filled with the current pair), then **every
tick** rewrites the model's primitives in place: the r, g, b bytes of all four vertex colors to 0xff and the
quad indices to 2i..2i+3; the alpha bytes and the texture coordinates are not touched there. When the model has
2 x primitives + 2 vertices it calls FUN_002002f8, which:

1. stores the current pair at the ring head and builds, for each edge, the ring from newest to oldest minus
   the current first point (FUN_002fb348);
2. fits a natural cubic spline through those knots (FUN_00200000: `m[i-1] + 4 m[i] + m[i+1] = d[i] - d[i-1]`,
   second derivatives over 6, `c[0] = 4`, `c[i] = 4 - 1/c[i-1]`, back substitution) and evaluates it at
   `i / prims * (ring - 1)` for i = 0..prims (FUN_002001b8, Horner form with the constants 3 and 2 at
   0x367010; the last knot comes back unchanged); the results are truncated to shorts in the model's vertex
   buffer, edge e at vertex 2i + e;
3. on the first pass only (work byte +5, set by init, cleared when the head wraps) counts the stationary tail
   (trailing samples equal to their neighbour);
4. when entry byte +0xf is set, narrows the trail: vertex 2i becomes `v[2i] * t + v[2i+1] * (1 - t)`
   (FUN_002fb308) with t from 1 down by 1 / visible per sample, not below 0;
5. on the first pass only, writes the alphas (128 down to 0 over the visible primitives, start and end
   alpha per quad, 0 past the visible part; 128 is the float at 0x37e334) and, for type-6 primitives, the
   texture coordinates (u stepped across the visible part, v from 0x10 to the division's end - 0x10;
   FUN_00200238 reads the divisions +0x10..+0x12);
6. advances the head unless entry byte +0xd is set or the effect is paused.

Because the alphas and texture coordinates are written into the model and left there, after the ring has
filled once they keep the last first-pass values. The port rebuilt every primitive from the file each tick
with all four color bytes 0xff, so from the ninth tick of a swing (the keyblade trail `W_EX010` element 10 has a
ring of 8 and 64 primitives) the trail lost its fade and showed the file's texture coordinates (a sliver at
u = 0.98): the hard, choppy stripe. The port now keeps the first-pass values per object (`Obj::trail_prims`),
narrows on +0xf, and solves and evaluates the spline in the game's order of operations (a different but
equivalent formula rounded differently, which moved truncated vertices by one unit and with them the
stationary-tail count). Tests `trail_matches_the_game` and `trail_narrowing_matches_the_game` run FUN_00200a20
in `kh2-ee` for three ring lengths with the points held still for three ticks, then swinging, and compare
every vertex, alpha byte and texture coordinate.

### Decompile index (WP-FIX-9)

| address | what |
|---|---|
| 0x362b10 + 4 + 0x48 * kind | program table entry: name, frame, draw, init, reset/destroy handlers |
| 0x1fbcc0 / 0x1fbdb0 | pppDrawShapeX frame (animation step) / draw (record: flags, color, matrix; calls 0x21a660) |
| 0x21a660 | shape draw: clip, ST = raw / 4096, TEX0 + (color byte 7 << 37), ALPHA = color byte 4, packet tags |
| 0x200a18 / 0x200d58 / 0x200fd8 | trail frame (empty) / draw / init |
| 0x200a20 | trail ring allocation, per-tick model reset (rgb 0xff, indices) |
| 0x2002f8 | trail geometry, narrowing, first-pass alphas and UVs, head |
| 0x200000 / 0x2001b8 | spline solve / evaluate |
| 0x200238 | trail texture divisions |
| 0x2fb308 / 0x2fb348 | lerp `a t + b (1 - t)` / vector subtract |
| 0x3678a0..0x3678c0, 0x37e330, 0x37e334, 0x367000..0x36701c | clip window, 1.0, 128.0, spline constants (4, -1, 3, 2) |

Gaps: the model draw FUN_0020c7f8 (and its four packet builders 0x2114c0, 0x213540, 0x20c950, 0x20f950) was
not read; how it combines the 0xff vertex colors with the trail's material color is the port's earlier
reading (vertex color / 128 times material). The GS SPRITE screen alignment above is not ported.

## What is not done

* Hit-point accuracy: the rule is above; `combat.rs` needs the edit (not made, not in this package's ownership).
* Munny and item payout of FUN_001b25d0, the camera-bound effect bind (flag 0x20000), the depth fade of sprites
  (flags 0x400, 0x800) and the packet budget; see the sections above.
* The Sno lens samples the opaque pass only, bilinear, with a per-fragment displacement (see above).
* The `+0x17` depth-test-off mapping and the clock unit of the start wait are inferences.
* Category tests: all category-selected lookups are first-match (FUN_001db9e0); nothing else uses more than the
  first element.
