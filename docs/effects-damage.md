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

### pppSnoMdlDttNorm (amplitude verified, geometry inferred)

A model that shows the screen shifted along the model's normals: a lens or shock-wave ripple.
Verified (state and arguments, same oracle test `sno_amplitude_matches_the_game`): work 0 holds an
amplitude as three floats (value, rate, acceleration; init and reset zero them); every tick
value += rate += acceleration, and on the entry's frame the entry's floats at +8, +0xc, +0x10 add
to value, rate and acceleration. The draw is skipped for model 0xffff or a zero color alpha (byte
+0xb of work 1). The model is drawn by FUN_0020ab40 with the matrix in work 2 (a draw-matrix program
such as `pppKeDMatFr` copies its camera-space matrix there), the amplitude (times the bind's x scale
when entry byte +0x16 is set), the taper flag (entry byte +0x15) and the GS ALPHA value of entry byte
+0x1c (0 means 0x44). The handler also lowers the view matrix's z translation for the duration of the
draw (the value is restored afterwards, checked in the oracle test) by what reads as
`entry +0x18 * 1e7 / (matrix z)` (inferred from the soft-float calls): a depth bias, not ported.

Inferred from the disassembly of FUN_0020ab40 (the VU0 work was not run): the model's triangle
blocks (type 1: three vertex colors, three vertex indices, three normal indices) are drawn with the
frame buffer as the texture. For every vertex the displacement is the unit normal times the
amplitude; the normal is the direction from the model's origin to the vertex when the model's flag
word has bit 1 clear, else the stored normal (4.12 fixed) through the matrix. With the taper byte
set the displacement's x and y are multiplied by the unit normal's view-space z. The texture
coordinate of the vertex is the screen position of the displaced point and the triangle is
drawn where the undisplaced vertex projects, tinted by the vertex color times the object color, with
the entry's blend. The port produces exactly this as `Batch::offsets` and `fx.rs` draws it with the
material `DistortMaterial` (`fx_distort.wgsl`), which reads the scene drawn so far through Bevy's
transmission copy and blends by hand. What differs from the console: only what the opaque pass
drew is seen (not other effects drawn before the lens), the sample is bilinear, and the displacement
is interpolated per fragment instead of the screen position per vertex.

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
  played the keyblade's spark for every attacker.
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

## What is not done

* The VU0 geometry of `pppSnoMdlDttNorm` (FUN_0020ab40) was not run in `kh2-ee`; it is the inferred
  reading above. The depth bias is not ported.
* The shape-draw flag bits of Tail3X, the draw-call budget in FUN_0021a660 and the GS state are not
  modelled (shared with the existing `pppDrawShapeX` port).
* The rest of FUN_001b25d0 (munny and item payout), the hide-then-remove step after the `dead` mode
  and the Soldier's `leave` effect.
* Hit-point accuracy: `Hit.point` is the attacker volume's base point, not the nearest point on the
  victim's volume (FUN_0018ce18).
* The category tests use the first matching element only for the damage effect and the spark; effect
  starts by id start every element with that id (no file on the disc repeats an id, checked over all
  750 effect files).
