# Enemies: kinds M_EX0xx to M_EX4xx

[Contract](contract.md) | [Index](../../README.md)

## The sixth enemy: M_EX420 (WP-EN9a)

`M_EX420` (object 120 = 0x78, `00objentry` row 120: ObjectType 4, `NeoStatus` 1003, flags 4, `SpawnLimiter` 10; `enemy::M_EX420`,
called by its model: no name is in the data the app reads) is the enemy kind the area survey found most often missing
(33 areas: al00, al01, bb00, bb06, bb13, ca02, ..). al00's `btl` program 0xa loads b_20 with ten of them (the group also
holds `M_EX060`, still absent; test `al00_btl_program_a_holds_ten_m_ex420`, disc). Its BDX script (6501 words, `cargo run -p
kh2 -- bdx M_EX420`) has the modes `appear_shadow`, `move_shadow_round`, `move_shadow_target`, `atk_claw`, `atk_a_charge`,
`atk_s_out`, `atk_group_call`, `mode_battle`, `mode_revenge`, `mode_battle_boss`.

- **Stats.** The script's set-up routine R174d (0x174d) takes eight arguments from its one call (0x0656) and hands the
  first of them, 9 (pushed at 0x063e), to `set_obj_param` (host 2.9, 0x1774): `enmp` row 9. `KH2_BDXLOG` shows
  `SetObjParam { a: 9 }`; the harness enemy has 20 HP.
- **Death.** The `dead` mode (registered at 0x01e2, routine R082e at 0x082e) hands the object's +0x1c to host 1.92 (the
  app's drop), starts element 2 of its effect file (host 1.87 at 0x0853), releases it (0.79), sets bits 3 and 6, fades out
  and hides: `Kind::death_effect` true.
- **Hosts.** No new host call: every one it calls is in `enemy_host.rs` already; the Stub and Approx ones it reaches
  (stubs 1.105, 1.121, 0.79, 1.171, 1.206; approx 1.11, 1.23, 1.34, 1.56, 1.92) carry their own reasons in the table.

Acceptance run (WP-EN9a), `KH2_AREALOG=1 KH2_BDXLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_WAVE_GROUP=b_20 KH2_NO_PARTY=1
kh2-game al00` (Sora starts inside kind-2 group 0x30d, which creates serial 0x3fd): `area group b_20 kind-2 group 0x30d:
M_EX420 (serial 0x3fd, wave 0)`, then modes `appear_shadow`, `mode_battle`, `move_zigzag`, `idle_time`, `atk_claw` (it hit
Sora, 20 to 17 HP), `mode_revenge`. With `KH2_KEYS` (Q at 4 s, J every 0.5 s from 5.5 s): `M_EX420 hit: atkp 126 damage 3 hp 17`
.. `atkp 129 damage 6 hp 0`, `experience: enmp 9 defeated: +4`, `hp 0: switching to mode dead`, `drop (host 1.92) asked row 41`
and the prize objects picked up. No `Missing` host call and no script error in the log.

Gaps: the other 32 areas were not run (only al00's b_20); `M_EX060` and the rest of b_20 are absent kinds; the drop row
the script asks (41) was not traced further.

## The enemy M_EX110 (WP-EN10b)

`M_EX110` (object 71, `00objentry` row 71: ObjectType 4, `NeoStatus` 1034, flags 8, Weight 100, `SpawnLimiter` 8, no spawn objects;
`enemy::M_EX110`, called by its model: the data gives no name) is a striped black and white Heartless of the 12 areas the area survey lists
(al00, al01, al03, al07, al10, al12, ..). Its BDX script (7155 words, `cargo run -p kh2 -- bdx M_EX110`) has the modes `appear`, `mode_battle`,
`mode_revenge`, `move_idle`, `idle`, `atk_foot_stamp`, `atk_rocket_attack` and `dead` (the ones a run reached; the entries are event 11
(0x001a, a handler for message 328), damage (0x079a), main (0x07c7) and init (0x08b7)).

- **Stats.** The set-up routine R19da (called at 0x0bc0 with `0, 47, 300, 600, 8, 4, 0.7, 0.35` after the object) passes its first value, 0,
  to `set_obj_param` (host 2.9 at 0x1a01): `enmp` row 0 (HP 50, Experience 50: `experience: enmp 0 defeated: +2 (lvpm 4 x enmp 50)`). The harness
  enemy has 10 HP. The second value, 47, is stored at object +0x1c (0x1a05) and is the drop row host 1.92 is asked for in `dead`
  (`drop (host 1.92) asked row 47`). Row 0 is the value the script pushes; no other `set_obj_param` call exists in the script.
- **Death.** `dead` starts element 2 of its effect file (1.87), releases it (0.79) and asks 1.92 for the drop, like the Dusk's:
  `Kind::death_effect` true.
- **Fades.** `move_idle` sets flags 1, 3 and 5 (bit 3 is the unhittable bit) and fades out; the next `FadeIn` (1.20, 20 frames) clears them. The
  figure is semi-transparent or gone for part of every cycle (screenshots below).
- **New host calls.** 1.99 `ground_depth` is EN10c's (Impl); 2.6 `in_reaction` (FUN_0018dfa0 -> FUN_00185a08: motion state 10 to
  39) is Approx on the app's motion slot; 2.67 `new_hit_group` (FUN_001bdf00: the counter at 0x1d5dd4c, 0x100 after 0x10000000) is Approx with the
  host's own counter starting at 0 (`enemy_host.rs`; `friend_host.rs` keeps its own). The ones it reaches that were already answered: 2.21, 1.206,
  1.171, 1.121, 1.53, 1.54 (Stub), 1.56, 1.120, 2.5, 1.111, 1.58, 1.62, 1.64, 1.140 (Approx).

Acceptance runs (WP-EN10b), al03 (smallest listed area): `btl` program 2 loads b_01 with five `M_EX110`, four Shadows and no other kind the app lacks
beside ten `M_EX630` (test `m_ex110_is_an_enemy_kind_with_its_objentry_row`, disc). The kind-2 group 0x1b creates three of them (serials 0xfe, 0xff,
0x109, wave 3) at the record's points (all zero in this group: the harness Sora stands among them). `KH2_PROGRAMS=0,2,0 KH2_WAVE_GROUP=b_01
KH2_WAVE_AT_GROUP=1 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 kh2-game al03` (`KH2_KEYS` Q at 8 s, J every
0.45 s from 30 s to 95 s, shot at 100 s): `SetObjParam { a: 0 }`, `M_EX110 hit: atkp 126 damage 3 hp 7` .. `atkp 130 damage 5 hp 0`, `experience: enmp 0
defeated: +2`, `hp 0: switching to mode dead`, `drop (host 1.92) asked row 47`, `prize: drop of row 47`. In 100 s Sora stayed at 20/20, the modes seen
were the eight above, and the final table had no Missing line but 0.68 (answered since, below); `accept.ps1 -Only combat-ground,tt04-dusk,idle-hud` PASS (`local\accept\en10b`): idle-hud shows the HUD and Sora, combat-ground Sora hurt beside four Shadows, tt04-dusk Sora, Donald and Goofy with a `Dusk Hurt hp 4/32` entry. PNGs looked at (`local\ex110_*.png`):
idle at 9 s (a striped Heartless behind Sora, three `M_EX110 Idle hp 10/10` entries), idle at 5 s (the same figure half transparent), a hit at 8 s
(damage numbers 5, entries `Hurt hp 3/10` and `Hurt hp 0/10`, hit sparks, a dark figure blown beside Sora), after defeats at 11 s ("OBTAINED MUNNY", prize
orbs, one entry left), a 100 s soak frame in the fade phase (the entry `Idle hp 10/10` listed, no figure drawn).

**Follow-up (reviewer's stuck state).** In an 88 s idle run all three sat in `atk_foot_stamp` from about 57 s on. Cause: R162c queues motion 0
behind motion 202 (host 1.13) and then loops in R16f2 until R0f0a sees the motion state (1.15) differ from 202; the app stubbed 1.13 for every kind
but M_EX900, so the queued motion never started and the state stayed 202 for good. Fix: `queues_motions` in `enemy_host.rs` now includes `M_EX110#`
(the same queue M_EX900 uses, Approx). Re-soak, 90 s, no keys, al03 b_01 (`ex110_s90e.png`): no `unimplemented host call`, no `script error`, no Missing line;
every object cycles `move_idle` / `idle` / `mode_battle` / `atk_foot_stamp` / `atk_rocket_attack`, the longest stay in one mode is 424 ticks
(`move_idle`), `atk_foot_stamp` ends after about 90 ticks. In the 14 s shot (`ex110_t14.png`, Q at 4 s) Sora is at 17/20: the kind hurts him.
Further host calls the attacks reached: 0.46 (and 0.45) through the members' `turn_vec` / `advance_vec` (Approx, the platform's trigonometry), 6.9
`target_distance` (Approx: the distance to the player's origin, his shape's radius is not subtracted), and **0.68** (FUN_0016cf78 ->
FUN_0017c398, a vector turned about the X axis by an angle, `kh2_bdx::math::rotate_x`, named in `names.rs`, in `is_math`): proved against the
game by `cargo test -p kh2-bdx --test rotate_x_oracle` (FUN_0016cf78 run in kh2-ee, 5 vectors x 9 angles, tolerance 1e-3: the sign is
`y' = y cos - z sin, z' = y sin + z cos`). `obj/M_EX110.mdlx` is in the script oracle's list (`cargo test -p kh2-bdx --test oracle`).

Gaps: the attack hit paths were seen once (Sora 17/20 at 14 s), not traced address by address; the other areas (al00 b_00 has 14 members of this
kind with mixed Shadows and Soldiers but its kind-2 groups were not entered in 70 s) were not run; the role of event 11 (message 328, handler
0x001a: starts action 135 and sets mode `rc_address_shift`) was not traced.

## The enemy M_EX210 (WP-EN10f)

`M_EX210` (object 74, `00objentry` row 74: ObjectType 4, `NeoStatus` 1043, `SpawnLimiter` 13, no spawn objects; `enemy::M_EX210`, called by its model: the data gives
no name) is a Heartless of the 11 areas the area survey lists (ca02, ca03, ca09, ca12, ca13, ca19 ..): a blue figure with purple blades on its arms that
chases Sora and kicks and uppercuts. Its BDX script (7008 words, `cargo run -p kh2 -- bdx M_EX210`) registers the modes `appear`, `mode_battle`, `mode_revenge`,
`btl_short`, `btl_long`, `btl_attack`, `btl_normal`, `btl_hard`, `btl_super_hard`, `move_idle`, `move_target`, `atk_kick`, `atk_strong_kick`, `atk_upper_cut`,
`atk_charge_punch`, `footwork`, `rvg_*`, `rst_*`, `idle*`, `freeze`, `leave`, `rc_meteor_giro`, `test_rc_meteor`, `btl_mission_gunpowder`, `event`.

- **Stats.** The set-up routine R18d6 (0x18d6, called at 0x08fa) takes eight values after the object; the first pushed, 3 (pushed at 0x08e2; R18d6 stores it to
  local+0x20), goes to `set_obj_param` (host 2.9 at 0x18fd): `enmp` row 3 (`experience: enmp 3 defeated: +4 (lvpm 4 x enmp 100)`). The harness enemy has 20 HP.
- **Death.** `hp 0: switching to mode dead`, then `drop (host 1.92) asked row 63` and `prize: drop of row 63`: 4 prize objects (munny seen picked up); like the Dusk's,
  so `Kind::death_effect` is true.
- **Host calls.** Two were new: 1.162 (Stub) and 6.24 obj_vec_8a0 (Approx), see `docs/systems/script-host/contract.md`. 6.24 is read by the loop at 0x105f that ends a chase; before it
  existed the call failed with `BadAddress(0) at 0x1064` every pass and the chase never ended. The rest were answered already (2.9, 1.23, 1.58, 2.21, 1.114, 1.110, 1.67,
  1.111, 1.120, 1.147, 1.15, 1.14, 1.4, 1.3, 1.34, 1.150, 1.201, 1.171, 1.206, 1.124, 1.12, 1.11, 1.55, 1.56, 1.301, 1.64, 1.20).

Acceptance run (WP-EN10f), ca03 group b_40 (btl program 0x38: four `M_EX210` and four `M_EX650`; kind-2 groups create them near Sora; test
`m_ex210_is_an_enemy_kind_with_its_objentry_row`, disc): `KH2_PROGRAMS=0,38,0 KH2_WAVE_GROUP=b_40 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1
KH2_NO_MUSIC=1 kh2-game ca03` through `tools/capped.ps1 -CapMB 2500`. (1) Soak, 140 s with J every 5 s from 15 s only to press "Continue" after Sora's knock-outs (enemy ticks stop at game over; Sora was knocked out 4 times): 0 `unimplemented host call`, 0 `script error`; the three first `M_EX210` reached t7966, t8040 and t8094 (about 133 s of enemy time), ran `appear`, `mode_battle`, `btl_attack`, `btl_short`, `btl_long`, `move_idle`, `move_target`, `atk_kick`, `atk_upper_cut`, `atk_charge_punch`, `mode_revenge`, `rvg_short`, and no mode ran longer than 270 ticks. An earlier 95 s soak (before the fix below) was wrong: `move_target` ended in a wait at 0x1205 forever (motion 153 played, motion 0 queued with host 1.13, and R0c50 waits for the motion state to leave 153; with the queue stubbed the state never left) and the enemies stood still facing Sora until he came near. Fix: the self motion queue of host 1.13 (`Obj::motion_queue`, see `docs/systems/script-host/contract.md`) is enabled for M_EX210 (`queues_motions`), as for M_EX110. (2) Hitting runs, `KH2_KEYS` Q at 8 s then J every 0.45 s from 12 s (a 150 s run: all three first `M_EX210` died, `M_EX210#3` t1253, `#1` t3359, `#2` t5737, 10 `experience: enmp 3` lines in all as respawns came, 4 knock-outs continued by the J presses): `M_EX210 hit: atkp 126 damage 3 hp 17 ..`,
`FlinchPush` and `Blow` reactions, `hp 0: switching to mode dead` for `M_EX210#3` at t1093 (17.7 s), 0 `unimplemented host call`, 0 `script error`; modes seen in the runs:
`appear`, `mode_battle`, `mode_revenge`, `rvg_short`, `rvg_long`, `btl_short`, `btl_long`, `btl_attack`, `move_idle`, `move_target`, `atk_kick`, `atk_strong_kick`, `atk_upper_cut`, `dead`. PNGs
looked at (`local\ex210_*.png`, not committed): idle at 7 s (`ex210_idle.png`: Sora on the Pirate ship deck, three `M_EX210 Idle hp 20/20` and four `M_EX650 Idle hp 14/14` entries, a purple-winged blue
figure in the foreground and two blue ones on the upper deck), a hit at 24 s (`ex210_hit.png`: Sora HP 9/20 behind a blue Heartless with a red heart emblem, entries at
hp 20/20, 14/20, 8/20), a death at 18.4 s (`ex210_death.png`: heart-shaped orbs and a green orb in the air, damage numbers "2", "5" and "4p", an entry `Blown hp 0/20`) and at 100 s
(`ex210_long.png`: no dead entry left, Sora HP 20/20, three Heartless at the far end of the deck). `idle-hud` (`base_idle.png`), `combat-ground` (`base_combat.png`: Sora
mid-swing, a Shadow hurt) and `tt04-dusk` (`base_dusk.png`: Sora with Donald and Goofy in the Sandlot, a hurt Dusk entry) were run by hand through `capped.ps1` (accept.ps1 stops `kh2-game`
processes, rule A10): HUD, Shadows and the Dusk fight look as before.

Gaps: (1) Host 1.162: what reads the table at 0x1c89b90 was not searched (the call changes nothing in the app as 1.143 finds no object). (2) Host 6.24: the offset +0x5a0 that
FUN_00182690 subtracts (not searched; taken as zero), and the other writers of +0x8a0 (`mips_scan.py --store-off 0x8a0` finds only stack stores and the clears in FUN_00168d30; FUN_00182690 (decomp) writes it through a computed base). (3) Only the
three `M_EX210` of the first wave and one death in the long run were followed; ca02, ca09, ca12, ca13 and ca19 were not run (ca02's b_01 mixes six `M_EX210` with Soldiers). (4) `atk_charge_punch`,
`footwork`, `rc_meteor_giro` (the reaction command meteor giro: event 11 messages 308 and 321 of `event_0b` at 0x001e; the app sends none), `btl_normal`/`btl_hard`/`btl_super_hard`,
`rst_*` and `leave` were not seen. (5) Every run logs `game over: area ca03: no snapshot` at the area start, before any enemy hit; its cause was not looked into [inferred: not from the kind].


## The enemy M_EX050 (WP-EN10j)

`M_EX050` (object 303, `00objentry` row 303: ObjectType 4, flags 0, ObjectTargetType 2, `NeoStatus` 1002, Weight 500, `SpawnLimiter` 15, ObjectShadowSize 2, no spawn objects; `enemy::M_EX050`, called by
its model: the data gives no name) is a Heartless of bb00, bb04, bb07, bb12, he05 and he07 (`docs/systems/script-host/area-survey.md`). In the app it is a large round blue-and-purple body with long chained arms. Its BDX script (5580 words, `cargo run -p kh2 -- bdx M_EX050`) registers the modes `appear`, `mode_battle`, `mode_battle_boss`,
`mode_revenge`, `mode_revenge_boss`, `btl_normal`, `btl_hard`, `btl_super_hard`, `rvg_*`, `rst_*`, `think`, `fight`, `angry`, `move_walk`, `footwork`, `idle`, `idle_time`, `freeze`, `atk_brush`,
`atk_dush`, `atk_dush_hit`, `atk_dush_ref`, `atk_jump`, `atk_curling`, `atk_curling_hit`, `leave`, `dead`, `event`, `revenge`.

- **Stats.** The set-up routine R13cd (0x13cd, called at 0x079a) takes eight values after the object; the first pushed, 35 (pushed at 0x0782; R13cd stores it to local+0x20), goes to `set_obj_param`
  (host 2.9 at 0x13f4): `enmp` row 35 (50 HP in the harness, Experience 250; log `experience: enmp 35 defeated: +10 (lvpm 4 x enmp 250, low-HP x1)`).
- **Death.** `hp 0: switching to mode dead`, `drop (host 1.92) asked row 3, used row 3`, `prize: drop of row 3: 6 objects`; `Kind::death_effect` is true as for the other Heartless.
- **Host calls.** Two arms were added (group 2): 2.64 `hit_reaction_original` and 2.85 `hit_atkp_element` (Impl, see `docs/systems/script-host/contract.md`): the damage entry R0231 (0x0231) reads both on every hit whose
  attack id is not 148, and before the arms the first hit logged two `unimplemented host call` lines (the script then saw 0 for both). `Obj::hits` carries the attack id only, so `enemy::receive_hits`
  notes the row's two bytes per id in `enemy_obj` (`note_hit_row`). All other calls were mapped already; the table of a run: 1.12, 1.11, 1.120, 1.15, 1.60, 1.80, 2.5, 1.56, 1.23 (Approx), 1.121, 1.171,
  1.206 (Stubs), 1.147, 1.14, 1.124, 1.4, 2.23, 2.8, 2.45, 2.9, 2.7 (Impl). Host 1.13 (three sites) never ran and no mode waited for a motion state, so `queues_motions` is unchanged. The script writes object
  memory at +0x11c after host 1.97 (log `script wrote object memory at +0x11c`: stored, nothing reads it); not traced.

Acceptance runs (WP-EN10j), through `tools/capped.ps1 -CapMB 2500`, one at a time; test `m_ex050_is_an_enemy_kind_with_its_objentry_row` (disc), `hit_rows_are_kept_by_attack_id`. Group b_20 of bb04
(btl program 0xa: three `M_EX050` and nothing else): `KH2_PROGRAMS=0,a,0 KH2_WAVE_GROUP=b_20 KH2_WAVE_AT_GROUP=1 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1
KH2_KEYS=7:W:2,10:Q,.. kh2-game bb04`. (1) Soak, 128 s of harness time with J every 5 s from 15 s only to press "Continue" (Sora knocked out 5 times; enemy ticks reached t6835, about 114 s of enemy
time): 0 `unimplemented host call`, 0 `script error`; two `M_EX050` ran `appear`, `mode_battle`, `btl_normal`, `btl_hard`, `think`, `move_walk`, `footwork`, `atk_brush`, `atk_dush`, `atk_jump`, `angry`,
`atk_curling`, `atk_curling_hit`; the longest time between two `mode` lines of one enemy was 372 ticks (`atk_brush`), so no mode hangs. A 125 s soak of bb00 (btl program 3, b_02: six `M_EX690` and four
`M_EX050`, `KH2_PROGRAMS=0,3,0 KH2_WAVE_GROUP=b_02`) was run before the two arms existed: the same modes plus `leave`, and only the two `unimplemented` lines above. (2) Hitting run, J every 0.45 s from 12 s:
`M_EX050 hit: atkp 126 damage 3 hp 44 ..` down to `atkp 126 damage 3 hp 0 ..` (hp 50 to 0 in 17 hits, `FlinchPush` and `Blow` reactions), `hp 0: switching to mode dead` for `M_EX050#1` at t1522 (about 25.4 s),
`experience: enmp 35 defeated: +10`, the drop above, 0 `unimplemented host call`; a second one died the same way and the group made a third (wave 1). PNGs looked at (`ex050_s11.png`, `ex050_s15.png`,
`ex050_s25.6.png` in the scratchpad, not committed): idle at 11 s (Sora with the keyblade raised in a ballroom with a tiled floor, two large blue-and-purple round bodies with chained arms behind him, a lock-on
bracket on the nearer; debug text `M_EX050 Idle hp 50/50` twice); a hit at 15 s (Sora flung through the air, one body in front with a sparkle ring and a glowing chain, entries `Hurt hp 38/50` and
`Idle hp 50/50`); a death at 25.6 s (the body flung with open arms, stars, a damage number 3, glowing yellow and green prize orbs around it, entry `Blown hp 0/50`). `combat-ground`, `tt04-dusk` and
`idle-hud` were run by hand through `capped.ps1` (accept.ps1 stops `kh2-game`, rule A10): see the report.

Gaps: (1) Only bb04 b_20 (and bb00 b_02 before the arms) were run; bb04 b_00 / b_01 (mixed with Shadows and `M_EX520`), bb07, bb12 (also `M_EX560`), he05 and he07 (also `M_EX750`, `M_EX510`, which the
app lacks) were not. (2) Not seen: `mode_battle_boss`, `mode_revenge_boss`, `btl_super_hard`, `rvg_*`, `rst_*`, `fight`, `idle`, `idle_time`, `freeze`, `atk_dush_ref` (a reflect state), `event`, `revenge`.
(3) The attack id 148 branch of R0231 (which skips both new calls) was not hit; which attack that is was not looked up. (4) Host 1.13, three sites, was not reached. (5) The +0x11c object write after
host 1.97 (R about 0x079e) was not traced to a reader. (6) 2.85's role is the name table's, the function has no decomp file.
## The enemy M_EX060 (WP-EN10q)

`M_EX060` (object 1, `00objentry` row 1: ObjectType 4, `NeoStatus` 1040, flags 0, Weight 500, `SpawnLimiter` 13, no spawn objects; `enemy::M_EX060`, called by its model: the data gives no name; in the PNGs it is a large orange-robed Heartless with a heart emblem that breathes fire) is a Heartless of al00, al01, al10, al12, al13 and al14 (area survey). Its BDX script (6619 words, `cargo run -p kh2 -- bdx M_EX060`) registers the modes `appear`, `mode_battle`, `btl_normal`, `btl_hard`, `atk_breath`, `atk_fire`, `atk_breaker`, `move_target`, `idle_time` and more (see the dump).

- **Stats.** The set-up routine R17fa (0x17fa, called at 0x0afa) takes eight values after the object; the first pushed, 53 (pushed at 0x0ae2; R17fa stores it to local+0x20), goes to `set_obj_param` (host 2.9 at 0x1821): `enmp` row 53 (Experience 250; log `experience: enmp 53 defeated: +10 (lvpm 4 x enmp 250)`). The harness enemy has 50 HP.
- **Death.** `hp 0: switching to mode dead`, `drop (host 1.92) asked row 64, used row 64`; `Kind::death_effect` is true.
- **Host calls.** Appended to `enemy_host.rs`: 1.83 `bone_to_world` Approx (object position plus the vector, no bone matrix; called inside the callback at 0x0fae that R0fa0 hands to R05ee: without it the callback ended in `script error BadAddress(0) at 0x1017`); 2.10 `attack_create` Stub (a handle) with 2.11, 2.12, 2.13, 2.62 Stubs (no free attack objects exist in the app); 0.44 `effect_stop` Stub. Consequence [gap]: attacks built through 2.10 (the fire and breath attacks seen so far) never hurt Sora. Host 1.13 never left a mode waiting (`queues_motions` unchanged).

Acceptance run (WP-EN10q), al00 `KH2_PROGRAMS=0,2,0 KH2_WAVE_GROUP=b_01 KH2_NO_PARTY=1` (btl program 2 b_01: `M_EX060` and Shadows only; program 0xa b_20 also holds ten `M_EX420` and absent `M_AL020_FIRE`; test `m_ex060_is_an_enemy_kind_with_its_objentry_row`, disc), `KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 kh2-game al00` through `tools/capped.ps1 -CapMB 2500`. (1) Soak without input, 100 s of enemy time (tick 6130 of two enemies): 0 `unimplemented host call`, 0 `script error`; modes run: `appear`, `mode_battle`, `btl_normal`, `btl_hard`, `atk_breath`, `atk_fire`, `atk_breaker`, `move_target`, `idle_time`; each ends and the cycle repeats. (2) Hitting run (Q at 11 s, J, W walks toward them): `M_EX060 hit: atkp 126 damage 3 hp 47 ..` .. `atkp 129 damage 6`, `hp 0: switching to mode dead` (#2, t2074), `experience: enmp 53 defeated: +10`, drop row 64, the group's next wave spawned. PNGs looked at (`local\ex060_*.png`, not committed): idle at 10 s (`ex060_idle.png`: a large orange-robed Heartless with fire at its fists next to Sora, debug text `M_EX060 Idle hp 50/50` twice), a hit (`ex060_hit.png`: the Heartless filling the screen, a damage number 6, golden sparks, debug text `Hurt hp 38/50`). No death PNG was taken (the death is by log only). `idle-hud`, `combat-ground`, `tt04-dusk` PASS in `accept.ps1`, PNG of combat-ground looked at.

Gaps: (1) attack objects (2.10 family) are stubs, so the kind never damages Sora with them; the shape and damage of attack rows were not read. (2) The enemies keep 5 to 13 m away in `move_target` / `idle_time`; a hit run needs Sora to walk to them. (3) Only al00 was run; al01, al10, al12, al13, al14 not; the b_20 group still skips `M_AL020_FIRE`. (4) 1.83 ignores bone matrices (the callback at 0x0fae is not disassembled by `kh2 bdx`, it was read as raw words).

the boss modes and the paths where host 1.77 would answer non-zero (the writer of +0x144 is untraced: searched `mips_scan.py --store-off 0x144`, 43 stores, none read as the enemy actor's). (3) The seed shot's own effects were not looked up.
## The enemy M_EX120 (WP-EN11a)

`M_EX120` (object 72, `00objentry` row 72: ObjectType 4, `NeoStatus` 1035, flags 8, Weight 300, `SpawnLimiter` 9, no spawn objects; `enemy::M_EX120`, called by its model) is a Heartless of al01, al07, al12, al13, mu01 and mu03 (area survey).
Its BDX script (4357 words, `cargo run -p kh2 -- bdx M_EX120`) registers modes `appear`, `mode_battle`, `mode_normal`, `mode_revenge`, `mode_hard`, `move_random`, `idle`, `idle_time`, `atk_aero_stop`, `atk_rush`, `atk_tornado`, `dead`. Stats: the constructor R0ef4 (0x0ef4, called at 0x03d8) takes eight values after the
object (pushed 0x03c0..0x03d5: 1, 49, 300.0, 500.0, 8.0, 4, 0.5, 0.0) and hands the first, 1, to host 2.9 at 0x0f1b: `enmp` row 1 (20 HP at the app's level 1). The second, 49, is kept at +0x1c and is the drop row of host 1.92 (log `drop (host 1.92) asked row 49`).

Host calls: every call the script made in play is already answered (no `unimplemented host call`, no `script error`); no new arm. Stubs in its run: 1.206, 1.171 (global flags 0x80 and 4), 1.121, 65535.0 (frozen); Approx: 1.56, 1.58, 1.111, 1.120, 1.12/1.11 (motion controller), 2.5. Host 1.13 (queue_motion) is
Stub for this kind (not in `queues_motions`); it never waited on it in the soak (the longest gap between mode changes of an instance was 345 ticks, 5.8 s).

Area and harness: mu01 `btl` program 0xa loads b_20 (nine members: four `M_EX120`, two `M_EX420`, two `M_EX210`, plus one more `M_EX420`; all kinds of the app), three kind-2 groups (0x52, 0x53, 0x54) whose boxes lie away from the player entry (-3.8, 1.8 m app), so nothing appears until Sora enters a box. The harness
variable `KH2_WAVE_AT_GROUP=x,z` (app metres, with `KH2_WAVE_GROUP`) now puts Sora there even when the group has a player entry; group 0x53 holds two `M_EX120` and one `M_EX210`: `KH2_WAVE_AT_GROUP=-19,0` (box x -26..-13, z -4.9..4.1 m). Recipe:
`KH2_PROGRAMS=0,a,0 KH2_WAVE_GROUP=b_20 KH2_WAVE_AT_GROUP=-19,0 KH2_YAW=1.57 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_KEYS=8:Q,<J every 0.45 s from 10 s> kh2-game mu01`.

Acceptance run (WP-EN11a): (1) Start: `M_EX120#2`, `#3` `mode appear` at tick 27 -> `mode_battle` / `mode_normal` / `move_random` (tick 107) -> `atk_aero_stop`, `atk_rush`, `atk_tornado`, `idle`, `mode_hard`; hits make them enter `mode_revenge` repeatedly. (2) Hit and death: `M_EX120 hit: atkp 126 damage 3 hp 17` .. `hp 0`,
then `experience: enmp 1 defeated: +6 (lvpm 4 x enmp 150)`, `mode dead`, `drop (host 1.92) asked row 49, used row 49: prizes per kind [0, 0, 2, 0, 0, 0, 0, 4, 0]`, prize pickups (munny +10, drive +1). Both `M_EX120` of group 0x53 died in the 95 s run (party on, J every 0.45 s from 10 s). (3) Soak, 150 s of wall time, no party, J every 2.6 s from 12 s with
Continue after each knock-out (the area reloads and the group respawns each time): about 6600 ticks (110 s) of enemy time, no stuck mode (the modes above, 345 ticks the longest quiet stretch) and the enemies died whenever hit enough; no invulnerable state. (4) Screenshots looked at: idle (`i1.png`, 9 s: a green and teal Heartless with a swirling
antenna and pointed hat beside Sora, Goofy in the foreground, debug text `M_EX120 Idle hp 20/20` twice), hit (`h1.png`, 11.5 s: the Heartless filling the lower right, damage numbers "2", debug text `M_EX120 Hurt hp 5/20`), death (`d1.png`, 19.2 s: debug text `M_EX120 Hurt hp 0/20`, a prize orb and glow beside Sora, the green hat in the foreground). combat-ground, tt04-dusk and idle-hud still pass
`accept.ps1` (PASS; the 2 ERROR lines are in all three).

Gaps: (1) Only mu01 b_20 group 0x53 was run; al01, al07, al12, al13, mu03 (whose b_20 also holds five `M_EX580`, not an app kind) were not. (2) Group 0x54 (two more `M_EX120`) was not entered. (3) The names of the modes `atk_aero_stop`, `atk_rush`, `atk_tornado` are the script's; their attack volumes follow the motion triggers (no new code) and the damage Sora took was not compared with the original.
(4) Host 1.13 was not mapped (see above): if a mode of another area waits on a motion change, add `M_EX120#` to `queues_motions` in `enemy_host.rs`.

## The enemy M_EX130 (WP-EN12b)
`M_EX130` (object 73, `00objentry` row 73: ObjectType 21, `NeoStatus` 1036, `SpawnLimiter` 17, no spawn objects; `enemy::M_EX130`) is a red-hatted Heartless; the survey's "next kinds" row 2 (21 areas, 26 sole groups; bb13 `KH2_PROGRAMS=a` group b_20 holds four beside three `M_EX420`; the first wave is one M_EX130 and three M_EX420, the second wave three M_EX130). `M_EX900` is another worker's package.
Its BDX script (7663 words, `cargo run -p kh2 -- bdx M_EX130`) hands `enmp` row 2 to `set_obj_param` (R1b90 at 0x1b90, called at 0x0e9c with 2 as the first of eight values pushed 0x0e87..0x0e99; host 2.9 at 0x1bb7), so the experience row is enmp 2 (`experience: enmp 2 defeated: +10 (lvpm 4 x enmp 250)`).
Host calls: one new arm, 2.22 `attack_anchor` (0x18e7c8, called at 0x0074 by the attack-hit entry event 17 and by its leak attack), Stub with the other free-attack-object setters (2.10 family): the script asked it (`unimplemented host call 2.22`) in `atk_leak`. The self motion queue (1.13) stays a Stub: no mode of this kind waits on it (longest stay in any mode 180 ticks).
Acceptance run (WP-EN12b), bb13 `KH2_PROGRAMS=a KH2_WAVE_GROUP=b_20 KH2_WAVE_AT_GROUP=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 kh2-game bb13` through `tools/capped.ps1 -CapMB 2500`, keys `8:Q`, J every 0.45 s from 8 to 21.5 s, K (guard) held 22 s for 60 s, then J from 85 s: 0 `unimplemented`, 0 `script error`; both waves cleared (the four M_EX130 and the three M_EX420 reach `dead`, each M_EX130 paying `experience: enmp 2 defeated`); modes seen: appear, mode_battle, mode_normal, atk_leak, atk_flare_spread, atk_flare_rapid, atk_church_bell, mode_revenge, mode_hard, dead; about 2240 enemy ticks (37 s of enemy time at 60 Hz) ran in that run, plus two longer runs of 2460 and 1390 ticks; Sora (20 HP at level 1) was knocked out in several runs, and after a Continue the second wave did not spawn (seen in the forced group, not tied to this kind). Test `m_ex130_is_an_enemy_kind_with_its_objentry_row` (disc).
Screenshots looked at: a red hat Heartless beside M_EX420 with hit sparks and damage numbers (idle/hit, M_EX130 hp 23/32), mid-fight with the M_EX130 at 6/32 beside a lock-on marker, and the death (M_EX130 `Blown hp 0/32`, a fading red hat, a heart prize and munny text).
Gaps: (1) Only bb13 b_20 was run; the other 20 areas were not. (2) Its attack objects are stubs (2.10 family, never hit Sora): the real damage seen came from M_EX420. (3) `M_EX130_AL` and `P_EX130` (survey rows) are separate objects and were not examined. (4) The soak is shorter than 75 s of unbroken enemy time because Sora (20 HP) dies and a Continue ends the second wave; no mode was stuck in any of them.

