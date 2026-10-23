# Enemies: kinds M_EX8xx and M_EX9xx

[Contract](contract.md) | [Index](../../README.md)

## The third enemy: M_EX930, the Dancer (WP-7D)

`M_EX930` (object 0x138, ObjectType 4, `enemy::DANCER`) stands with two Dusks in b_21 (btl program 0x7). "Dancer"
is inferred from its mode names (`atk_force_dance`, `atk_spin_u/m/d`, `atk_kick`) and its pink and cream textures
with the Nobody emblem; the code calls it by its model. Model (4 meshes, 60 bones, 2 textures), motion set
(253 slots, 32 with motions, entry 252 a paired player motion), effect file and BDX script (4383 words) are on the
disc (`cargo test -p kh2-bdx --test disc m_ex930`).

- **Stats.** R0f13 hands `enmp` row 48 to `set_obj_param` (the constant pushed at 0x04ce, the call at 0x0f3a): 32 HP
  at world level 1. The same routine stores the attack ranges (400 / 800 cm at context +0x30 / +0x34), the chase
  radii (2000 / 1200 cm, R0f99: world byte 0x33e830 is not 10, spawn kind not 5 or 6), the motion blend (8) and
  the prize row (25, read by `dead`).
- **Appear.** Spawn kind 2 (b_21) or -1 (the app's waves) faces the player and plays motion 44 with a fade-in
  (R058a); other kinds start motion 0 at a random frame (R062d).
- **Attacks.** Spins 206 / 207 / 208 (atkp 438, 439, 440 through range trigger 10), the kick 205 (atkp 441; three in
  a row, and `atk_kick_call` sends `atk_kick` to every object with a delay of 60), the force dance (below).
  Attack shapes and windows come from the motions' triggers like the other enemies'.
- **Force dance.** `atk_force_dance_near` (R0a48) closes in with motion 202 for at most 600 frames; within 100 cm
  it switches to `atk_force_dance` (R0b0a) when the player's 1.101 and 1.225 read clear, else to
  `atk_force_dance_finish` (motion 204). `atk_force_dance` tests 1.225, 1.161 and 1.101 on the player again, then links the player to the Dancer (1.18 `link_start`, joint word 0x4002, flags 1), sets
  his flag bit 1, plays motion 203 on itself and paired entry 252 on him (1.29: sub-motion `0010`, 116 ticks),
  queues his motion 0 and waits while motion 203 plays and the link holds. **Gap:** the player-side link
  (FUN_001beb90: parent at +0x570, mover 3; FUN_00169ee8 breaks it when the parent's +0x160 loses bit 0x8000) is
  not in the app, so Sora plays the paired motion where he stands instead of being carried by the Dancer; 1.161
  answers "not linked", which only ends the script's wait earlier (it then waits for motion 203's end anyway).
- **Reactions** use the default slots (10/12 flinch, 16..19 blow) of its set. **Death.** `dead` (R06bd) drops
  prize row 25 (1.92), starts element 2 of its effect file (`Kind::death_effect`), sets flags 3 and 6, waits half of
  +0x20 (15 of 30 frames), fades out over the other half and hides.
- **Not reached.** Event 13 (`event_begin`: 1.128, motion 4, 1.65) is not delivered by the app; motion 47
  (`footwork`) is not in its set (the request is logged and ignored).

Also fixed for the Dusks of b_00 / b_21 (WP-7D): when range trigger 16 is on (1.150 bit 1) the Dusk reads word 0
of its own target record (0x136d -> R0217); the host now serves that record (the player, index 0 [inferred], 0,
-1, 0), which removes the `BadAddress(0x3000000)` script error those Dusks hit in mode `escape`.

Acceptance run (WP-7D): `KH2_BDXLOG=1 KH2_WAVE_GROUP=b_21 KH2_WAVE_AT_GROUP=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1
KH2_KEYS="6:Q,<J every 0.45 s from 6.3 to 36 s>" kh2-game tt04`: modes `appear`, `mode_battle`, `btl_attack`,
`btl_short`, `btl_long`, `move_back`, `move_target`, `mode_revenge`, `rvg_short`, `atk_spin_random`, `atk_spin_d`,
`atk_kick_call`, `atk_kick`, `atk_force_dance_near`, `atk_force_dance`, `dead`; no script error, no unimplemented
host call; its spin 440 flinched Sora; the paired motion 252 played on Sora (`player: paired motion started: entry
252 sub-motion 0010 of Dancer`).

## The enemy M_EX880 (WP-EN9c)

`M_EX880` (object 317 = 0x13d, `00objentry` row 317: ObjectType 4, `NeoStatus` 3011, flags 0, `SpawnLimiter` 9; `enemy::M_EX880`,
called by its model: no name is in the data the app reads) is a Nobody that the area survey found in 13 areas (eh12, eh25, hb08,
hb18, he09, tt03, ..). Its BDX script (5035 words, `cargo run -p kh2 -- bdx M_EX880`) has the modes `appear`, `mode_battle`,
`btl_attack`, `btl_short`, `btl_long`, `atk_brush`, `atk_brush_all`, `atk_brush_revenge`, `atk_dragoon_jump`, `jump`, `move_target`,
`idle_battle`, `idle_time`, `mode_revenge`, `rvg_short`, `rvg_long`, `dusk_surprise`, `dead`. The rows 1184 (`M_EX880_DANCER`) and 2294
(`M_EX880_DANCER_EH`) are other objects with their own scripts and are not this kind.

- **Stats.** The set-up routine R1191 (0x1191) takes eight arguments from its one call (0x04ec); the first, 52 (pushed at 0x04d3),
  goes to `set_obj_param` (host 2.9, 0x11b8): `enmp` row 52 (HP 100 in the table; the harness enemy has 20 HP, as for the other kinds).
  `KH2_BDXLOG` shows `SetObjParam { a: 52 }`.
- **Death.** The `dead` mode (registered at 0x00eb, routine R06c3) hands the object's +0x1c to host 1.92, starts element 2 of its
  effect file (host 1.87 at 0x06e8), releases it (0.79), sets bits 3 and 6, fades out and hides: `Kind::death_effect` true.
- **Hosts.** No new host call: every one it calls is in `enemy_host.rs` already. The Stub ones it reaches are 0.79, 1.121, 1.171,
  1.206 and the mode-frozen test; the Approx ones are 1.11, 1.111, 1.120, 1.23, 1.55, 1.56, 1.60, 1.62, 1.64, 1.80, 1.87, 1.92, 2.5 and
  the mode-may-start test (each carries its reason in the table).

Acceptance run (WP-EN9c), tt03 (the first listed area; `btl` program 7 loads b_21 with two `M_EX880`, one Dusk and three absent
`M_EX950`; test `tt03_btl_program_7_holds_two_m_ex880`, disc): `KH2_PROGRAMS=0,7,0 KH2_WAVE_GROUP=b_21 KH2_NO_PARTY=1 KH2_AREALOG=1
KH2_BDXLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 kh2-game tt03` gives `area group b_21 kind-2 group 0x10b: M_EX880 (serial 0x36, wave 0)`
and the same for serial 0x67, then the modes `appear`, `mode_battle`, `btl_attack`, `atk_brush` (it hurt Sora, 20 to 16 HP),
`idle_battle`, `move_target`, `atk_brush_revenge`. With `KH2_KEYS` (Q at 4 s, J every 0.4 s from 5.5 s): `M_EX880 hit: atkp 127 damage 2
hp 18` .. `atkp 129 damage 6 hp 0`, `experience: enmp 52 defeated: +4 (lvpm 4 x enmp 100)`, `hp 0: switching to mode dead`, `drop (host
1.92) asked row 29` and `prize: drop of row 29 .. 2 objects`. No `Missing` host call and no script error in the log.

Gaps: the other 12 areas (eh12, eh25, hb08, hb18, he09, ..) were not run, only tt03; the group's `M_EX950` and Dusk-like members were
not examined; the `atk_dragoon_jump` and `atk_brush_all` modes were not seen in the run; the drop row (29) was not traced further.

## M_EX900 (WP-EN9d)

**Status: spawned (WP-EN12a).** `enemy::M_EX900` is in `enemy::KINDS`; the gate below (no sender of event 17) was lifted by WP-DYN1 and the soak was run in WP-EN12a (last paragraph of this section). The app now logs 
`area groups: tt03 btl program 0x49 group b_40: M_EX900, Dusk, Dusk (not in the app: [])`. Test `m_ex900_is_a_spawned_kind` (disc) checks the objentry row (309, type 4, NeoStatus 3004) and that b_40 holds one M_EX900 and two Dusks.
The paragraphs below up to the WP-DYN1 note describe the state before the sender was found. Everything below was built and run with the kind enabled (the host calls stay and serve any kind).

Event 17 sender search (WP-EN9d, second pass), all negative: the script has no literal 17 (`kh2 bdx M_EX900`); `FUN_0016c620`
(-> `FUN_0016c4c0`, `FUN_0016c550`) has 19 call sites (`mips_scan --jal 0x16c620`) with fixed ids 0, 6, 10, 0xb, 0xd, 0xe,
0xf, 0x10, 0x13, 0x17, 0x18, 0x1a, 0x1b (FUN_00169a90, 0016b3e0, 0016b420, 0016bfd0, 001767a8, 00178748, 00182f68,
00183080, 00185cf0, 001889f0, 001954f8, 001b21f0, 001b2478, 001b2588, 001b28a0, 001b3c70, 001b3d18) and one broadcast to
the global scripts (FUN_00195690, list 0x35e364); `FUN_0016c500` (7 sites: ids 7, 0xc, 0x14, 0x16 and the broadcast) and
`FUN_0016c550` (FUN_00187098, the mode machine's hooks, whose ids come from the mode record halfwords; FUN_001becb8, the link
release, whose event fields +0x57c/+0x580/+0x584 are only ever cleared: `--store-off 0x57c` -> FUN_00168d30, FUN_001beb90).
The only literal 0x11 handed to `FUN_0016c620` is at 0x188620 in the callback at 0x188498 (reached through `FUN_001889d0` ->
`FUN_00188cc0`, no direct caller): a command-record interpreter that sends event 0x11 with (object, halfword +4, halfword +6) of
a command record for one of its cases. That is the sender's shape (code 1 / 2 = halfword +4), but the producer of those command
records (the object-command queue behind `FUN_00188cc0`) and the jump-table case were not traced, and it is the kind of record the
event player / mission scripts queue, which the app does not have for enemies. No frame trigger or collision part was read in the
`M_EX900.mset`. Next step for a later WP: trace the producers of the records `FUN_00188cc0` walks.

**Event 17 sender found (WP-DYN1, docs/dynamic-tracing.md section 3).** The records `FUN_00188cc0` walks are the motion's frame triggers
(the trigger table of each `.mset` slot), and the dispatcher's jump-table entry 7 (`0x188604`, table 0x37aa78 read live) is the event 17 sender: a
frame trigger of type 7 with halfwords (code, argument). `kh2 mset obj/M_EX900.mset` shows them: `7@0[2, 0]` on 26 slots (code 2 clears script word
+0x18 and restores the weakness byte to 100), `7@0[3, 0]` / `7@175[3, 0]` on the dive slots (code 3 sets +0x18 and the byte to 0), one code 1. The
app now delivers it (`enemy_ai::motion_events`), so the two kinds of `KINDS` with an entry for event 17 (M_EX690, M_EX940) get it too. M_EX900 is
still not in `KINDS` (no soak of it was run in WP-DYN1); its gate in the paragraph above is lifted.

`M_EX900` (object 309 = 0x135, `00objentry` row 309: ObjectType 4, `NeoStatus` 3004, weight 500, flags 0, `SpawnLimiter` 10;
`enemy::M_EX900`, called by its model) appears in 12 areas (eh11, eh25, he09, tt03, tt06, tt07, ..). In tt03 the `btl`
program 0x49 loads b_40 with it and two Dusks (test `tt03_btl_program_49_holds_an_m_ex900`, disc). Its BDX script
(7439 words, `cargo run -p kh2 -- bdx M_EX900`) has the modes `appear`, `dive`, `mode_battle`, `btl_attack`, `atk_upper`,
`mode_revenge`, `rvg_short`, `atk_dead`, `rc_crash`, `crash_dead`, `stun`, `freeze`, `leave`, `dead`.

- **Stats.** Its set-up routine R1ab8 (0x1ab8, one call at 0x08c8) hands `enmp` row 45 (pushed at 0x08af) to `set_obj_param`
  (host 2.9, 0x1adf): 32 HP in the harness, experience `+10` (`enmp 45 defeated: +10 (lvpm 4 x enmp 250)`).
- **Death.** The `dead` mode (registered at 0x037d) drops through host 1.92 (row 23 in the run: 6 prize objects and item
  343), then fades; `Kind::death_effect` true [inferred from the other kinds' dead modes, not traced].
- **New host calls** (`enemy_host.rs`): 1.13 queue_motion on itself (Approx: the 4-entry queue is kept in `Obj::motion_queue`,
  its first entry starts when the playing motion ends, a script `play_motion` drops the queue [inferred]; the consumer of the
  game's ring at the motion controller +0x3c was not traced; it matters: rc_crash plays motion 205 and queues 0); 1.311
  set_scaled_byte (FUN_001a0ba0: byte +0x1a8 + i of the status block = the enmp row's weakness i times the percent, rounded up;
  FUN_001a0610 multiplies the damage by byte [attack element] and byte +0x1ae (i = 6), FUN_0018b618 makes a hit ineffective
  when the product is 0; `enemy_ai` rescales `Stats::weakness`); 1.213 (0x1707d8 -> FUN_001699a8: halfword +0x4c of the
  objentry row = `NeoStatus`, 1 for every P_EX100 row; the script asks "is the target Sora"; Approx: the context lives in the
  script frame, its object is the target = the player, answer 1); 2.21 shared_block (FUN_001a5040, a process-wide
  `SharedBlocks`, the game's list is global); 2.45 hit_atkp_id (the halfword +2 of the hit record; `AiHit::atkp` carries it).
- **Run** (`KH2_AREALOG=1 KH2_BDXLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_NO_PARTY=1 KH2_WAVE_GROUP=b_40 kh2-game tt03`, Sora
  hitting with J every 0.7 s from 5.5 s): the two Dusks die, `M_EX900 (serial 0x11, wave 1) created near the player`, modes
  `appear`, `dive`, `mode_revenge`, .., the hits land (`M_EX900 hit: atkp 129 damage 6 hp 0`), `hp 0: switching to mode dead`,
  `experience: enmp 45 defeated: +10`, item 0x157 picked up. No `unimplemented host call` and no script error.

Gaps: (1) The script keeps a word at +0x18 of its context that is 1 while it flies (set by R0ddb at 0x0ddb, the dive) and makes the
main loop (0x02d3) set damage scale 6 to 0, so every hit is ineffective. It is reset to 0 by a real hit (0x021a: never, hits are
not real while it is 0), by clash event 19 (0x01ed), by event_end (0x0292) and by **event 17 code 2** (0x005c, which also
puts scale 6 back to 100); event 17 code 1 enters `crash_dead`. Nothing in the app sends event 17 and no sender was found
(callers of `FUN_0016c620` have no literal 0x11; `mips_scan --li 0x11` found `FUN_0016bbe8`, `FUN_001722c0`, `FUN_00178290`,
`FUN_00179880`, `FUN_0018852c`, none read in detail) [inferred: a motion-embedded or collision event]. So after a dive it stays
invulnerable for as long as it is not clashed (in a run it sat in `rc_crash` for over a minute, the fallback of the dive end when the
target is not within 100 cm: R12d9 -> R130f). It dies only when hit before its first dive or after the clash resets the flag.
(2) The dive's end distance (118 to 126 cm in the harness, the grab path needs under 100) depends on the app's approximate
motion control (1.61 / 1.12 are Approx), so the paired grab (motion 252, thread 0x1373) was not reached. (3) The other 11 areas were
not run; tt06 and tt07 are the other Twilight Town areas.

**Soak and verification (WP-EN12a).** Kind added to `KINDS` (39 kinds); no new host arm. Harness `KH2_PROGRAMS=0,1,0 KH2_WAVE_GROUP=b_00 KH2_WAVE_AT_GROUP=-2,0 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_SCREENSHOT=<png> KH2_SHOT_AT=<s> KH2_KEYS=15:J,20:J,.. kh2-game tt19` through `local/capped.ps1 -CapMB 2500`. (1) tt19 b_00 soak, 125 s with J every 5 s from 15 s only to press Continue (Sora was knocked out several times; enemy ticks stop at game over): enemy ticks reached t7050 (about 117 s of enemy time), 0 `unimplemented host call`, 0 `script error`; the three first M_EX900 (kind-2 group 0x12b) and their respawns ran `appear`, `mode_battle`, `dive` (27 times), `btl_short`, `btl_long`, `move_target`, `atk_jump`, `atk_upper`, `atk_under`, `footwork`, `move_round`, `leave`, `mode_revenge` -> `atk_dead` -> `rc_crash` -> `crash_dead` (one run: t4597 to t4746, 149 ticks, ended on its own). Every dive ended: 140 `event 17: motion frame trigger 7 (code 2, argument 0); handler ran` lines, the first dive of each enemy at about t102 and after it the host 1.311 call back to scale 100 (`Func { group: 1, index: 311, args: [.., Int(6), Int(100)] }`) and the next hit landed (`M_EX900 hit: atkp 126 damage 3 hp 29`, t4432). The only `ineffective true` hits (3 of 22) fall inside a dive, where scale 6 is 0 by design (script word +0x18). The `rc_crash` fallback did not stick. (2) tt03 b_40 with two Dusks (`KH2_PROGRAMS=0,49,0 KH2_WAVE_GROUP=b_40 KH2_WAVE_AT_GROUP=1 KH2_NO_PARTY=1`, J every 0.6 s from 8 s): the Dusks die (`enmp 41 defeated: +6`), M_EX900 appears (wave 1), is hit 8 times (`atkp 126`..`147`, `FlinchPush` and `Blow`), `hp 0: switching to mode dead` at t2743, `experience: enmp 45 defeated: +10`, drop `asked row 130, used row 130 .. 4 prize objects` and the prizes are picked up (munny +10, +5, HP +5). A second run with a 1.0 s cadence killed it too (t2719, +10). 0 `unimplemented host call`, 0 `script error`. (3) PNGs looked at (`local\ex900_*.png`, not committed): idle at 8 s in tt19 (`ex900_s8.png`: Sora beside a wall, debug text `M_EX900 Idle hp 32/32` three times, the enemies far off, one spiky silver shape and one small silver one on a lit floor at the left), dive at 14 s (`ex900_s14.png`: large silver spiked shapes filling the left half beside Sora, HP 7/20 and the Hurt state, pink slivers), a hit at 44.5 s in tt03 (`ex900_d44.png`: golden hit stars and the damage number 2 at the centre, debug text `M_EX900 Hurt hp 9/32 .. (d 0.8)`, a spiky silver enemy in the lower right), the death at 46.5 and 47.5 s (`ex900_d46.5.png`, `ex900_d47.5.png`: no enemy left; a green HP orb and a multicoloured munny gem in the air, the strips "NEXT LEVEL 24" and "OBTAINED MUNNY 5" at the upper left). With Lv1 Sora (20 HP) and no party the kill needs a lucky cadence (several runs ended in a knock-out before the 8th hit); the hit cadences that killed were 0.6 s and 1.0 s from 8 s. Gaps: only tt19 b_00 and tt03 b_40 were run (not eh11, eh25, he09, tt06, tt07); the paired grab (motion 252) and the dive end distance of gap (2) above were not looked at; no audio was listened to.

## The enemy M_EX950 (WP-EN9e)

`M_EX950` (object 314, `00objentry` row 314: ObjectType 4, `NeoStatus` 3009, flags 0, `SpawnLimiter` 17, `SpawnObject1` 1537 =
`M_EX950_CARD`, `SpawnObject2` 1538 = `M_EX950_DICE`, both ObjectType 0; `enemy::M_EX950`, called by its model: no name is in the data
the app reads) is a dice-and-card Nobody of the 14 areas the area survey lists (ca09, ca10, ca12, ca13, ca14, ca15, tt03, ..). "Gambler"
is inferred from its mode names. Its BDX script (11184 words, `cargo run -p kh2 -- bdx M_EX950`) has the modes `appear`, `mode_battle`,
`mode_battle_boss`, `mode_revenge`, `mode_revenge_boss`, `btl_brush`, `btl_dice`, `btl_card`, `atk_brush`, `atk_dice`, `atk_card`, `move_run`,
`move_step`, `move_fly`, `fly_fall`, `near`, `near_wait`, `footwork`, `leave`, `hide`, `revenge_shout`, `rc_gamestart`, `rc_dice_*`, `rc_card_*`,
`dead`.

- **Stats.** The set-up routine R2971 (0x2971, one call at 0x1985) takes eight arguments; the last one, 50 (pushed at 0x196d), goes to
  `set_obj_param` (host 2.9, 0x2998): `enmp` row 50 (`experience: enmp 50 defeated: +10 (lvpm 4 x enmp 250)`). The harness enemy has 32 HP (20 for the Nobodies of other kinds).
- **Death.** The `dead` mode (registered at 0x1949, routine R28dc -> R291a) hands the object's +0x1c to host 1.92 (drop: `drop (host 1.92) asked
  row 27`, `prize: drop of row 27 .. 6 objects`), starts element 2 of its effect file (host 1.87 at 0x2938), releases it (0.79), sets bits 3 and 6, fades
  out and hides, like the Dusk's: `Kind::death_effect` true.
- **Variants.** The mode picked by R27db (rand_float 0.35 / 0.7) is stored in object +0x18 (0 brush, 1 dice, 2 card); `btl_brush`,
  `btl_dice`, `btl_card` and the `atk_*` modes choose by it. The two spawn objects 1537 and 1538 are created through the objentry's
  `SpawnObject` columns; the app has no objects for them (create_object gives `Missing` only if the script calls it: it did not in the run).
- **Moving.** `move_run` (R200d) only faces the player rotated by 15..75 or 270..360 degrees and plays motion 2; the movement is the motion's
  root motion. It keeps 2 to 6 m from Sora and walks past him. It leaves (`mode leave`, R2760 when the distance is over the +0x38 / +0x3c
  limits, or a global flag) and hides itself when Sora is far (seen at 1779 units, `hidden: despawned`).
- **Hosts.** New arms (appended to `enemy_host.rs`): 1.130 (Impl), 1.140 (Approx), 1.179, 1.75, 1.76 and 1.128 (Stub) and 1.13 on itself
  (Stub), see `docs/enemy-ai.md` "Host call mapping". The rest were in the table already; the Stub ones it reaches are 1.121, 1.171, 1.206,
  1.211 and the mode-frozen test.

Acceptance run (WP-EN9e), tt03 (the first listed area; `btl` program 7 loads b_21 with three `M_EX950`, two `M_EX880` and one Dusk; test
`tt03_btl_program_7_holds_two_m_ex880`, disc): `KH2_PROGRAMS=0,7,0 KH2_WAVE_GROUP=b_21 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_NO_SOUND=1
KH2_NO_MUSIC=1 kh2-game tt03` gives `area groups: tt03 btl program 0x7 group b_21: M_EX950, M_EX880, M_EX880, Dusk, M_EX950, M_EX950 (not in the
app: [])`, `SetObjParam { a: 50 }` and the modes `appear`, `mode_battle`, `btl_brush`, `move_run`, `atk_brush`, `atk_card`, `atk_dice`, `leave`. With
`KH2_KEYS` (W held from 4 s for 2.6 s to reach it, J every 0.4 s from 6.6 s): `M_EX950 hit: atkp 126 damage 3 hp 29` .. `atkp 130 damage 5 hp 0`,
`experience: enmp 50 defeated: +10`, `hp 0: switching to mode dead`, `drop (host 1.92) asked row 27`, `prize: drop of row 27`; all three die in 55 s
and the last log table has no `Missing` line of M_EX950 (the one `1.140` line of `M_EX880#3` is answered by the new arm). Harness PNGs looked at:
idle (three pale pointed-hat figures with Sora and the Nobodies, HUD present), a hit (`M_EX950 Hurt hp 20/32`, one figure bent over at the right),
and after the fight (no enemy text, Sora Lv2, prize sparkles). `idle-hud`, `combat-ground` and `tt04-dusk` of `accept.ps1` PASS and their PNGs show
the HUD, four Shadows and a Dusk as before.

Gaps: the `rc_*` modes (the reaction-command mini game: host 1.165 `offer_command` 313 after 1.221, host 1.237 `offer_four`) were not reached in
any run (no event 11 of the 312/323 kind arrived), so whether the app answers them is not known; the two spawn objects (1537 `CARD`, 1538
`DICE`, objentry type 0) were not seen to be created; `mode_battle_boss` / `mode_revenge_boss` were not seen; ca09 and the other 12 areas were not
run, only tt03; the reader of +0x100 (1.179) and of the controller word +4 (1.75 / 1.76) was not traced (`mips_scan --load-off 0x100` was not
run: the base is not a fixed object); drop row 27 was not traced further.

## The enemy M_EX910 (WP-EN10g)

`M_EX910` (object 310, `00objentry` row 310: ObjectType 4, `NeoStatus` 3005, `SpawnLimiter` 20, no spawn objects; `enemy::M_EX910`, called by its model: the data gives no name) is a
dark cloaked Heartless with a blade of the 11 areas the area survey lists (eh08, eh12, eh16, hb08, hb09, he09). Its BDX script (7354 words, `cargo run -p kh2 -- bdx M_EX910`)
registers the modes `appear`, `form`, `mode_battle`, `mode_battle_boss`, `mode_revenge`, `mode_revenge_boss`, `btl_short`, `btl_long`, `btl_attack`, `btl_normal`, `btl_hard`, `btl_super_hard`,
`move_target`, `move_roll`, `atk_stab`, `atk_side`, `atk_vertical`, `atk_swordplay`, `warp_back`, `warp_target`, `footwork`, `rvg_*`, `idle`, `idle_time`, `freeze`, `leave`, `dead`,
`change_target_door`, `change_target_leon`, `rc_duel_baut`, `rc_duel_idle`, `event`, `the_end`.

- **Stats.** The set-up routine R1a85 (0x1a85, called at 0x0cca) takes eight values after the object; the first pushed, 46 (pushed at 0x0cb2; R1a85 stores it to local+0x20), goes to `set_obj_param`
  (host 2.9 at 0x1aac): `enmp` row 46 (`experience: enmp 46 defeated: +10 (lvpm 4 x enmp 250)`). The harness enemy has 32 HP.
- **Death.** `hp 0: switching to mode dead`, then `drop (host 1.92) asked row 15, used row 15: .. 6 prize objects` and `prize: drop of row 15 ..: 6 objects, kinds [6, 6, 6, 6, 6, 6]`; `Kind::death_effect` is true as for the other kinds.
- **Host calls.** None were new: every call the script makes was answered already (no `unimplemented host call`, no `script error`). Not `Impl` at first use: 1.211 (Stub, `obj_flags_abc_set`) and 1.74 (Approx, `mover_reset`), both
  existing arms. The self motion queue (host 1.13) is not enabled for the kind (`queues_motions`): the script plays its motions with 1.11/1.12 and the soak shows no mode waiting on it.

Acceptance run (WP-EN10g), hb09 group b_40 (btl program 0x33: five `Dusk` at load and one `M_EX910` made on demand by kind-2 group record 0x56 once the Dusks are gone; test
`m_ex910_is_an_enemy_kind_with_its_objentry_row`, disc): `KH2_PROGRAMS=0,33,0 KH2_WAVE_GROUP=b_40 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_SHOT_AT=900
KH2_KEYS=8:Q,12:J,12.45:J,.. kh2-game hb09` through `local/capped.ps1 -CapMB 2500` (the harness exits 3 s after `KH2_SHOT_AT`, 40 s by default, so a soak needs a large value; with
`KH2_NO_PARTY=1` Sora was knocked out by the Dusks before the kind appeared in most runs, without it the party kills them). (1) Hitting run (no party, Q at 8 s then J every 0.45 s from 12 s, 170 s): the
kind appeared at t2248 (37 s), `M_EX910 hit: atkp 126 damage 3 hp 29 ..` with `FlinchPush` and `Blow` reactions, `hp 0: switching to mode dead` at t4089 (68 s), `experience: enmp 46 defeated: +10`, 0 `unimplemented host call`, 0 `script error`.
(2) Soak, 200 s with J only until 36 s and then every 5 s only to press "Continue" after Sora's six knock-outs (enemy ticks stop at game over): one `M_EX910` from t2065 to t11499 (about 157 s of enemy time), never hit
(2 hits when it was near Sora, `rvg_short`/`atk_swordplay` at the end), 0 `unimplemented host call`, 0 `script error`; modes run: `appear`, `form`, `mode_battle`, `btl_attack`, `btl_short`, `btl_long`, `move_target`, `idle_time`, `atk_stab`, `atk_side`,
`atk_vertical`, `warp_back`, `mode_revenge`, `rvg_short`, `atk_swordplay`; the longest mode is `form` at about 340 ticks (5.7 s); every mode ended. One quirk: at t6345-6349 (just after a Continue) `mode_battle` to `move_target` repeated once
per tick for five ticks, then settled. PNG looked at (`ex910_z3.png`, scratchpad, not committed; a party run, shot at 26 s): Sora HP 8/20 with Donald and Goofy in the hb09 alley, a debug list line `M_EX910 Idle hp 27/32 (d 4.0)` between two
Dusk entries and a dark cloaked blade-armed figure left of Donald. No PNG of the death was caught (the fight differs from run to run: four shots at 21, 31, 44 and 48 s caught only Dusks); the death is shown by the log lines above only.
`idle-hud`, `combat-ground` and `tt04-dusk` were not rerun: no shared file changed except the `KINDS` entry.

Gaps: (1) eh08 group b_00 (three `M_EX910` alone, btl program 1) creates nothing in the app: the log says `0 enemies at load, 0 on demand` although group record 0x126 (kind 2) counts 3; why was not looked into
(the kind-2 group's activator or condition). (2) hb08 b_40 (five `M_EX910` and Dusks, btl program 0x34) and eh12 b_01 and eh16 b_00 bring the kind in later waves; the 70 s and 200 s runs of eh16 never reached it, hb08's waves belong to the
struggle mission layout (`struggle`), not the group, so only hb09's single one was seen. he09 was not run. (3) `mode_battle_boss`, `mode_revenge_boss`, `move_roll`, `atk_vertical` hits on Sora, `warp_target`, `change_target_*`, `rc_duel_*`,
`leave`, `rvg_normal/hard/super_hard`, `btl_normal/hard/super_hard` were not seen. (4) The objentry `ObjectTargetType` was not looked up.
## The enemy M_EX920 (WP-EN10p)

`M_EX920` (object 311, `00objentry` row 311: ObjectType 4, `NeoStatus` 3006, flags 8, ObjectTargetType 2, Weight 100, `SpawnLimiter` 15, no spawn objects; `enemy::M_EX920`, called by its model: the data gives no
name; in the PNGs it is a purple Heartless with a long blade-like limb that warps beside Sora) is a Heartless of the areas eh06, eh10, eh11, eh17, he09 and mu10 (area survey, 8 areas; he09 b_9g, mu10 b_60 and eh10 b_00 hold it as
listed by the `area groups:` log line). Its BDX script (10411 words, `cargo run -p kh2 -- bdx M_EX920`) registers, among others, the modes `appear`, `mode_battle`, `mode_revenge`, `btl_short`, `btl_long`, `rvg_short`, `rvg_long`, `atk_shot_s`,
`atk_shot_b`, `atk_brush`, `atk_brush_rush`, `atk_brush_warp`, `move_round`, `move_target`, `move_warp`, `footwork`, `footwork_warp`, `idle` and `dead`. Its motion set has two paired player motion entries (the limit windows of
`docs/limits-reactions.md`, entry 252 opens 286); the app offers none for it.

- **Stats.** The set-up routine R2683 (0x2683, called at 0x1ab4) takes eight values after the object; the first pushed, 47 (pushed at 0x1a9c; R2683 stores it to local+0x20), goes to `set_obj_param` (host 2.9 at 0x26aa):
  `enmp` row 47 (Experience 250; log `experience: enmp 47 defeated: +10 (lvpm 4 x enmp 250)`). The harness enemy has 32 HP.
- **Death.** `hp 0: switching to mode dead`, then `drop (host 1.92) asked row 24, used row 24: prizes per kind [0, 0, 0, 0, 0, 0, 6, 0, 0], items [] (6 prize objects, 0 item objects)`; `Kind::death_effect` is true as for the Dusk.
- **Host calls.** One arm was appended to `enemy_host.rs`: 0.85 `effect_stop_mark` (0x16dbb8 -> FUN_001d9f48), a Stub (effects run to their end; the first run logged `unimplemented host call 0.85`, none after the arm). Every other call the
  runs made was answered already (Impl, Approx or the existing Stubs 1.121, 1.171, 1.206, 1.211, 65535.0); host 1.13 never left a mode waiting (`queues_motions` is unchanged).

Acceptance run (WP-EN10p), eh10 group b_00 (btl program 1: three `M_EX920` made by kind-2 groups (the first record is volume 0x125); test `m_ex920_is_an_enemy_kind_with_its_objentry_row`, disc): `KH2_PROGRAMS=0,1,0 KH2_WAVE_GROUP=b_00 KH2_NO_PARTY=1
KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_SCREENSHOT=<png> KH2_SHOT_AT=<s> KH2_KEYS=11:Q,12:J,12.45:J,.. (J every 0.45 s) kh2-game eh10` through `local/capped.ps1 -CapMB 2500`. Three `M_EX920` were
created at the record's points (about 6, 9 and 10 m from Sora). (1) Soak, 330 s of wall time (enemy tick about 18700 over three Continue reloads of the area): 0 `unimplemented host call`, 0 `script error`; modes run: `appear`,
`atk_shot_s`, `atk_shot_b`, `atk_brush`, `atk_brush_rush`, `atk_brush_warp`, `move_warp`, `move_round`, `move_target`, `footwork`, `footwork_warp`, `idle`, `mode_battle` -> `btl_short` / `btl_long`, `mode_revenge` -> `rvg_short`, `dead`;
the longest single mode run was `footwork` at 330 ticks (5.5 s), `move_round` 180, `move_target` 118; no mode stuck. Sora was knocked out three times (the game-over Continue worked, the area and the group were set up again). (2) 25 hits
(`M_EX920 hit: atkp 126 damage 3 hp 29 .. FlinchPush` and `Blow` reactions), two kills (`#3` at t2632 after 44 s, `#1` at t18734), each followed by `experience: enmp 47 defeated: +10` and the six-orb drop. The third instance was
not killed in that run (Sora hits rarely: the Heartless keep 3 to 6 m away or warp). PNGs looked at (`local\ex920_*.png`, not committed): idle at 12 s (`ex920_idle.png`: Sora on a metal floor in a white and blue hall, debug text
`M_EX920 Idle hp 32/32` three times at d 6.0, 9.1 and 10.5; the Heartless are not visible in the picture), a hit at 13.6 s (`ex920_hit.png`: two purple hooded blade Heartless beside Sora, golden hit stars and a damage number "3",
lock-on reticle, an HP bar; entry `Hurt hp 29/32`), a death at 44.4 s (`ex920_dead.png`: a burst of sparks and prize bubbles at the target, only two enemies listed, damage number "2"). `idle-hud`, `combat-ground` and `tt04-dusk`
(`local\accept.ps1 -Only ..`) ran PASS and their PNGs show the HUD, Shadows hit in tt00 and a Dusk hit in tt04.

Gaps: (1) Only eh10 b_00 was run; mu10 b_60 (with Dusks), he09 b_9g, eh06, eh11 and eh17 (beside `M_EX870` / `M_EX900`, kinds the app lacks) were not; the survey run of mu10, eh06, eh10, eh11, eh17 and he09 only listed the groups.
(2) Not seen: `rvg_long`, `btl_*` beyond short and long, the boss modes and `leave`; the limit offer of paired entry 252 (the app sends no event to Sora, no window opens). (3) The elements of the shot and brush attacks
(host 1.87 `spawn_attached`, Approx) were not looked up and one run did not show them. (4) The 0.85 mark bit has no reader looked up (searched only the two functions named above). (5) The kill of all three instances in one run was
not achieved (see above).

## The enemies M_EX870 and M_EX960 (WP-EN11b)

`M_EX870` (object 316 = 0x13c, `00objentry` row 316: ObjectType 4, flags 8, `NeoStatus` 0, Weight 100, `SpawnLimiter` 34, `SpawnObject1` 315) is what the area groups list; eh17's `btl` program 1 loads b_00 with eight `M_EX870` and five `M_EX920`
(kind-2 groups 0x60 two, 0x61 two, 0x63 four of them; area survey: only eh17 was found to list it). `M_EX960` (object 315, row 315: ObjectType 4, `NeoStatus` 3010, flags 8, `SpawnLimiter` 0, no spawn objects) is in no group: each `M_EX870` makes one with host 1.0
`create_object(315)` (BDX `M_EX870` call at 0x03ed, in its set-up before the `appear` mode) and moves it to its own point. Both are kinds of `enemy::KINDS` (`enemy::M_EX870`, `enemy::M_EX960`, called by their model names: the data gives none). The scripts
(`cargo run -p kh2 -- bdx M_EX870`, 3606 words; `M_EX960`, 4059 words) are two halves of one fighter: `M_EX960` picks the attack (`atk_shout_shield`, `atk_shout_shot`, `atk_shout_blade`) and sends the matching name as a message (host 1.90 at 0x0c2d, R0c18); `M_EX870` runs
`atk_shield`, `atk_shot`, `atk_blade` and `idle`. Both register `appear`, `mode_battle`, `mode_revenge`, `leave`, `dead`, `freeze`, `event`, `idle_time`, `footwork`, `btl_*`, `rvg_*`, `rst_*`; `M_EX960` also the `move_*` modes.

- **Stats.** `M_EX870` never calls `set_obj_param` (host 2.9) and its row has `NeoStatus` 0, so (as for `M_EX850`) nothing builds a status block: `Kind::status` is false, it cannot be hurt, its `enmp` is a stand-in [inferred]. `M_EX960`'s constructor R0dc5 (0x0dc5, called at 0x0539) takes eight
  values after the object (pushed 0x0521..0x0536: 51, 28, 300.0, 500.0, 8.0, 4, 0.7, 0.35); 51 goes to host 2.9 (0x0dec): `enmp` row 51 (Experience 250; 32 HP at the harness level), 28 is kept at +0x1c as the drop row of host 1.92.
- **Lifecycle.** The `M_EX870` main thread (R0105 at 0x0105, every tick) asks host 1.94 `obj_valid` for its `M_EX960` (handle at +0x6c) and switches to `leave` when it is gone; `leave` runs to its `dead`-style routine R0929 (hide, then host 1.328). The app removes a hidden object, so
  the pair disappears one tick apart after the `M_EX960` dies (`M_EX960#4 t4078: hidden: despawned`, `M_EX870#2 t4079: hidden: despawned`, mode `leave` at t4079). The group stays clearable: only the `M_EX960` has to die.
- **Host calls.** Appended or changed in `enemy_host.rs`: 1.160 and 6.57 (`call_with_mem`, Approx), 1.328 (Approx), 1.133 (Stub, was Missing), 1.89 on the enemy itself (Stub, was Missing); see docs/enemy-ai.md. `Obj::target_rec` makes the object's own target record writable
  (R0a50 at 0x0a50 stores the word 6.57 answers into its +4: the same value, so no change). The first run logged `unimplemented host call` for 1.160, 6.57, 1.133, 1.89 and 1.328 and a `script error BadAddress(0x3000004)` at 0x0a67 (the write into the target record); none after the arms.

Acceptance runs (WP-EN11b), eh17 b_00 (`KH2_PROGRAMS=0,1,0 KH2_WAVE_GROUP=b_00 KH2_WAVE_AT_GROUP=45,-61 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_KEYS=9:Q,12:J,13:J,..` (J every second) through `local/capped.ps1 -CapMB 3500`; the group made by this
position is 0x61: one `M_EX920` and two `M_EX870`): (1) 0 `unimplemented host call`, 0 `script error`. (2) Soak, party on, no key but Q: 7020 enemy ticks (117 s), modes run `appear`, `mode_battle`, `idle`, `atk_shield`, `atk_shot`, `atk_blade`, `leave` for `M_EX870`, `appear`, `mode_battle`, `btl_attack`, `btl_short`, `btl_long`,
`atk_shout_*`, `move_*`, `mode_revenge`, `rvg_*`, `dead` for `M_EX960`; the longest quiet stretch of an `M_EX870` was 597 ticks (10 s, `idle` waiting for the next shout), of an `M_EX960` 96 ticks; the party killed both `M_EX960` (t4486, t4496) and both `M_EX870` left. (3) Hits and kills with J: 30 `M_EX960 hit:` lines
(`atkp 127 damage 2 hp 27`), `hp 0: switching to mode dead`, `experience: enmp 51 defeated: +10 (lvpm 4 x enmp 250)`, `drop (host 1.92) asked row 28, used row 28: prizes per kind [0, 0, 0, 0, 0, 0, 6, 0, 0] (6 prize objects)`; no `M_EX870 hit:` line ever (it has no status block). (4) PNGs looked at (`scratchpad.png`, `hit.png`, not committed):
idle (`b.png`, 16 s: dark floating hooded Heartless-like figures with a blade and shield beside Sora, debug text `M_EX870 Idle hp 50/50` twice and `M_EX960 Idle hp 32/32` twice), a hit (`hit.png`, 21 s: damage number "2", debug text `M_EX960 Hurt hp 27/32` at 2.3 m, Sora lower middle, Goofy and Donald beside him).
Sora (Lv 1, 20 HP) was knocked out when he fought the group alone (game over, Continue); with the party on, the two `M_EX960` fell in 47 to 70 s of wall time over the runs.

Gaps: (1) Only group 0x61 (two of the eight `M_EX870`) was run; 0x60 and 0x63 (two and four) were not entered on purpose. (2) No death PNG: the dying `M_EX960` is visible for 0.5 s and several attempts to time the shot missed it; the death is shown by the log lines only. (3) The `M_EX870` motion slots (44 `appear`,
201 / 202 attacks) and the elements of `spawn_attached` 10..17 (eight, started at once in the `appear` set-up) were not looked up; what the eight elements draw was not seen. (4) That `M_EX870` is unhittable in the original is read from `NeoStatus` 0 and the missing `set_obj_param`, not seen in play [inferred]. (5) Host 1.133's method is an
inference (see docs/enemy-ai.md). (6) `mode_revenge_boss`, `mode_battle_boss`, `freeze`, `event` and the `rst_*` modes never ran in the app. (7) The kind-2 group's own bookkeeping on 1.328 (FUN_00199398) is the app's removal.
## The enemy M_EX890 (WP-EN12c)
`M_EX890` (object 308, `00objentry` row 308: ObjectType 21, `NeoStatus` 3003; `enemy::M_EX890`) is a tall spiky white and purple Heartless; the survey's "next kinds" row 4 (tt29 `KH2_PROGRAMS=7` group b_21 holds four, 9 groups in all, all sole).
Its BDX script (6901 words, `cargo run -p kh2 -- bdx M_EX890`) hands `enmp` row 44 to `set_obj_param` (R18e1 at 0x18e1, called at 0x0b91 with 44 as the first of eight values pushed 0x0b79..0x0b8e; host 2.9 at 0x1908), so the experience row is enmp 44 (`experience: enmp 44 defeated: +10 (lvpm 4 x enmp 250)`).
Host calls: none added. Every call of the script is already mapped (the list of `kh2 bdx` host calls was checked against the arms; 1.165 offer_command, 1.110 spawn_kind and 2.22 attack_anchor were not hit in the runs). The self motion queue (1.13) is not called by the script and stays a Stub.
Acceptance run (WP-EN12c), tt29 `KH2_PROGRAMS=7 KH2_WAVE_GROUP=b_21 KH2_WAVE_AT_GROUP=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 kh2-game tt29` through `local/capped.ps1 -CapMB 2500`, keys `8:Q` and J every 0.45 s from 10 s: 0 `unimplemented`, 0 `script error`; all four reach `dead` (the last at tick 1832), each paying `experience: enmp 44 defeated`; modes seen: appear, mode_battle, mode_revenge, move_side, move_up, atk_brush, atk_jump, atk_jump_shout, atk_upper, atk_upper_shout, btl_short, idle_attack, jump, rvg_short. With `KH2_NO_PARTY=1` and Sora guarding 20..100 s: 1980 enemy ticks (33 s of enemy time), modes appear, move_side, atk_brush, mode_battle, btl_short, btl_attack, btl_long, idle_attack, move_back, move_up, jump, atk_jump, none stuck, all four at hp 32. The reviewer's longer soak (4 Continues, 5580 enemy ticks, about 93 s) found every mode ending, the longest 385 ticks (`atk_upper`). Test `m_ex890_is_an_enemy_kind_with_its_objentry_row` (disc).
Screenshots looked at: four M_EX890 listed (hp 9, 32, 28, 14 of 32) with Sora hurt and a lock-on ring on the tall Heartless, and a hit mid-combo on one (hp 2/32, Sora's blade, lock-on ring).
Gaps: (1) Only tt29 b_21 was run; the other 8 groups were not. (2) The enemy-time soak is 33 s, shorter than 75 s (the app ticks enemies slower than 60 Hz here); no mode stuck. (3) Death was seen in the log only; no screenshot of the death frame. (4) Its attack objects are stubs (2.10 family).

