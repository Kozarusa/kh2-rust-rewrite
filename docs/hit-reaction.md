# Hit reactions: wall recoil, clashes, victim reactions

Findings from reading the game's main executable (decompilation plus disassembly of functions the
decompiler skipped). Addresses are function addresses in `SLUS_210.05` (USA). Nothing here is
copied code; the typed rules are in `crates/kh2-app/src/reaction_rules.rs` and their numbers come
from the user's disc at run time. `atkp` field names follow OpenKH `docs/kh2/file/type/00battle.md`.

Status words: **verified** = read directly in the code; **inferred** = follows from the code but a
name or meaning is a guess; **unknown** = not found.

## Short answer: why Sora recoils from walls in the original

Verified. The recoil is a generic rule of the actor class, not stored in the combo rows.

1. Every frame an attack volume is swept against terrain (FUN_001a2cc0 calls FUN_0018c8e0 when the
   volume has flag 4). The test only runs for rows with `Flags` bit 0 ("BG Hit"). It is skipped
   when the volume already touched a wall (flags 8 or 0x10) or when `(flags & 0x100) == 0 and (flags & 2) != 0`.
   If the swept segment misses and `FloorCheck` (0x28) is non-zero it tries again with the segment
   raised by that many units.
2. On a hit (FUN_0018ca78) the volume is marked (flag 0x10 if the surface type nibble is 1, else
   flag 8). A hit spark is spawned from `EffectOnHit` (variant mask 0x100 or 2 by that nibble). If
   the owner has flag 0x40 in its word at +0x588 the fixed sound 500010 (bank 500, index 10) plays.
   Then, if the volume lacks flag 0x100, the owner's virtual method +0x70 runs.
3. That method is FUN_00175668 in every vtable that was checked, including the party member class
   (vtable 0x35c9a8, constructed by FUN_00176e00, which also covers Sora). It sets owner flag
   0x2000000, decrements the combo counter byte at +0xb04 (FUN_00176aa0) and calls FUN_00185d30
   with power argument -1.0 and no motion override.
4. FUN_00185d30, unless the volume already has flag 0x40: requests motion ID 0x29 (41) when
   FUN_0019aa28 is false, ID 0x2b (43) when true, then pushes the actor. Negative power becomes the
   default 50.0. Ground push: FUN_001857b0, direction = minus the actor's own facing (not the wall
   normal), initial speed = (power + a per-actor term at stat block +0x1c8) * 0.1. Each logic tick
   (FUN_001858b0, called from FUN_00181e00) moves by the speed, multiplies it by 0.9, and stops
   below 1.0. For power 50 that is speeds 5.0, 4.5, ... over 16 ticks, about 41 game units in
   total. Air case: FUN_00185978 instead (jump-back push, scaled by `sstm` HitBackToJump). Last it
   sets volume flag 0x40 so the volume reacts once.
5. The motion request is not gated on the current motion, so it replaces the attack motion.

So a Sora swing whose `atkp` row has BG Hit set bounces off ATTACK_COLLIDE geometry whatever its
`RefactSelf` is. In the USA rows the Sora combo hits 126, 128 and 131 have BG Hit. Hits 127, 129,
130, 147 do not, and 87, 88, 124, 125, 148 have `RefactSelf` 2 and no BG Hit, so none of those
bounces. The user's observation is explained.

Implemented in the app (WP-2A, see "Implementation status" at the end).

The wall sound is played only when the volume's owner object has flag 0x40 in its word at +0x588
(FUN_0018ca78, the `& 0x40` test). Verified: the constructors FUN_00184190 and FUN_00184058 set that
flag, they take an owner (stored at +0x9f4) and register their motion with the owner's motion
component, so this is the attached-object (weapon) class; the party class sets 0x4 and 0x80
instead (FUN_001746e0, FUN_00176e00). FUN_00197008, which plays every 500xxx sound, then looks the
sound up in the bank named by the short at +2 of the record at actor +0x9b4: the object's own SE
bank if it has flag 0x40, otherwise (flag 4) the bank of the weapon the party actor holds at
+0xaf8. So for Sora every 500xxx sound, the wall sound and `ReflectHitSound` alike, is an entry of
the keyblade's bank (`obj/W_EX010.apdx`). Inferred: the volumes of Sora's swings belong to the
keyblade object; in the app the weapon body carries the attack triggers of rows 126, 128 and 131,
and a volume of the character body would play no wall sound.

Inferred: motion ID 41 is named REFLECT in the project's `kh2_anim::id` (from the game's debug
motion list, which I could not find again in the executable); 43 is not named there. FUN_0019aa28
reads actor flags (+0x160 bits 0x20 and 0x1000000, +0x108 bit 0x800000, owner type flag 1) and is
treated as "airborne". Its callers set +0x108 bit 0x800000 from +0x160 flags 0x20/0x1000000, which
are most likely the in-air states.

## Attack against attack ("clash")

Volumes of opposing teams are paired in FUN_0018d808 (team differs, and a 7x7 byte table at 0x37ac98
indexed by the two volumes' `Type` says whether that pair clashes). A pair is accepted only if
both sides pass FUN_0018d308 (WP-6E, `reaction_rules::reflect_cone`, test `reflect_cone_by_hand`):
when `ReflectRange` (0x22) is non-zero, the owner's facing (FUN_0016a890), turned about Y by
`ReflectAngle * 2` degrees when that byte is non-zero (FUN_0017c2e8, the same routine as script host
0.13), is dotted with the other volume's method +0xc (FUN_0018c4f8: the flat unit vector from the other
owner's +0x730 to this owner's position, or minus this owner's facing when the other volume has no
owner); the pair passes when the product is below -cos(range degrees) (degree factor 0x37ac8c, cosine
FUN_003000d8): the other owner stands inside the cone in front. The app takes +0x730 as the other
owner's position [inferred]. Then FUN_0018d6b0 spawns a spark (FUN_0018d560), which also
disables both volumes (flag 0x80, FUN_0018dc58) except one whose own `RefactSelf` is 2. FUN_0018d430
does the bookkeeping per side: it sets volume flag 0x200 unless that side's `RefactSelf` is 2, then
calls the owner's virtual method +0x6c with both volumes.

`RefactSelf` / `RefactOther` values (OpenKH): 0 Reflect, 1 Guard, 2 Nothing. Verified usage:

* Party class (+0x6c = FUN_00178708 -> FUN_001754a0). `A` is the actor's volume, `B` the other.
  If `A.RefactSelf == 2` or `B.RefactOther == 2`: spark only. Otherwise, when FUN_0019aa28 holds for
  the actor, or bit 2 of its mover's flags (+0xc, +8), or +0x160 flag 0x40, or when both are 0: the
  same recoil as a wall hit
  (FUN_00185d30, power -1.0, ID 0x29 / 0x2b, combo counter decremented). Otherwise (one of them is
  1, ground): the guard bounce FUN_001756c8 (motion 0x2a = 42, power 200.0 push facing away, then
  FUN_00175aa8 with 80.0). FUN_00178708 also copies `ReflectRC` (0x20) of the other volume into
  its owner at +0xaa4; the field is 0 in every USA row.
* Party class in detail (WP-5C, FUN_001754a0 and callees; ported for Sora in `player_reaction.rs`,
  rule function `player_reaction::party_clash`, unit test `party_clash_follows_fun_001754a0`):
  1. the actor's +0x108 bit 0x2000000 is set first in every case: the hit-landed latch the next
     attack press needs (`Sora::connected`);
  2. spark only: FUN_0018d560 with system effect 0xd (`00effect.bar`, holder 0x1c954d8, returned by
     FUN_001a21d0);
  3. recoil: spark 0xd, FUN_00185d30 with the power at 0x37a4a0 (-1.0, so the default 50.0) and no
     motion override, then FUN_00176aa0 (combo counter +0xb04 minus one, not below 0);
  4. guard bounce: spark 0xb when the actor's +0x588 has 0x80 (the party class sets it), else 0xc;
     then FUN_001756c8: FUN_0018cc48 gives the flat unit direction from the other volume's position
     (its method +8) to the actor (+0x730); the actor is turned along it times 0x37a4a8 (-1.0: it
     faces the other attack; FUN_0016a828, skipped when FUN_0019ad98 holds, not decoded); FUN_001857b0
     pushes it along the direction (away) with 0x37a4ac (200.0); FUN_0017e220 clears the motion queue
     and requests motion 0x2a; FUN_0017e378 queues motion 0 behind it; FUN_00175aa8 writes 0x37a4b0
     (80.0) to +0xb00. +0xb00 is the invincibility timer: FUN_00175ab0 (party tick FUN_00174b68)
     counts it down by the frame step, FUN_00175ad8 refuses hits while it is above 0, and Quick Run
     (FUN_00178108) writes its `fmab` star time there too (the app's `Sora::invuln`). Then the own
     volume gets flag 0x40.
  The port: motion via the state fallback in the battle state, blend 0 (FUN_00185d30 and FUN_001756c8
  clear `$f20` and pass it in `$f12` and `$f13` to FUN_0017e220: `reaction_rules::RECOIL_BLEND`),
  `State::Hurt` (back to idle when the motion ends, which is what the queued motion 0 gives; the air
  recoil queues motion 4 instead, not modelled). The spark is started (see docs/limits-reactions.md).
* The actor flags the party handler reads (WP-6E, `reaction_rules::ClashActor`, test
  `clash_actor_flags`): +0x160 is the controller's flag word (+0x140 + 0x20). FUN_00187fb0 ->
  FUN_0017f3d0 keeps only the bits 0x7183 of it at the start of the trigger pass, and FUN_00188030 sets
  the others while a range trigger covers the frame: 0x20 by trigger 3, 0x40 by trigger 24 (case 0x18,
  which also writes the float +0xf0), 0x1000000 by trigger 30, 0x100000 / 0x200000 by triggers 25 /
  26. So FUN_0019aa28 is "the mover's bit 1 (the app's airborne) or trigger 3 or trigger 30 or +0x108
  bit 0x800000", and the flag 0x40 of FUN_001754a0 is "trigger 24 covers Sora's frame". The mover's
  bit 2 and +0x108 bit 0x800000 were not traced and read as clear.
* Enemy-style class (handler at 0x1b2478, not in the decompilation, read from disassembly). First,
  in every case: FUN_001b21a0 stores the other volume's owner at +0xae4 (and at +0xaec when it is
  in the object list with +0x3b0 == 1); when the actor has a script thread (+0x484), FUN_00187130
  runs the running mode's hook in slot 8 (FUN_00187040 with 7) with (context +0x88, own volume, other
  volume) on the mode machine at +0x300 and
  FUN_0016c620 delivers **script event 0x13 (19)** with the two volume objects as arguments. Then, if
  the volume has flag 0x40 or `A.RefactSelf == 2`: nothing more. Else if `ReflectedMotion` (0x15) is
  non-zero: FUN_00185d30 with power `ReflectHitBack` (s16, negative = 50.0) and that motion ID as the
  override, then FUN_00186ca8 on +0x300 (stops the running action unless its +0x9c bit 1 is set). If
  `ReflectAction` (0x18, s32) is non-zero (checked after the motion, not instead of it):
  FUN_0019e870 turns it into a pointer, row + `ReflectAction` (an offset from the atkp row itself,
  into the table's data), and FUN_00186db0 starts that record on the action system at +0x300. Then
  volume flag 0x40 is set. Not ported: the action records (the format behind the offset and the
  action system at +0x300 were not decoded). The script part (slot 8 hook, event 0x13) is ported
  since WP-6G (`Runtime::clash`, enemy_ai.rs; docs/enemy-ai.md).
* USA data: `RefactSelf` is 2 in 1803 rows, 0 in 521, 1 in 2; `RefactOther` is 1 in 1487, 0 in 583,
  2 in 256; `ReflectedMotion` is 0 except 10 (one row) and 140..143 (187 rows); `ReflectHitBack`
  is only 0 or -1; `ReflectRC` is always 0; `ReflectRange` is set in 70 rows, `ReflectAngle` in 7.
* Sora's rows 87, 88, 125 and 148 have `RefactOther` 1: when such a Sora attack meets the attack of
  a party-class actor, that actor takes the Guard branch (if its own row is not 2 and it is on the
  ground). The enemy-style handler never reads `RefactOther`; no other reader of it was found.

### Lifetime of the clash spark (WP-FIX-14)

User report: after a clash a rainbow bubble sphere with a bright white core stays in the world. Cause: system
effect 0xd (the spark of "spark only" and the recoil) is a **looping effect** (its program file's flags have
bit 0; element flags 0, so no 0x40 / 0x80 own end) and the clash spark has no owner (`FUN_0018d560` passes 0 as
the sixth argument of `FUN_001db868`), so nothing of the effect's own ends it: `FUN_001e5660` copies the flag to
the manager's +0x84, `FUN_001e6240` then finishes a manager only when +0x85 is set (loop end,
`FUN_001e0298`, called by `FUN_001d9f48`), and the groups' loops (`FUN_001e5b70`) run on while +0x85 is
clear. The app ran 0xd forever (unit test `fx::tests::clash_spark_0xd_has_no_end_of_its_own`; the guard-bounce
sparks 0xb and 0xc are not looping and end within about a second).

What the game does about it: `FUN_001c09f8` keeps a **ring of the last 8 spark effects**
(`DAT_01d5df30`, index `DAT_01d5df50`). Every spark registers there: the hit spark and the ineffective
effect 0x12 of `FUN_0018d090`, the wall spark of `FUN_0018ca78`, the clash spark of `FUN_0018d560` (the
victim's damage effect is not registered). The new effect takes the slot of the oldest; the effect that was in
that slot gets `FUN_001da080(DAT_0037bd90 = 4.0, effect)`: loop-end flag, fade out over 4 frames (a time of 0 or
less would kill it at once, `FUN_001da028`). `FUN_001c09a0` only clears slots whose effect is already gone;
`FUN_001c0968` empties the ring. A second ring of 4 with fade `DAT_0037b450` does the same for another effect
class (`FUN_001a2438`, not ported). Ported as `fx::SparkRing` / `fx::ring_push` (marker `Evicted`, faded in
`fx::tick` with the same fade path as element flag 0x40); the app's hit sparks, wall sparks, effect 0x12 and
the clash spark all go through it. Tests: `fx::tests::spark_ring_pushes_out_the_oldest_after_eight`,
`ring_push_marks_the_pushed_out_spark`.

Consequence, read from the code: a clash spark leaves when 8 further sparks (any hit) have happened after it,
so during a fight it vanishes within a few hits. A lone spark with no later spark stays, as far as the decompiled
code shows. [inferred] that the original behaves the same; I found no other writer of the end flag for an
ownerless system effect (callers of `FUN_001d9f48`, `FUN_001da080`, `FUN_001e0298` searched: 0016dbb8, 0016dc48,
00175cb0, 001864b8, 001a2438, 001c09f8, 001cd880, 001dd060, 00226188, 00227978, 002cc6a8, 002f7dc0, 002f7df8; the
owner-bound end in `FUN_001dd0d0` needs the record's owner at +0x48, which is 0 here). If the user still sees a
sphere stay after a fight ends, the next step is a kh2-ee trace of the holder's record list over time.

**[inferred] Added on top of the decompiled rule:** a looping spark registered in the ring (`fx::RingSpark`,
in practice only the clash spark 0xd) also fades out once its program Life (`pd.life / 0x1000` = 60 frames) has
run, over the same 4 frames (`fx::ring_spark_expired`, test `fx::tests::lone_clash_spark_is_gone_after_life_plus_fade`).
Reason: the decompiled rule gives a lone spark no end, and the user reports a clash spark that stays on screen;
the user's report outranks the reading. Not found in the original code; remove it if a trace shows the original
keeps a lone spark. The ring cap stays. Harness: one forced spark goes from "running effects 1" to 0 about 2 s
after it starts (the 60 frames plus load of the log's once-a-second sampling).

Harness: `KH2_CLASH_FORCE_SPARK=<s>,<s>,..` starts spark 0xd in front of Sora at those times; `KH2_FXCOUNT=1`
logs `fx count: t=<s>s running effects <n> ids [..]` once a second.

## ReflectHitSound

Verified (FUN_0018c6d0, the hit record builder): when a hit lands, the attacker's object plays
`ReflectHitSound` (0x1c) if it is not negative, and a victim-side spark from `EffectOnHit` is
spawned (variant 0x400 or 0x800 by victim flag 0x80 at +0x588). So it is the landed-hit sound
(Sora's rows use 500000..500011), not the wall sound. No other reader of that field exists in the
main executable. It is skipped for ineffective hits and when the victim's status word has 0x2000
(meaning unknown). The sound number goes through FUN_00197008 (see above), so it comes from the
keyblade's bank for Sora. The Shadow rows (atkp 9..12) have `ReflectHitSound` -1: Heartless
attacks play nothing from this field.

## Victim reaction

Receive path (verified order): FUN_0018cf50 builds the hit record (FUN_0018b618 / FUN_0018c6d0),
spawns hit effects (FUN_0018d090: `EffectOnHit`, `RandomEffect`, `DamageEffect`; effect 0x12 and no
reaction when the hit is ineffective), calls the attacker's method +0x74 (party class: FUN_00178748
gives drive gauge from `DriveDrain` and sends event 0xf), then the victim's method +0x64. The
receive function FUN_001b21f0 plays the victim sound (FUN_001b2378), applies damage (FUN_00185a68,
victim method +0x78), converts the reaction by state (FUN_001bff00, below), and runs the reaction
dispatcher (+0x68 -> 0x184e80).

Hit record fields used: +0x00 direction (attacker to victim, y zeroed, normalised, FUN_0018bac0),
+0x10 attacker object, +0x14 victim, +0x18 flags (1 reaction done, 2 damage done, 4 ineffective),
+0x1c attacking volume, +0x20 the `atkp` row, +0x24 a byte passed in by the collision test (meaning
unknown), +0x26 effective reaction, +0x27 original `EnemyReaction`, +0x28 damage. Damage is zeroed
and flag 4 set when the victim's element multiplier byte times its general multiplier is 0
(FUN_0018b618 / FUN_001a0610); that is the "ineffective" case.

Reaction classes (FUN_0018b700 collapses `EnemyReaction`, then a jump table at 0x37a900 picks the
handler; `reaction_class` in the module):

| raw `EnemyReaction` | class | handler |
|---|---|---|
| 0 | 0 | nothing |
| 1, 12 | 1 | push only (FUN_00185098) |
| 2 | 2 | flinch motion only (FUN_00185198) |
| 3, 6, 8, 10, 13, 16, 18 | 3 | flinch plus push (FUN_001850c8) |
| 4, 7, 9, 11, 15, 17 | 4 | blow (FUN_00185460) |
| 5 | 5 | FUN_00185668 -> FUN_001bf920: the pull toward the attack object (motion 20 / 21, `atkp` +0x10 the swirl); WP-7M2, below |
| 14 | 14 | motion 0x17 (front) / 0x18 (back) plus push (FUN_001856a8) |
| 19 and above | same | no handler |

For classes other than 0 and 2, FUN_00184e80 first calls FUN_001becb8 when the global DAT_0035f33c
is non-zero (a state release on the victim; not decoded). Every
reaction ends in FUN_001caeb0, which for the victim not flagged 0x80 scatters orbs from a stock
(3 for classes 0, 2, 3; 15 for classes 4, 5, 14); not decoded further.

Sizes (verified, FUN_0018b7e8, FUN_0018b9e0). Push strength for classes 1, 3, 14: `KnockbackStrength1`
if the row is present and it is not 0x7fff (so 0 gives no push), else the `sstm` default (HitBack
for 1 and 14, HitBackSmall for 3). When the effective reaction is 12 or 13 the strength is
halved. Class 4: the launch is `KnockbackStrength1` upward and `KnockbackStrength2` horizontal
(the opposite of what the current app assumes), or the `sstm` defaults BlowUp and BlowSpeed when
both are 0. The victim faces the attacker when hit from the front (victim facing . hit direction
< 0) and plays motion 16, from behind motion 18; in both cases it is pushed along the hit
direction. The push uses the same decaying mechanism as the wall recoil.
Class 5 (WP-7M2, verified; the full rule with the constants' addresses is in docs/magic.md
"Magnet"): FUN_001bf920 keeps the attack object at victim +0xaac, draws the hold distance at random
between `KnockbackStrength1` and `KnockbackStrength2` (+0xab0), keeps the row's `Unknown` (s16 at
+0x10) as a swirl (+0xab4), plays motion 0x14 when hit from the front, else 0x15 (+0xab8), faces the
attack object (away from it for 0x15) and switches the mover to 14 (15 with mover flag 2 or +0x160
bit 0x40). Each frame the mover's velocity method (vtable 0x35f348 +0x1c, FUN_003332f0 ->
FUN_001bfc10) moves the victim's body centre toward the attack object at `(distance - hold) * 0.1`
units per frame, held within -20..20, plus the pull turned a right angle about Y, flattened, made unit
and times the swirl; its end method (+0x18, FUN_001bfba8) re-faces it, or, once FUN_0018ccf8 (the
attack object finished) has cleared +0xaac, returns to the default mover with motion 0 (blend 8).
The earlier reading "minus the distance to the attacker" was wrong: the distance is to the attack
object and enters the speed, not the hold. Port: `reaction_rules::Pull`, `PullConsts` (tests
`pull_follows_fun_001bfc10`, `pull_numbers_from_the_disc`), played by `enemy.rs` for spell attacks.
`victim_outcome` still returns `NotDecoded { class: 5 }` (callers turn it into the pull; Sora's
receiver does not).

Flinch motion (class 2, ground class 3; FUN_00185198): ID from the u16 list at 0x35ce40
(10, 11, 12, 13). Index starts at 2 (behind), is 0 when the hit is from the front, and gets +1
when the hit record byte +0x24 is not one of 0, 2, 3, 4, 5. If the chosen slot has no motion, the
other of the pair is used. Class 3 in the air uses the air flinch (FUN_001853c0), and with owner
type flag 0x10 a different routine (FUN_00185410).

Blow flight (verified, FUN_001855a8 sets the launch, FUN_0019e468 the floor, FUN_0019e4d8 runs it
every logic tick until the actor lands). Units are game units per tick, the same as pushes and the
jump model's gravity. The launch velocity is the hit direction (y zeroed, normalised) times the
horizontal speed, plus the upward speed. Each tick: the horizontal speed is multiplied by `sstm`
BlowBrakeXZ and is not allowed below BlowMinXZ times the launch speed; a rising vertical speed is
multiplied by BlowBrakeUp; then gravity 20/49 (0x37b408, equal to the jump model's) is added and
the fall speed is capped (FallMax); then the actor moves by that velocity. With the USA `sstm`
(BlowBrakeXZ 0.8, BlowMinXZ 0.1, BlowBrakeUp 0.8, defaults BlowUp 24, BlowSpeed 40) a default blow
rises about 0.7 m and lands after 30 ticks (0.5 s); row 129 (KnockbackStrength1 3, 2 45) is a flat shove.

Sora as a victim (verified by reading the receive path): the party class's method +0x64 is
FUN_00179038 -> FUN_00175340, not the FUN_001b21f0 the enemy classes use. It runs the `patn`
conversion (FUN_001bff00), the damage (FUN_00185a68, +0x78 = FUN_001792f0 -> FUN_00176c98) and the
same reaction dispatcher, then calls FUN_0013dd20 / FUN_0013dd70 / FUN_0013ddc0 on the object at
0x355768 with 1 (other classes), 2 (class 3) or 3 (class 4). Those play no sound; they look like
the pad vibration request (inferred; frame trigger 31 is vibration). The path never calls
FUN_001b2378, so Sora has no victim hit sound of the kind enemies have, and the Shadow attack rows
have no `ReflectHitSound`: the data gives Sora no hit sound when he is hit. Gap: whatever he
sounds like then comes from his motions (A200..A213 carry no sound trigger) or from the sound of
the attack's `EffectOnHit` effect.

State conversion (verified, FUN_001bff00; table `patn` of 00battle.bin, 15 records of 32 bytes
after an 8 byte header: status bit, then a replacement per original reaction, 0xff = keep). For
each record whose bit is set in the victim's status mask ((word +0xabc | +0xac0) & ~word +0xac4)
the effective reaction (+0x26) is replaced, later records winning; the lookup uses the original
reaction. Not applied when the hit is already ineffective. Bit 7 records apply only when the
victim is grounded, bit 8 only when it is not, bit 2 only when the attacker has flag 0x4000.
Examples from the USA table: bit 14 maps reaction 5 to 3; bit 12 maps blows (4, 5, 7, 9, 11, 15,
17) to 14; bit 1 cancels reactions 3..=11 and 15..=18. What sets each status bit (armor,
abilities, states) is unknown.

## Hit-stop

Not found. What was checked:

* A scan of the whole main executable for loads of `atkp` fields through the row pointers found
  readers only for: `RefactSelf`, `RefactOther`, `ReflectedMotion`, `ReflectHitBack`,
  `ReflectAction`, `ReflectRC`, `ReflectRange`, `ReflectAngle` (clash code above),
  `ReflectHitSound`, `EffectOnHit` and friends (effects), `Flags`, `FloorCheck`, `Interval`
  (repeat-hit timing, 0x1a47e4 and 0x1a4df0), `Revenge`, `ComboGroup`, `AttackTrReaction`
  (victim sound choice), `KnockbackStrength1/2`, `DriveDrain`, and the `Unknown` s16 at 0x10,
  which only the class 5 handler reads. None of them starts a freeze.
* Neither the attacker callback (+0x74), the damage application, the reaction dispatcher nor the
  hit record builder touches a frame counter or motion speed.
* The `stop` table (15 rows of id and u16 flags) is looked up per motion request (FUN_001d5ab0 from
  FUN_0017dd38): it looks like per-motion interrupt flags, not freeze frames (inferred).
* OpenKH lists frame trigger 31 as vibration; the `vbrt` table is not modelled yet.

So the rule is either in code that was not reached (overlay code for enemy classes, camera/time
code), in motion trigger data, or it does not exist in the generic pipeline. Evidence stops here.

## Implementation status (WP-2A)

Code: `reaction_rules.rs` (pure rules, unit tests), `combat.rs` (wall sweep, messages, reaction
data loaded from the disc), `sora.rs` and `enemy.rs` (the reactions), `audio.rs` (wall sound).

Done:

* Wall test only for rows with BG Hit, after the sweep against hurt volumes and not for a swing
  that already hit a target (FUN_001a2cc0 sweeps targets first; FUN_0018c8e0 skips a volume with
  flag 2). `KH2_HITLOG=1` logs the touches that the BG Hit rule rejects. In tt00 (Sora starts
  against a wall): ground hit 1 (A300, atkp 126), hit 3 (A302, 128) and the first air hit (A330,
  131) recoil; ground hit 2 (A301, 127) and the finisher A304 (129) touch the wall and do not.
  Rows 130, 147 and 148 were not seen touching a wall; the rules test checks their flags.
* Wall recoil as above: motion 41 (A320), or 43 (A350) in the air, combo counter minus one, push 50
  backwards along the facing (initial speed 5.0, 16 ticks, about 41 units: Sora's z went from -35
  to -76 in the run), sound 500010 from the keyblade bank (`se 500:10 (obj/W_EX010.apdx) playing`),
  spark from `fx::hit_sparks`. Each decision is a log line (`wall recoil: ...`).
* Victim reactions by class for enemies and Sora (`reaction_rules::victim_outcome`): none, push,
  flinch, flinch plus push, blow (KnockbackStrength1 up, KnockbackStrength2 horizontal, with the
  flight above, motion 16 or 18, faces the attacker when hit from the front), class 14. The push
  strength follows FUN_0018b7e8 (0x7fff selects the `sstm` default, 0 means no push, reactions 12
  and 13 halve it). Examples seen in the app: Sora's hit 126 flinches a Shadow with a push of 125,
  hit 128 with 75 (motion 10, hit from the front); row 124 (reaction 0) does nothing; row 129 blows
  with up 3 and horizontal 45; the Shadow rows 9 and 11 (reaction 3, KnockbackStrength1 0x7fff)
  push Sora by the `sstm` default 200.
* The Dusk (WP-6C, `docs/enemies.md`) goes through the same code with no change: its set has the
  default reaction slots (10..13 flinch, 16..19 blow and landing). Seen in the 6C run: Sora's hit 127
  flinches a Dusk (reaction 3, push 125, motion 10); last hits blow it and it lands
  (`Dusk lands from a blow`); its kick (atkp 17, reaction 3) flinches Sora with a push of 350; Sora's
  swing against its kick clashes (`Dusk clash: recoil motion 140`).
* N_EX570_BTL (WP-6H) uses the default slots too (10..13 flinch, 16..19 blow); seen: Sora's 128 flinches it (push 75), last hits blow it; its claw 1042 (reaction 3) flinches Sora with a push of 200. Damage is held at its lowest HP (FUN_001a0100).
* WP-7D: M_EX930 (the Dancer), N_EX500_BTL and N_EX600_BTL have motions in the default slots (10..13 flinch, 16..19 blow) and use them unchanged; seen: Sora's 128 / 126 / 147 flinch them, 129 blows them (`Dancer lands from a blow`); the Dancer's spin 440 (reaction 3) flinches Sora with a push of 100, N_EX600_BTL's 168 (reaction 4) blows him. The scripts of the two Struggle opponents read the effective reaction of each hit (host 2.7), which `AiHit::reaction` now carries.
* The `patn` conversion and the ineffective-hit rule run before the class handler. No status
  bit is ever set in the app (the `ReactionStatus` component, absent by default, carries one), so
  the conversion is exercised by unit tests only. `combat::IneffectiveHit` is written by both
  receivers when the victim's element multiplier times its general multiplier is zero (no victim
  in the data has that); it is the hook for effect 0x12 and carries a TODO for fx.rs.
* The attacker plays `ReflectHitSound` when its hit lands (not when ineffective).
* Enemy hit sounds alternate per victim instead of per run, and are skipped for ineffective hits.

Approximations that are the app's, not the game's: the invulnerability timer after Sora is hit
(36 ticks; the game's rule was not found), the last hit launching the victim whatever the row says
(the game kills through other code), airborne class 3 using the ground flinch and push, the
airborne wall recoil using the ground push, and a Sora hit that the game would not interrupt
(classes 0 and 1) leaving his state alone.

## What stays unknown

* Hit-stop: duration, who freezes. Nothing found.
* Whether overlay classes (enemies) override the wall method +0x70 or the clash method +0x6c with
  more than the base behaviour. The vtables in the main executable that were checked all use the
  base wall method; enemy objects defined in overlays were not examined. Enemy wall recoil and
  the clash rules are not implemented.
* Meaning of: the hit record byte +0x24 (the app assumes the plain value); the actor stat block
  value +0x1c8 (added to every push power; the app adds 0); owner word +0x588 flag 0x80; the
  status bits behind `patn`; the surface type nibble (1 versus other) used for the spark variant;
  volume flag 0x100.
* The air recoil push FUN_00185978 (WP-6E, decoded as far as this): only when FUN_0019b1d0 holds
  (FUN_0019a470, or +0x588 bit 0x8000000, and in both cases +0x108 bit 0x80 clear) it calls method +8
  of the mover (+0xc) with the vector (dir.x, 50.0 (0x37a960), dir.z, power x the float at +0x38 of the
  table DAT_00360130), dir = the facing times -1.0 (0x37a968). The table pointer is set at run time
  (FUN_001cc040 -> FUN_001cc140), and the mover's method +8 was not read, so the push is not ported (the
  ground push stands in). Also open: the air flinch (FUN_001853c0), class 5 for Sora as the victim (enemies: WP-7M2), the orb scatter, and the
  `FloorCheck` probe of FUN_0018c8e0 (a second sweep with the segment moved by that many units when the
  first misses; every Sora combo row has FloorCheck 0).
* The recoil's motion requests: blend 0 (closed by WP-6E, above). FUN_0017e378 then queues motion 0
  on the ground and 4 in the air.
* Motion 43 (0x2b) exists in Sora's motion set: it is A350 (slot 172).
