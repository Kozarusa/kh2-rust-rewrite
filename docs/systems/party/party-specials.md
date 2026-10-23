# Party: the specials of Goofy and Donald

[Contract](contract.md) | [Index](../../README.md)

## 15. Status: the specials' attack objects and effects (WP-S3)

Files: `friend_host.rs` (the hosts), `friend_ai.rs` (the attack-object pass, the effects), `party.rs` (the
status numbers, the script motion, the target point). Log `KH2_PARTYLOG=1`: `attack object 0x7a000001: atkp
1165 group 0x101 at .. radius 160 height 160 for 20 frames`, `attack object .. hits <enemy>: atkp .. SubId ..
(hit n, Power .., Element .., Interval ..)`, `released after its 20 frames`, `destroyed (FUN_0018c2d8)`, `effect
element 8 of obj/P_EX020.apdx (At { .. }): started`; `KH2_HITLOG=1` prints the damage (`Dusk hit: atkp 1165
damage 2 ..`). Harness `KH2_PARTY_ABILITIES=<item>,..` counts ability items as equipped on the members (the
new game holds Donald Thunder 167 and Cure 168 only; Fire 165, Blizzard 166, Goofy Tornado 423, Turbo 425,
Bash 429 from the table 0x352ec0).

**Attack objects** [V unless marked]. Host 2.10 (0x18e2f0 -> FUN_001a2ba0 -> FUN_0018bd08): an object of
0x140 bytes in the global list `DAT_01c89b58` (+0xc0 the next): +0x14 the `atkp` id (argument 1), +0x18 the
sub key (argument 3; FUN_0019e788 picks the row), +0x20 the hit group (the row's ComboGroup +0x2c when not 0,
else argument 2, else FUN_001bdf00's next), +0x1c a serial (FUN_001bde48). 2.31 (0x18e488 -> FUN_001a2c58):
a new object with the given `atkp` id and the first one's owner, sub key, group, serial and anchor +0x24 (its
size and growth start cleared). 2.11 (FUN_001a30e0): radius +0x100, half height +0x104 (the radius when not
positive). 2.12 (FUN_001a2fc8): +0xf0 = the old position once the object has one (+4 bit 1), +0xe0 = the new
one, +4 bit 4 when it moved more than two frame units. 2.62 (0x18e680 -> FUN_001a3480): +0xc8 = 0, +0xcc =
frames, +0xd0 / +0xd4 the start size (the final one when not positive). 2.13 (FUN_0018dc98): +4 |= 0x800.
Each frame the task FUN_0018c2d8 (registered by FUN_0018c3f0, priority 21000) destroys every object with
0x800 (vtable +4) and runs every other one's +0x10 = FUN_001a2cc0 (vtable 0x35e788): with +0xcc > 0 the clock
+0xc8 gains a frame (`DAT_00356f40`, not while the pause bit 0x20 of 0x35d268 is set) and past +0xcc the
object is released (no test that frame); the tested size is `start x (1 - s) + final x s`, `s =
sin(t / frames x pi x 0.5)` (FUN_00300278 = sinf, `DAT_0037b45c` pi, `DAT_0037b460` 0.5); the volume
(FUN_00189610, a sphere when radius = height) is tested at +0xe0 and, with bit 4, in steps of `min(radius,
height)` toward +0xf0 while more than half a step is left (`friend_ai::sweep_points`, test
`member_attack_sweeps_toward_its_previous_position`; growth: `member_attack_grows_from_its_start_size`). An
object with +0xc4 (2.61, an effect it follows) takes the effect's position first; no member script on the
disc uses it in the specials done here. [inferred] The victims are the enemies' hurt shapes (the member's
team, +0x3b0, is the player's); FUN_0018d768's own test and its early stop were not read: as `magic.rs`, every
victim is tested at every sweep point and the first overlapping point is taken. The victim's hit memory and
the row variant are `magic_rules::HitMemory` / `variant` (FUN_001be108 / FUN_001be190 / FUN_001be300,
FUN_0019e7e0), kept per (member, victim) [inferred]. A hit is a `combat::Hit` with the variant's SubId
(`Hit::row`), so `enemy::receive_hits` does the damage and reaction, and an `fx::SpellHit` with the member's
own effect file (2.22 anchors the attack to the object's +0x80, the file FUN_0018d090 takes the spark from).

**Effects.** 1.87 (0x171800 -> FUN_0016a570(obj, id, a, b, 0)) and 1.149 (0x171840 -> FUN_0016a570 with the
third argument's object as the owner) start the first element `id` of the object's own file
(`FUN_001db868(obj + 0x80, id, a, 1, b, owner)`) bound to the owner; 1.22 (0x1717c0 -> FUN_0016a5b8:
FUN_0016a530, no owner) starts it and overwrites its position (FUN_001d9d00) with the vector. The member's
file is `obj/<model>.apdx` (`spawn_party` registers it). The handles answer 0.49 / 0.50 (the instance runs),
0.53 (FUN_001d9d00: for a bound element its bone frame times its translation, `fx::object_bone`; one tick
old), 0.38 (placed anew), 0.44 / 0.85 (stop), 0.51 (destroyed); [inferred] 0.52 (fade) is taken as a stop.

**The target record.** 1.120 answers the centre of the hurt shape the member targets (`party_tick` keeps it in
`target_point`) [inferred: the record's point]; 1.141 is the floor under the point (`World::ground`, not
FUN_001a5c30's mask-4 cast). The record's word 0 (the object) names what FUN_00113d48 copied: the member itself,
the player, the other member or the enemy (`friend_host::read_mem`), so Cure's 1.149 binds element 12 to the one
healed and 1.130 (0x170510, the objects' difference) tells healing itself from another.

**Donald's specials** (`kh2 bdx P_EX020`; modes registered at R0206..R0296: main / exit / enter routines).
- `thunder` (R13c1 -> R13c8): voice 6 (1.262), motion 192 then 0 queued, the cast trigger (R089a: bit 0 of
  +0x239), element 7 on Donald, 10 frames, MP (1.276), the target's point (else 300 ahead), a ground probe from 80
  above: ground less than 160 below -> element 8 at the point, else element 9; attack 1165 at the point, radius
  160 for 20 frames (2.62 (20, 160)), and its copy (2.31) radius 25, half height 320, 320 above, for 20 frames;
  the mode waits 20 frames. Log (b_44, tt04): `attack object 0x7a000001 hits 2605v0: atkp 1165 SubId 0 (hit 0, Power
  100, Element 3, Interval 8)` and `Dusk hit: atkp 1165 damage 2 hp 30 ..`; every Dusk in the radius is hit, again
  after the row's Interval (8 frames): three hits in 20 frames.
- `fire` (R07d3 -> R07e1): voice 6, motion 192, the trigger, MP, element 0, 4 frames, element 1 kept at the
  context +0x18, attack 1163 at +0x1c anchored, radius 220 half height 70; for 60 frames while element 1 runs the
  attack follows its position (0.53 -> 2.12); then released (R0967), and the mode waits for the element's end.
  Log: `hits 2731v0: atkp 1163 SubId 0 (hit 0, Power 50, Element 1, Interval 12)`, then `SubId 2 (hit 2, Power
  125 ..)` (FUN_0019e7e0's variant) with a blow.
- `cure` (R1552 -> R1559): as in section 14; its elements 11 (on Donald) and 12 (on the one healed) start.
- `blizzard` (R0a0b -> R0a19): voice 6, motion 191, the trigger, MP, element 3, then a request thread (2.20) at
  word 0xaf8 (a homing projectile library, listed with `cargo run -p kh2-bdx --example listing -- P_EX020 af8`)
  that needs hosts 1.21, 1.88, 1.160, 2.19, 2.29, 2.38, 2.39, 2.43, 2.44, 2.57, 6.9, 6.51, 6.54, 0.41, 0.45, 0.46.
  (Done in WP-S5, section 16.)

**The caster's numbers** [V]. FUN_001a0610 reads the attacker's status +0x188 / +0x18a, which FUN_0019f920 rebuilds
from the saved record: the `lvup` row (byte +4 strength, +5 magic, +6 defense) of the record's level (+0xf) in the
table FUN_001b50c0 picks (FUN_001b58d8(NeoStatus): 1..15, 14 -> 1, 15 -> 6, else 0 -> 1; the first table when the
index is past the count or its offset is 0; the level held at the table's count), plus the record's bytes +9 / +0xa /
+0xb, plus FUN_001a0c08 for the two weapon words, the armor and the accessory slots (the `item_stat` Attack / Magic /
Defense). `PartyData::member_status`; test `party_member_status_from_lvup_record_and_equipment` (Sora's come out as
the status page's 5 / 7 / 2). Donald at the app's start: magic 6, so Cure (atkp 1525, type 6, Power 200) heals
`(6 x 200 + 99) / 100` = 12 (log `party: P_EX020 heals Sora with atkp 1525 (type 6, FUN_00185a68): +12, HP 4 -> 16`).
[inferred] A level of 0 is read as 1. A member's defense when an enemy hits it (`party_receive_hits`) is the same
rebuilt +0x18c (Donald 2 at the app's start instead of the record's 0). **Gap**: the members' motion-trigger and attack-object hits on enemies are
damaged by `enemy::receive_hits` with `enemy::member_stats` (the record's bytes only, no `lvup` row and no
equipment): thunder does 2 on a Dusk where the status would give more (outside-ownership change, report).

**The script's motion** (fixes a member stuck in a special): a motion request of the script (1.11, FUN_0017e220)
starts the motion again even when it plays, and loops only when its data has a loop range (as `enemy_ai`); 1.13
(0x1703d8 -> FUN_0017e170, the controller's queue +0x3c) plays the queued motion once the requested one has ended
[inferred: one queue entry, the member's blend]. Before, the cast motion 192 kept looping after a special, and the
next special's request of the same slot did not restart it: the cast trigger never came and the mode waited forever.

**Member handles.** The members' script handles were `SELF_BASE + (0x1000 + slot) x OBJ_SIZE`: slot 0 (Donald) is
0x0200_0000 = `PLAYER`, so every host arm that tests for the player took Donald's own calls as the player's (his
script's contexts named the player; 1.130 and 1.149 showed it). They start at 0x1001 now.

**Goofy's specials** (`kh2 bdx P_EX030`: `tornado` main R07af, `turbo` R0b4e, `blade` R0d30).
- `tornado` (R07af -> R07bd) [V for the script, the hosts as marked]: motion 191 (1.12), invincible for the motion's
  length (1.67 then 1.91 -> FUN_00175aa8: +0xb00 = it; FUN_00175ab0 counts it down in the party tick, FUN_00175ad8
  refuses hits: `party_receive_hits` logs `not hit by atkp ..: invincible`), motion 4 queued; once range trigger 16
  is on (1.150 bit 1) and the motion is still 191 (1.15 answers the motion number the script asked for, not the
  remapped slot): attack 1188 radius 200; each frame while the motion runs (1.14): a goal (the target's point at
  Goofy's height, or the player when the target is nearer than 150, or 100 ahead without a target), `h = sin(a) x
  35 + 65` above it (a ground probe from 80 above when far), the direction (start (0, -1, 0, 1)) turned toward the
  goal by 10 degrees (0.46 = FUN_0016d138 -> FUN_0017c0f8, `friend_host::turn_vec`, test
  `member_turn_vec_follows_fun_0017c0f8`), `a` += 5 degrees, the attack 100 above Goofy (2.12); while trigger 16 is
  on 1.111 moves him along the direction at 3 / 10 / 2 a frame, else the attack is released and he stops; at the
  end motion 4 and 1.74. 6.3 / 6.5 (the target search FUN_002f01a0 / FUN_002f04a8, used only without a valid target)
  are not ported. Atkp 1188 has Power 0 and reaction class 5 (a pull toward the attack object, which
  `enemy::receive_hits` decodes only for spells: `NotDecoded { class: 5 }`); the damage comes from the spin's own
  motion volumes (atkp 1160, damage 1 with a blow on a Dusk). Log (tt04 b_44, `KH2_PARTY_ABILITIES=423,-429`):
  `P_EX030#1 invincible for 215 frames (1.91, +0xb00)`, `attack object 0x7a000001: atkp 1188 group 0x101 at ..
  radius 200 ..`, `script mode "tornado" -> "-"` 7.3 s later. [inferred] The movement: the enemy host's 1.111 keeps
  the ground part of direction x speed and the member's motor its gravity (the goal's height is not reached:
  Goofy spins on the ground); the motion's length (motion frames) is taken as frames of +0xb00 (one per 60 Hz tick),
  so the invincibility ends halfway through the app's motion; the units of the two were not compared.
- The runtime (`kh2-bdx`, `runtime.rs`) hands 0.45 / 0.46 the two vectors and the scalar and writes the host's answer
  back into the first (their pointer is the script's memory, which the host cannot reach); the enemy and spell hosts
  do not answer them (unchanged). `friend_host::advance_vec` is 0.45 (FUN_0016d098, test
  `member_advance_vec_follows_fun_0016d098`).
- `blade` (R0d30) (done in WP-S5, section 16; as found by WP-S3): a child thread (1.82) throws the shield object (1.234 the weapon list +0xaf8, 1.83,
  1.148 / 1.89 / 1.79 / 1.11 on the weapon, attack 1161 with a new group (2.67) anchored to it and a callback 2.38,
  2.40) and the mode waits for it (context +0x20). The app's hand objects are not objects a script can move, so the
  child thread fails (a script error reading through the null weapon object at 0x0e14 in the base run, then the mode
  waits forever: Goofy stops choosing actions after his first `blade`; this was so before WP-S3).
- `turbo` (R0b4e -> R0b5c) (done in WP-S5, section 16; as found by WP-S3): motion, 1.91 for its length (186), motion queued, then while the motion runs
  a dash steered with 0.46 toward the target or toward points of 1.247 `random_free_point` (FUN_001726d8 ->
  FUN_001a6068 / FUN_001a6198, a spiral search with the game's second generator) and 6.4 / 6.5 (target search),
  moved with 1.111 while range trigger 16 is on; no attack object (the motion's volumes hit). In the app 1.247 is not
  answered: the script reads through its 0 answer (`script error BadAddress(0) at 0x0beb`) and the mode ends after
  ~1.7 s (log `turbo.log` with `KH2_PARTY_ABILITIES=425,-429`).
- 10.55 on the script's own memory (Goofy's search blocks) is classed a stub: the host cannot reach the script's
  globals, and only 6.5 reads the bit. (WP-S6: the runtime does it, section 17.)

**The script's motion, again**: a script motion that ended stays on its last frame (`Animator::play` would start a
finished motion again, so the script's 1.14 never saw it end and Goofy's `tornado` never left its loop); the queued
motion (1.13) is put on by `friend_ai` after the slice that saw the end.

Not verified: the spark element each hit starts (only the effect file is chosen as the game does); voices 6 / 21 / 3
the scripts ask for (1.262 on a member goes to the enemy voice path, which finds no enemy of that name).

## 16. Status: Goofy's turbo and blade, Donald's blizzard (WP-S5)

Files: `party_rules.rs` (`free_point`), `friend_host.rs` (the hosts), `friend_ai.rs` (the hand object, the
callbacks, the wall test, `MemberAttacks`), `party.rs` (`member_hands` draws a thrown hand object,
`retarget`), `enemy.rs` (the class-5 pull), `kh2-bdx` (`Runtime::call_routine`, the empty copy, name 6.9).

**Turbo** (`P_EX030` R0b5c) [V for the script and 1.247]. Host 1.247 (0x1726d8 -> FUN_001a5ea8 with mask 1):
FUN_001a6068 builds the search block: the centre, the radius `lo + r x (hi - lo)` and a start angle of
`((t & 4) >> 2 | t << 1) x 0x37b5a8 + r x 0x37b5a8` (`r` the second generator 0x3716a0 times 0x37b5ac, `t`
the counter 0x35e9b0, then `t + 1` modulo 8), as host 1.248's. FUN_001a6198: one step of 0x37b5b4 (pi / 4)
further each try, the candidate `centre + (sin a, 0, cos a) x radius` is cast for a floor from 0x37b5c0 (200)
above to 0x37b5bc (1000) below (FUN_001a5c30 -> FUN_001d1648); with a floor the line from the centre to the
point 0x37b5b8 (20) further out and 0x37b5b0 (100) above the floor must be clear (FUN_001a5bf8), and the floor
point is the answer; after eight tries the radius shrinks by 100; below `lo` the floor under the centre
(else the centre). Test `party_free_point_matches_fun_001a5ea8` (kh2-ee, the two collision queries stubbed:
the point, 0x3716a0 and 0x35e9b0 agree in 7 cases) and `party_free_point_by_hand`. [inferred] The floor cast
is `World::ground` and the line test `World::raycast` (the masks of FUN_001d1648 were not read); each member
keeps its own generator and counter (the game shares them). Log (tt04 b_44, `KH2_PARTY_ABILITIES=425,-429,-423`):
`script mode "-" -> "turbo"` at t 8.5, `"turbo" -> "-"` at t 14.5 (the whole motion; before: the script error
at 0x0beb after ~1.7 s); the host table shows `1.247 random_free_point Approx x10` and no missing call but
6.4 before it was classed (below).

**The target search 6.3 / 6.4 / 6.5** [V for what was read, **not ported**]. 6.3 (FUN_002f01a0): block word 0 =
the argument (5, the history length [inferred]), +0x14 = 0x383fb4 (1000), +0x18 = 0x383fb8 (pi), the rest 0; the
script then writes +0x10 / +0x14 (0 / 500) and +0x18 (2 pi) itself. 6.4 (FUN_002f0748): byte +4 and word +8 = 0.
6.5 (FUN_002f04a8): clears the record (FUN_0018ed40), walks the object list (FUN_001b7c88), keeps objects that pass
FUN_002f0218 (the kind +0xc: 0 = +0x588 bit 0x10, 1 / 2 = bit 4) and FUN_002f03f8 (FUN_0019a5d0 unless kind 2 or
bit 4), skips +0x108 bit 0x8000 when block +4 bit 0x10000 is set (10.55), takes each target part
(FUN_0016b4e0(obj, 3, ..), FUN_001894d0 its point), with history on skips parts in the history (FUN_002f01d8),
measures the (flat when asked) distance to the point given, keeps those within +0x10..+0x14 and, when +0x18 is
below 0x383fbc (pi), within that angle of the direction given; the nearest (or farthest) goes into the record
(FUN_0018f490) and, with history on, into the history (FUN_002f0438 -> FUN_002f02f8, not read). The block lives
in the script's own memory, which the host cannot reach, and FUN_002f02f8 / FUN_0019a5d0 were not read: all three
are a stub, the member keeps its own target (Goofy's `tornado` and `turbo` dash at it). (Ported in WP-S6: section 17.)

**Blade** (`P_EX030` R0d30 / R0d94, exit R13f5) [V for the script; the hosts as marked]. The member's hand-0 object
is a scripted object (`friend_host::WeaponObj`, handle `WEAPON_HANDLE + slot`): 1.234 (0x174280, +0xaf8) names it;
the child thread R0d94 waits for the throw trigger (R0f55: 1.151 bit 0), takes it off the hand (1.161 has_link ->
1.102 link_end), places it at its bone 3 (1.83 -> FUN_0016aa18 on the last drawn pose, 1.148), makes attack 1161
with a new group (2.67) anchored to it (2.22), radius 60, and registers R10f7 as its hit routine (2.38: +0x8c the
thread, +0x90 the routine, +0x94 the argument; disassembly of 0x18e4f0), plays motion 201 on it and flies it for at
most 180 frames: each frame the homing step R110b, 0.45, 1.89 (the secondary velocity +0x880; [inferred] the free
object moves by it once a frame, `friend_ai`), 2.12 to its position, 1.79; it stops on a wall (2.40) or a hit. A
hit runs the routine (FUN_0018cf50: after the spark and the owner's +0x74, FUN_0018dbd8 -> FUN_0016c550 with
(argument, victim, attack); `Runtime::call_routine`): R10f7 sets the context's +0x40 and points the target record at
the victim (1.160 -> FUN_0018f490; `PartyMember::retarget`). The object then holds still on the target (1.89 speed
0) while hits keep coming, ending 16 frames after the last one; the main routine links it back (R13ac: 1.235 the
joint, 1.18 link_start), plays motion 193 and queues 0; the mode's exit (R13f5 -> R0a96 -> R0ad9) releases the
attack (2.13). `party::member_hands` draws a free hand object at its own root and facing (one tick behind) with
the motion asked for, [inferred] the motion number as the hand object's slot (not a player-type object) and a
yaw-only facing. Test `goofy_blade_throws_and_takes_back_the_hand_object` (kh2-bdx disc test, the mock world
with a hand object; no script error). Log (tt04 b_44, `KH2_PARTY_ABILITIES=-423,-425 KH2_PARTY_WEAPONS=75,49`):
`hand object off the hand (1.102)`, `attack object 0x7a000001: atkp 1161 ..`, `.. hits 2985v0: atkp 1161`,
`its routine R10f7 runs (2.38, FUN_0018cf50)`, `target record now names 2985v0 (1.160 ..)`, `hand object back on
the hand (1.18)`, `script mode "blade" -> "-"`, then `P_EX030 chose record 5 Attack ..`: Goofy goes on choosing
(before: BadAddress(0) at 0x0e14 and the mode waited forever). (The members held items 75 / 49 in these runs without
`KH2_PARTY_WEAPONS` too.) A member without a hand object gets 0 from 1.234. (WP-PY3) The script then goes on to
call 1.161 and 1.83 on the null handle; in the original FUN_001beb00 reads the word at address 0 + 0x570 and
FUN_0016aa18 the matrices at 0 + 0x670 / 0x6b0, i.e. whatever low memory holds, which cannot be modelled.
`friend_host::weapon_calls` answers a null handle with no hand object as `false` (1.161) and the zero vector (1.83),
both Approx [inferred], so the script no longer faults with `BadAddress(0)` at 0x0e14.

**Hosts of `P_EH000` beyond 10.35 (WP-PY3).** `KH2_JUMP_WORDS=4:0:0 KH2_JUMP=1:0:0 KH2_JUMP_AT=4 KH2_BDXLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 target\debug\kh2-game.exe tt02`: 1.131, 1.54 and (when called) 1.105 / 1.301 are not missing, the enemy host answers them as Stub (`enemy_host.rs`); 1.54 is exact (FUN_0019dde0 clears bit n in the words at +0x47c and +0x480 + 4 (n >> 5)), and no reader of the arrays was traced. 1.131 (FUN_0018f3f8) files a 5-word record into the pool `DAT_01c89bd0 + 0x14 * index` (index popped from `DAT_01c8a5d0[--DAT_01c8a7d0]`), which the app does not pool. 1.301 (FUN_001abee8) is the byte at 0x35ea48 + 0x1e, 0 in the executable's initial data (the Stub answers 0); its writer was not searched. 1.105 (FUN_001731c0) clears bit 6 of +0x768; readers of +0x768 exist (`mips_scan --load-off 0x768`: FUN_00168d30, FUN_00173178, FUN_00176e00, FUN_00176f88, FUN_0017c718, FUN_0017c840, FUN_00191c18), their use of bit 6 is not read. 1.244 `in_battle` (FUN_001903d8, word 0x1c8a7d8) is now Impl in the friend host (`FriendFields::in_battle` from `audio_bgm::BattleFlag::active`). Still unimplemented for `P_EH000`: 10.58 (WP-RV2 read it: it stores a record and a value on the friend object, freed by the destructor, no other reader; 10.50 is called by no script, the "x75" here has no source).

**Blizzard** (`P_EX020` R0a19 -> R0acf, request thread R0af8) [V for the script]. The cast (motion 191, the
trigger, MP, element 3 on Donald) starts a request (2.20) of 288 bytes at R0af8 with the facing: element 4 of
Donald's file unbound (1.21 -> FUN_0016a530) and attack 1164 (2.10, radius 80 by 2.11); each frame the homing
step R0f2b (1.140 / 1.120 / 6.9 on the target record copied by 1.131, 0.21 / 0.13 / 0.46 turns), 0.45 along the
direction, 0.38 the shard's place, 2.12 the attack there, 0.41 its direction (FUN_0017be00's angles: the element's
rotation, as the hit spark's); it ends when the attack meets a wall (2.29) or clashes (2.39) or after its 30-frame
life (then 0.52 fade 8 and 2.13). Hits on enemies are `combat::Hit`s with the rebuilt status (WP-S4). Log (tt04
b_44, `KH2_PARTY_ABILITIES=166,-167,-168,-165`): `attack object 0x7a000001 hits 2733v0: atkp 1164 ..` and `Dusk
hit: atkp 1164 damage 10 hp 22 reaction 4 ..`; no missing call. It needed one interpreter fix: R1312 copies 0 bytes
from address 0 (`copyto [local0]+0x118, 0`), which the game's interpreter does with memcpy (FUN_001d7cb0 case 2 ->
FUN_00310af4) and so reads nothing; the runtime raised BadAddress(0) at 0x137f (test `copy_of_no_bytes_reads_nothing`).
Not reached by blizzard (the request's +0x100, +0x108, +0xfc, +0x118 stay 0 from R1312 / R1385, and the bounce
branch tests `-1 >= 0`): 1.88, 1.160 (R0d3a), 2.19, 2.43, 2.44, 2.57, 6.51, 6.54; they stay unanswered by the
members' host (the enemy host's `missing` if a script reached them). Not seen in a run: a shard meeting a wall. (WP-S6, section 17: none of them is reachable.)

**Wall test of the members' attack objects** (FUN_001a2cc0 -> FUN_0018c8e0): after a move of more than two
units (+4 & 4) a row with BG Hit (+0x12 bit 0) whose object has hit nothing and met no wall casts the segment from
the previous position to the new one, then down by FloorCheck (`combat::wall_cast`, mask 4); a hit sets 8 or 0x10
(FUN_0018ca78). [inferred] +4 bit 0x100 (testing after a hit) is not modelled; the wall spark is not shown.

**Tornado's pull** (atkp 1188, reaction class 5). FUN_00185668 -> FUN_001bf920 keeps the attack object in the victim
(+0xaac) for any attack object, so a member's object pulls like a spell's: `friend_ai::MemberAttacks` holds the
members' live objects (keys with bit 31, the slot and the handle) and this tick's hits; `enemy::receive_hits` and
`think` read it after `magic::SpellAttacks`. Log (tt04 b_44, `KH2_PARTY_ABILITIES=423,-425,-429 KH2_REACTIONLOG=1`):
`Dusk hit: atkp 1188 damage 0 .. reaction 5 effective 5`, `Dusk pulled (reaction class 5, FUN_001bf920): attack
2164260865 ..`, `Dusk pull: 201 units from attack ..`; the spin's own motion hit (atkp 1160) then replaces the pull
(FUN_0016b698), as for the spells.

## 17. Status: the target search, blizzard's later hosts, the hand object's velocity (WP-S6)

Files: `kh2-bdx` (`search.rs`, the runtime's arms for 6.3 / 6.4 / 6.5 / 10.55 / 6.54, `Host::target_parts` /
`Host::target_found`, the executable's numbers in `HostTable::search`, the name 2.19), `friend_host.rs`,
`friend_ai.rs`, `party.rs` (`search_target`).

**The target search** [V: test `search_matches_fun_002f04a8`]. Goofy's scripts keep the block in their mode
context: `tornado` at +0x24 (R0a15: 6.3 with a history of 1; 10.55 with 1; R0a1f: +0x10 = 0, +0x14 = 10000;
R0a2e: +0x18 = 2 pi), searched (R091a) only when 1.140 says his target record is not valid; `turbo` at +0x20
(R0cfb: a history of 5; 10.55; +0x10 / +0x14 = 0 / 500; +0x18 = 2 pi), searched every 30 frames of the dash
(R0c1b), and after five searches 6.4 empties the history (R0c12). Both pass the record of 2.23 (the member's
+0x9f4), the member's position and facing, nearest 1, flat 1, history 1; neither writes the kind +0xc, so it
stays 0. The block (FUN_002f01a0): +0 the history's capacity, byte +4 its next slot, byte +5 the parts the last
search walked, +4 bit 0x10000 (10.55, FUN_002f0758: skip objects with +0x108 bit 0x8000), +8 the history count,
+0xc the kind, +0x10 / +0x14 the distances, +0x18 the angle, +0x1c + 8 i the history (object, part id);
6.3 leaves the history entries as they are, and +0x14 / +0x18 start at 0x383fb4 / 0x383fb8 (1000, pi).
FUN_002f04a8: the record cleared (FUN_0018ed40: 0, 0, 0, -1), byte +5 = 0; the object list from 0x1d5d7a0
(+0x8b4 the next; FUN_001b7c88 skips objects with +0x108 bit 0x80000 or 0x10000000, FUN_0019a440); an object
passes FUN_002f0218 (one linked under a parent with +0x588 bit 0x80 never; kind 0: +0x588 bit 0x10, set by the
enemy class FUN_00184820; kinds 1 / 2: bit 4, the player-type classes FUN_001746e0 / FUN_00174800; other kinds:
any), not +0x108 bit 0x8000 with the skip bit, and FUN_002f03f8 (kind 2 or bit 4, else FUN_0019a5d0: class kind
byte (+8)+4 not 0 / 1 / 5, not 8 / 0xd in battle with 0x35d268 bit 0x10000, +0x7f8 bit 0x400, +0x108 bit 0x20
clear, a target part); then each target part (FUN_0016b4e0: the collision list +0x8d8, records of type 3, the
part enabled in the list's word +4 by its id byte 0 unless the bit arrays +0x474 / +0x47c say otherwise,
FUN_0019a6a0): byte +5 counts it; with history on a part in the first +8 entries is skipped (FUN_002f01d8); its
point is FUN_001894d0's (the bone's matrix times the record's offset, moved up by the lift +0xe); the distance
(the height difference dropped when flat) must lie in +0x10..+0x14; below 0x383fbc (pi) the ground direction of
the part must differ from the direction given by less than +0x18 (atan2 of x and z, FUN_003003e8, the difference
wrapped by FUN_0017bbc0 with 0x37a528 / 0x37a52c); the nearest (or the farthest) wins, the first of equals
(FUN_0018f490: object, part id, 0, -1). With history on, the winner goes into the next slot (FUN_002f0438 ->
FUN_002f02f8: unless already there; the count grows up to the capacity, the slot advances modulo it,
FUN_002f02b0). The script's block is out of the host's reach, so the runtime (`kh2_bdx::search`) does 6.3,
6.4, 6.5 and 10.55 itself and asks the host only for the objects' target parts (`Host::target_parts`), then
writes the record and tells the host (`Host::target_found`); the oracle test lays random objects (flags, class
kinds, collision lists, matrices) into kh2-ee memory, runs the game's FUN_002f04a8 with every callee its own,
and compares the record and the whole block over 400 scenes (144 with a target). Test
`goofy_turbo_searches_its_targets` (kh2-bdx disc test, the mock world): kind 0, the skip bit set, the nearest part
within 500 first, the other next (the history), then none, and after five searches the nearest again; no script
error.

In the app (`FriendHost::target_parts`): kind 0 lists the hittable enemies' TARGET shapes (the collision records of
type 3, with their id byte and the hurt shape's centre: `combat::hurt_spheroid` is the bone frame times the offset
plus the lift, as FUN_001894d0) in the fighters' order; their handles are `0x7e000000 + the entity's index`, so the
history keeps them from search to search. `target_found` on the member's target record points the member's target
at the part found (`party::search_target`: the special's record too) and answers 1.140 / 1.120 / 6.9 from it at
once; a cleared record makes the target invalid. [inferred] `hittable` stands for FUN_0019a5d0's tests; kinds 1 / 2
(the player and the members) are answered with no parts (no member script asks them); the enable mask of the
collision list (its word +4, which `kh2_model::collision` does not keep) and +0x108 bit 0x8000 (10.55's skip) are
not modelled; the list's order is the fighters' query order. The app's own re-target pass (`party_tick`) still runs
while a special does (as before). 10.14 (0x2f6da0 -> FUN_00112370) in Goofy's script passes mode 0 (R0a7b) or mode 1 with his own
record of 2.23 (R0b14): the record is copied onto itself (and cleared when FUN_0018ed58 finds it invalid), so no other
record reaches +0x9f4 that way. Log (tt04 b_44, `KH2_PARTY_ABILITIES=425,-429,-423 KH2_PARTYLOG=1 KH2_BDXLOG=1`):
`script mode "-" -> "turbo"`, then every half second `party: P_EX030#1 target search (6.5, FUN_002f04a8): 7 parts
walked, the record names 2985v0 shape 0`, then `.. names 2859v0 ..`, `.. names 2796v0 ..` (the history moves the
choice on), 23 searches in the run, no script error; the host table shows `6.5 ? Approx` and no missing call.

**Blizzard's later hosts** [V from the script]. In P_EX020 the request fields +0x100, +0x108, +0xfc, +0x10c, +0x114,
+0x118 are written only by R0b1b, R1312 and R1385, each with 0 (no indirect writer: no `push.i` of their offsets in
the script). So 1.88 (R0ca9, behind +0x100), 2.38 at R0c9d (+0x10c), 2.57 (R0c71 / R0d02, +0x118), 6.51 / 6.54
(R1145 / R114b, +0x108) and 2.19 (R1191, +0xfc > 0) are never reached; the bounce branch R0cce ands with the
constant test `-1 - 0 >= 0`, so R0d3a (1.132, 2.44, 2.23, 1.131, 1.140, 1.160, 2.43, 2.13) and the 2.44 at R0cdb
are dead code. What is ported anyway, because the code is plain: 6.54 (0x2ef110: the vector into the static
buffer 0x1e35680, the answer its address) in the runtime for every script; 2.19 (0x18e7a0 -> FUN_001a30a8: +0xe0
and +0xf0 set, +4 bit 0x20) named `attack_set_positions` and answered for the members' attack objects (bit 0x20 not
kept: its reader was not read); 2.44 (0x18e5d8: the word +0x84) answers the member, the attack object's owner
(FUN_001a3280 reads the owner's +0x9f4 through it) [inferred: FUN_0018bd08's write of +0x84 not read]. 1.88, 1.160
(other than the blade callback's), 2.43 (FUN_001a3280), 2.57 (FUN_001b3c48 -> FUN_001db868 on the object +0x18) and
6.51 (FUN_001d1648, a collision cast with mask 0x10004) are stubs naming the reason. Log (tt04 b_44,
`KH2_PARTY_ABILITIES=166,-167,-168,-165`): `script mode "-" -> "blizzard"`, `attack object 0x7a000001 hits 2733v0:
atkp 1164 ..`, `Dusk hit: atkp 1164 damage 10 hp 22 reaction 4 ..`, `"blizzard" -> "-"`, no script error, none of
the stubs in the host table.

**The hand object's velocity and facing** [V]. 1.89 (0x172f90 -> FUN_00183418): +0x880 = the direction made unit,
+0x88c = the speed. Its only reader is FUN_00181e00 (the object's movement, from FUN_001b74e0; the `lqc2` loads
mips_scan does not list, found in the decompilation): the frame's displacement gets `+0x880 x +0x88c x
DAT_00356f40` and +0x880 / +0x870 are cleared at the end, so the object moves only in frames the script set it.
`friend_ai` now clears the velocity after the move (before, it kept moving by the last one). 1.79 on the hand object
(0x172d98 -> FUN_0016a828): without the lock bit (+0x7f8 bit 8, FUN_0019ad98) only a yaw, atan2 of x and z
(FUN_003003e8), into +0x66c (FUN_0016a7f0): the flat facing `member_hands` draws is the game's. [inferred] The
turn from +0x550 to +0x66c (who applies it) and the mover's own displacement (+0xc vtable +0x1c, FUN_00182370) were
not read. Blade still runs (log: `hand object off the hand (1.102)`, `atkp 1161 ..`, `back on the hand (1.18)`,
`"blade" -> "-"`, no script error).

**Gaps**: FUN_0019a6a0's bit arrays +0x474 / +0x47c (who sets them) and the collision list's enable word; the
writer of +0x108 bit 0x8000 on enemies (10.55 skips them); FUN_001d1648's masks (6.51); FUN_0018bd08's +0x84.

