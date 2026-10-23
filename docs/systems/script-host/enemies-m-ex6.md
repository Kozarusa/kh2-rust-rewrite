# Enemies: kinds M_EX6xx

[Contract](contract.md) | [Index](../../README.md)

## The enemy M_EX650 (WP-EN10a)

`M_EX650` (object 14, `00objentry` row 14: ObjectType 4, `NeoStatus` 1019, flags 0, Weight 100, `SpawnLimiter` 10, no spawn objects;
`enemy::M_EX650`, called by its model: the data gives no name) is a blue, round cannon-like Heartless of the 13 areas the area survey lists
(ca01, ca02, ca03, ca13, ca18, ca19, ..). Its BDX script (5895 words, `cargo run -p kh2 -- bdx M_EX650`) has the modes `appear`, `mode_battle`,
`mode_battle_boss`, `mode_revenge`, `mode_revenge_boss`, `btl_short`, `btl_long`, `btl_attack`, `idle_time`, `footwork`, `move_random`,
`move_target`, `atk_rakka`, `atk_handou`, `rvg_short`, `rvg_long`, `leave`, `dead`.

- **Stats.** The set-up routine R14e1 (called at 0x0c7a) takes eight arguments; the last one, 65 (pushed at 0x0c62), goes to `set_obj_param`
  (host 2.9 at 0x1508): `enmp` row 65 (`experience: enmp 65 defeated: +2 (lvpm 4 x enmp 50)`). The harness enemy has 14 HP.
- **Death.** The `dead` mode (registered at 0x096e, routine R0e2d -> R0e34) hands the object's +0x1c to host 1.92 (`drop (host 1.92) asked row 35`),
  starts element 2 of its effect file (1.87), releases it (0.79), sets bits 3 and 6, fades out and hides, like the Dusk's: `Kind::death_effect` true.
- **Shared block.** R15f9 (0x15f9, called at 0x0c7c) asks host 2.21 for key 5, size 48 (the first call zeroes it); the word +0x24 it clears is
  read by R1441 (0x1441), which `mode_battle` calls (0x13fc). The tail of R15f9 (bind the object with spawn byte 92 through host 1.143, set +0x28
  to 1, host 1.162) has no reader in this script: the app answers 1.143 with 0 (Stub) and the interpreter's failed write ends the routine early,
  see `docs/systems/script-host/enemy-ai-host-calls.md` "Host call mapping".
- **Modes seen** in a 100 s run: `appear` (about 80 script ticks), `mode_battle` -> `btl_short` / `btl_long` -> `idle_time`, `move_random`,
  `move_target`, `footwork`, `atk_rakka`, `atk_handou`, `btl_attack`, `mode_revenge`, `rvg_short`, `leave`, `dead`. New host arms: only 1.143.
  The ones it reaches that were already answered: 2.21, 1.206 and 1.171 (Stub), 1.56, 1.120, 2.5, 1.55, 1.111, 1.309, 1.60 (Approx).

Acceptance run (WP-EN10a), ca03 (the first listed area with a group of only this kind: `btl` program 0x38 loads b_40 with four `M_EX650` and
four `M_EX210`, which the app does not have; test `m_ex650_is_an_enemy_kind_with_its_objentry_row`, disc): `KH2_PROGRAMS=0,56,0 KH2_WAVE_GROUP=b_40
KH2_WAVE_AT_GROUP=1 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 kh2-game ca03` (Sora in the middle of the
group; `KH2_KEYS` Q at 8 s, J every 0.45 s from 8 s; the shot time must be set: the harness quits right after the screenshot). Log:
`area groups: ca03 btl program 0x38 group b_40: M_EX650, M_EX650, M_EX650, M_EX650 (not in the app: [M_EX210 x4])`, `SetObjParam { a: 65 }`,
`M_EX650 hit: atkp 126 damage 3 hp 11` .. `atkp 128 damage 2 hp 0`, `experience: enmp 65 defeated: +2`, `hp 0: switching to mode dead`, `drop (host 1.92)
asked row 35`, `prize: drop of row 35`. Soak (J from 30 s to 100 s, shot at 100 s): 4 defeats, Sora stayed at 20/20, the others kept cycling
through the modes above, no `unimplemented host call` line, no `Missing` line in the final tables, one `script error BadAddress(2046820372) at
0x162a` (the shared-block tail above). Harness PNGs looked at (`local\ex650_*.png`): idle (HUD present, four `M_EX650 Idle` entries, two blue
cannon-like figures far up on the ledge), a hit (a blue cannon-shaped figure blown back beside Sora, damage number 3, `Blown hp 4/14`), after a
defeat (three entries left, munny and prize orbs near Sora), the end of the soak (one `Hurt hp 2/14`, a blue round figure with a red fuse on the
deck). `idle-hud`, `combat-ground` and `tt04-dusk` of `accept.ps1` PASS and their PNGs show the HUD, Shadows and a Dusk as before.

Gaps: M_EX650 never hurt Sora in the runs (20/20 at 100 s), so its attack hit path (`atk_rakka`, `atk_handou`: host 2.5 HitStun and the ones
that hand a hit record to the player) was not checked; the groups that mix it with Shadows or Soldiers (ca01 b_01, ca02 b_00 and b_40) were run for 25 s at most
(ca01 b_01: two `M_EX650` appeared of the four counted) and not soaked; ca13/ca18/ca19 were not run at all, ca02 only loaded; the
reader of block +0x28 and the role of host 1.162 were not traced (`bdx` shows no `ldf +0x28`; host 1.162 is unnamed in `names.rs`); the object
with spawn byte 92 was not identified.

## The enemy M_EX660 (WP-EN10c)

`M_EX660` (object 304, `00objentry` row 304: ObjectType 4, `NeoStatus` 1020, flags 8, Weight 10, `SpawnLimiter` 7, no spawn objects;
`enemy::M_EX660`, called by its model: the data gives no name) is a small winged Heartless that flies at Sora, in the 11 areas the area
survey lists (al14, ca14, ca15, ca20, he09, lk08, ..). Its BDX script (7449 words, `cargo run -p kh2 -- bdx M_EX660`) has the modes `appear`,
`approach_appear`, `mode_battle`, `mode_battle_boss`, `mode_revenge`, `mode_revenge_boss`, `btl_short`, `btl_long`, `btl_attack`, `btl_normal`,
`btl_hard`, `btl_super_hard`, `idle`, `idle_time`, `footwork`, `fly_over`, `fly_target`, `atk_s_dush`, `atk_b_dush`, `comeon`, `rvg_*`, `rst_*`,
`ref_blow`, `leave`, `freeze`, `dead` (plus `_raw` variants of some).

- **Stats.** The set-up routine R1a7e (0x1a7e, called at 0x080d) takes eight arguments; the eighth, 38 (pushed at 0x07f5), goes to
  `set_obj_param` (host 2.9 at 0x1aa5): `enmp` row 38 (`experience: enmp 38 defeated: +1 (lvpm 4 x enmp 20)`). The harness enemy has 10 HP.
- **Death.** The `dead` mode (registered at 0x0525, routine at 0x0fbc -> R0fca) hands the object's +0x1c to host 1.92 (`drop (host 1.92) asked row 17`),
  starts element 2 of its effect file (1.87 at 0x0fe8), releases it (0.79), fades out and hides, like the Dusk's: `Kind::death_effect` true.
- **Host calls.** Only one was new: 1.99 ground_depth (Impl, see `docs/systems/script-host/contract.md`; asked 175 times in 100 s by the script's flight checks at
  0x13e5 and 0x13f7). Stub or Approx ones it reaches that other kinds already use: 2.21, 1.121, 1.171, 1.206, 1.56, 1.58, 1.309, 1.34, 1.23, 1.87, 1.92, 2.5, 1.111, 1.120.

Acceptance run (WP-EN10c), al14 (the first listed area whose group loads at once; ca14, ca15 and lk08 hold the kind only in kind-2 groups whose
header limit +0x22 is 0, so the app creates nothing there, see Gaps; test `m_ex660_is_an_enemy_kind_with_its_objentry_row`, disc): `KH2_PROGRAMS=0,86,0
KH2_WAVE_GROUP=b_61 KH2_WAVE_AT_GROUP=1 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 kh2-game al14` (btl program 0x56
loads b_61 with six `M_EX660` and five kinds the app lacks: three `M_EX520`, two `M_EX620`; Sora in the middle of the group; `KH2_KEYS` Q at 8 s, J every
0.45 s from 70 s to 108 s, shot at 110 s). Log: `SetObjParam { a: 38 }`, `M_EX660 hit: atkp 127 damage 2 hp 8` .. `atkp 130 damage 5 hp 0`, `experience: enmp 38
defeated: +1`, `hp 0: switching to mode dead`, `drop (host 1.92) asked row 17`, `prize: drop of row 17`; six defeats, 0 `unimplemented host call`, 0 `script error`,
no `Missing` line in the final table. Modes seen: `appear`, `approach_appear`, `mode_battle`, `btl_short`, `btl_long`, `btl_attack`, `fly_over`, `fly_target`, `atk_s_dush`,
`atk_b_dush`, `footwork`, `idle`, `mode_revenge`, `rvg_short`, `dead`. The kind hurts Sora: with no input from 8 s to 70 s he was knocked out once (game over, then
the jump back to a full party, HP 20) and the six kept cycling the modes above until the hits started; none was stuck. Harness PNGs looked at (`local\ex660_*.png`): idle
(HUD present, six `M_EX660 Idle hp 10/10` entries, orange and purple winged cannon-beaked figures hovering around Sora over the sand), a hit run at 14 s (`Hurt`,
Sora HP 12/20 with a pained face, two of them large in front of the camera, entries at hp 8/10), and after the soak (no enemy text, Sora HP 16/20, no enemies in view).
`idle-hud`, `combat-ground` and `tt04-dusk` of `accept.ps1` were run, see the report.

Gaps: (1) Five of about 60 hits (`atkp 124`, hp 10 and a few at hp 3 to 8) did 0 damage with reaction 0 and effective 0 although the enemies had HP left and all six
died later; which state of the script scales the damage to 0 (the damage-scale writers FUN_001a0ba0 and the flight modes) was not traced. (2) The kind-2 groups of ca14 (b_20, b_71),
ca15 (b_21), al14 (b_93 and others) and lk08 (b_20) report `limit 0 (+0x22)` and create no enemy in the app (an app spawn limit, not this kind's script); only al14 b_61 was run.
(3) The reaction motions `ref_blow` and the `rst_*`/`rvg_*` families were not seen except `rvg_short`; the boss variants (`mode_battle_boss`, `mode_revenge_boss`) were not
reached. (4) The `_raw` variants were not traced. (5) Hosts 1.99's flag branch (object +0xe0) is not read. (6) The areas ca14, ca15, ca20, he09 and lk08 were not soaked.
## The enemy M_EX690 (WP-EN10h)

`M_EX690` (object 17, `00objentry` row 17: ObjectType 4, `NeoStatus` 1023, flags 0, Weight 500, `SpawnLimiter` 13, no spawn objects; `enemy::M_EX690`, called by its model: the data gives no
name) is a Heartless of the 10 areas the area survey lists (bb00, bb07, bb11, ca02, ca09, ca15 ..). Its BDX script (4562 words, `cargo run -p kh2 -- bdx M_EX690`) registers the modes
`appear`, `mode_battle`, `mode_normal`, `mode_hard`, `mode_revenge`, `mode_battle_boss`, `mode_revenge_boss`, `btl_*`, `rvg_*`, `rst_*`, `atk_pierce`, `atk_abare`, `atk_counter`,
`move_away`, `idle`, `idle_prize`, `idle_time`, `footwork`, `freeze`, `land`, `leave`, `rc_lancesnatch`, `dead`, `event`, `revenge`.

- **Stats.** The set-up routine R0fe9 (0x0fe9, called at 0x05dd) takes eight values after the object; the first pushed, 67 (pushed at 0x05c5; R0fe9 stores it to local+0x20), goes to
  `set_obj_param` (host 2.9 at 0x1010): `enmp` row 67 (Health 160, MaxDamage 5000, Experience 150, Prize 0; log `experience: enmp 67 defeated: +6 (lvpm 4 x enmp 150)`). The harness enemy has 32 HP.
- **Death.** `hp 0: switching to mode dead`, then `drop (host 1.92) asked row 38, used row 38` and `prize: drop of row 38: 6 objects`; the dead mode starts attached effect 2 (host 1.87), so
  `Kind::death_effect` is true as for the Dusk and `M_EX210`.
- **Host calls.** None was new: every call the app's runs made was answered already, no host arm was added and no motion queue was needed (host 1.13 never ran, so `queues_motions` is unchanged).
  Calls seen with their status (`KH2_BDXLOG` first-call lines): 2.9, 1.23 (Approx), 1.206, 1.171, 0.79, 1.121 (Stubs), 1.56, 1.120, 2.5, 1.80, 1.11, 1.111, 1.55, 1.60, 1.87, 1.92 (Approx), 1.110,
  1.119, 1.124, 1.14, 1.147, 1.15, 1.201, 1.230, 1.28, 1.3, 1.4, 1.57, 1.19, 1.20, 1.70, 1.79, 1.84, 2.23, 2.8 (Impl). In the script but not reached in the runs: 1.13 (three sites), 1.161 / 1.18
  (link), 1.168, 1.179, 1.180, 1.224, 1.128, 1.65, 1.78, 1.10, 1.61, 1.29, 1.67 and the attack-object calls 2.10, 2.11, 2.22, 2.61, 2.62; they are the modes of the Gaps below.
Acceptance run (WP-EN10h), ca15 group b_20 (btl program 0x15: three `M_EX690` and nothing else; kind-2 groups 0x10a (two) and 0x10b (one); test
`m_ex690_is_an_enemy_kind_with_its_objentry_row`, disc). The group's activator volumes are far from Sora's start, so the run starts at entrance 1 and walks: `KH2_PROGRAMS=0,15,0 KH2_WAVE_GROUP=b_20
KH2_ENTRANCE=1 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_KEYS=7:W:2,.. kh2-game ca15` through `local/capped.ps1 -CapMB 2500` (W held 2 s at 7 s walks Sora into
volume 0x10a; the first two are created at the record's points, `created at the record's point at Vec3(22.98, 0.51, -4.23)`). (1) Soak, 103 s with J every 5 s from 15 s only to press "Continue" after Sora's
knock-out (one knock-out): 0 `unimplemented host call`, 0 `script error`; `M_EX690#1` .. `#3` (the third made after a respawn) ran `appear`, `mode_battle`, `mode_normal`, `atk_pierce`, `idle_prize`,
`move_away`, `mode_revenge`, `atk_abare`, `leave`, about 97 s of enemy time; the longest mode runs measured between the `mode` lines: `atk_abare` 360 ticks, `atk_pierce` 296, `move_away` 214,
`idle_prize` 203, `appear` 90, `mode_revenge` 44; the script cycles `mode_battle` -> `mode_normal` -> `atk_pierce` -> `idle_prize` -> .., and `leave` fades the object out (host 1.19, 30 frames) and hides it
(host 1.28, `hidden: despawned`), after which the group makes another. (2) Hitting run, `KH2_KEYS` `7:W:2,10:Q` then J every 0.45 s from 12 s: `M_EX690 hit: atkp 126 damage 3 hp 29 ..` .. `atkp 129 damage 6 hp 0`
(12 hits, `Blown` and `Hurt` reactions), `hp 0: switching to mode dead` for `M_EX690#1` at t1280 (about 21.3 s), `experience: enmp 67 defeated: +6`, the drop above; 0 `unimplemented host call`, 0 `script error`.
PNGs looked at (`local\ex690_*.png`, not committed): idle at 12 s (`ex690_idle.png`: Sora in a rocky ship-wreck passage; the two `M_EX690 Idle hp 32/32` entries of the debug text are 7 m away and off screen),
a hit at 16.5 s (`ex690_s16.5.png`: Sora mid-combo under the lock-on bracket with a golden hit ring, stars and a damage number "6", entries `Idle hp 32/32` and `Blown hp 14/32`), a death at 21.6 s
(`ex690_s21.6.png`: green and purple prize gems, munny rings and stars in the air, an entry `Hurt hp 0/32`). `idle-hud`, `combat-ground` and `tt04-dusk` were run by hand through `capped.ps1` (accept.ps1
stops `kh2-game` processes, rule A10): see the report.

Gaps: (1) Only ca15 b_20 was run. bb00 (b_01: 14 members mixing `M_EX520` and `M_EX690`; b_02: six `M_EX690` and four `M_EX050`, which the app lacks), ca02 (b_21, with `M_EX770` and `M_EX420`),
ca09 (b_20, with `M_EX760`), ca15 b_21 (with `M_EX660`) and bb07 / bb11 were not run. (2) Not seen: the modes `mode_hard`, `mode_battle_boss`, `mode_revenge_boss`, `btl_*` beyond the
`mode_battle` entry, `rvg_*`, `rst_*`, `atk_counter`, `rc_lancesnatch` (a reaction command: the app sends no event 11 message), `freeze`, `land`, `idle_time`, `event`, `revenge`. (3) `M_EX690` never
hurt Sora in the log by a line naming it; Sora was knocked out once in the soak and the `sora hit: atkp 107` lines are not tied to the kind in the log [inferred: the `atk_pierce` / `atk_abare` hits].
(4) What triggers `leave` was not traced (the soak shows it follows `mode_battle` directly, once, after about 83 s of enemy time). (5) The ObjectTargetType 2 of the objentry row was not looked up.

## The enemy M_EX600 (WP-EN10o)

`M_EX600` (object 121, `00objentry` row 121: ObjectType 4, `NeoStatus` 1014, flags 0, Weight 200, `SpawnLimiter` 10, no spawn objects; `enemy::M_EX600`, called by its model: the data gives no name; in the PNGs it is a black and
red Light Cycle bike with a red spike on its back) is a Heartless of Space Paranoids and Hollow Bastion (area survey: hb02, hb11, hb19, he09, tr02, tr03). Its BDX script (5535 words, `cargo run -p kh2 -- bdx M_EX600`)
registers the modes `appear`, `mode_battle`, `mode_battle_boss`, `mode_revenge`, `mode_revenge_boss`, `btl_attack`, `btl_hard`, `btl_long`, `btl_normal`, `btl_short`, `btl_super_hard`, `rvg_*`, `rst_*`, `atk_charge`,
`atk_somersault`, `atk_spin`, `move_back`, `move_slide`, `move_slide_back`, `move_target`, `footwork`, `idle`, `idle_time`, `freeze`, `leave`, `dead`, `event`, `revenge`. The other objentry rows with the name
(`M_EX600_LC`, `_LC_ATK`, `_LC_CHG`, `_LC_GRD`, objects 1675 and 1717-1719, `NeoStatus` 0) are separate objects and not part of this kind; `N_EX600` is a different kind of the app.

- **Stats.** The set-up routine R13af (0x13af, called at 0x066a) takes eight values after the object; the first pushed, 10 (pushed at 0x0652; R13af stores it to local+0x20), goes to `set_obj_param` (host 2.9 at 0x13d6): `enmp`
  row 10 (Experience 100; log `experience: enmp 10 defeated: +4 (lvpm 4 x enmp 100)`). The harness enemy has 20 HP.
- **Death.** `hp 0: switching to mode dead`, then `drop (host 1.92) asked row 42, used row 42` and `prize: drop of row 42 at ..: 4 objects`; `Kind::death_effect` is true as for the Dusk.
- **Host calls.** No arm was added: every call the script made was answered already (43 distinct calls in the 146 s soak; Impl, or the existing Approx / Stub arms: 1.121, 1.171, 1.206 and the mode calls 65535.0 Stub; 1.111, 1.12,
  1.120, 1.23, 1.34, 1.56, 1.60, 1.61, 1.80, 1.87, 1.92, 2.5, 65535.1, 65535.2 Approx). Host 1.13 is in the script once (0x009b, the handler of event 11) and was never called in the runs; `queues_motions` is unchanged.
  No edit to `enemy_host.rs` and `docs/systems/script-host/contract.md`.
Acceptance run (WP-EN10o), tr02 group b_60 (btl program 0x35: two `M_EX600` created at load (spawn kind 3) and four more (spawn kind 2, wave 1) by kind-2 group 0x38, volume shape 2 at game (1020, -100, -129); test
`m_ex600_is_an_enemy_kind_with_its_objentry_row`, disc). Start: `KH2_PROGRAMS=0,35,0 KH2_WAVE_GROUP=b_60 KH2_ENTRANCE=1 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1
KH2_SCREENSHOT=<png> KH2_SHOT_AT=<s> kh2-game tr02` through `local/capped.ps1 -CapMB 2500` (Sora is put at the group's player entry). (1) Soak, shot at 150 s with J every 5 s from 15 s (only to press "Continue" after Sora's
2 knock-outs; enemy ticks stop at game over): 0 `unimplemented host call`, 0 `script error`; up to tick 8760 (about 146 s of enemy time) the instances ran `appear`, `mode_battle` -> `btl_short` / `btl_attack` / `btl_long` ->
`move_slide` / `move_target` / `move_back`, `footwork`, `atk_charge`, `atk_spin`, `atk_somersault`, `mode_revenge` -> `rvg_short`, `idle_time`, `dead` and `leave`; longest runs between `mode` lines: `appear` 271 ticks,
`move_slide` 180, `footwork` 177, `atk_charge` 171, `atk_spin` 146, `move_target` 94; no mode was stuck (nothing above 4.6 s apart from `appear` at 4.5 s). `leave` (motion 45, `spawn_attached` 1, `set_flag` bit 3) ran for
one instance (#4, 29 s after its creation, still alive) and was not looked at in a PNG. (2) Hitting run, `KH2_KEYS` `12:Q` then J every 0.45 s from 13 s: `M_EX600 hit: atkp 126 damage 3 hp 17` .. `atkp 129 damage 6 hp 0`
(about 5 to 8 hits per kill, reactions 3 and 7), `hp 0: switching to mode dead` for `M_EX600#1` (tick 1030), `experience: enmp 10 defeated: +4`, the drop above; in a 100 s hitting run five of the six died (#1 t1030, #2 t1468,
#3 t1875, #5 t2039, #6 t2559) and #4 entered `leave` at t3208. PNGs looked at (`local\ex600_*.png`, not committed): idle (Sora on the dark grid floor, a black and red Light Cycle at lower left and another further right, debug
text `M_EX600 Idle hp 20/20` twice), a hit at 13.7 s (Sora mid-swing, golden hit stars, damage number "3", a blue lock-on reticle, debug text `M_EX600 Hurt hp 17/20`), a death at 17.6 s (debug text `M_EX600 Hurt hp 0/20`, a red
burst, prize gems, a green orb and an "OBTAINED MUNNY!" strip at upper left, damage number "6"). `idle-hud`, `combat-ground` and `tt04-dusk` were not rerun: the change adds a kind and touches no shared code path [not verified].

Gaps: (1) Only tr02 b_60 was run; hb02 b_20 (also `M_EX760`), hb11 b_20 (six `M_EX600` in kind-2 groups 0xfb, 0xfe, 0xff), hb19 b_20, tr03 b_00/b_20 and he09 b_98 were not (he09 b_98 has `Shadow` members).
(2) Not seen: the modes `mode_battle_boss`, `mode_revenge_boss`, `btl_hard`, `btl_normal`, `btl_super_hard`, `rvg_hard` / `rvg_long` / `rvg_normal` / `rvg_super_hard`, `rst_*`, `move_slide_back`, `idle`, `freeze`, `event`,
`revenge`. (3) Why the one instance chose `leave` (the script's condition for entering it was not traced) and what it does after motion 45 (the instance stopped ticking) were not looked into. (4) The visuals of `atk_charge`,
`atk_spin`, `atk_somersault` (host 1.87 `spawn_attached`, Approx) were not looked at. (5) What hurts Sora was not tied to log lines naming the kind [inferred: Sora lost HP and was knocked out twice in the soak]. (6) The
`ObjectTargetType` of the objentry row is 0 (not looked up further).
## The enemy M_EX620 (WP-EN10n)

`M_EX620` (object 11, `00objentry` row 11: ObjectType 4, `NeoStatus` 1016, flags 8, Weight 100, `SpawnLimiter` 20, no spawn objects; `enemy::M_EX620`, called by its model: the data gives no name) is a Heartless of the
Agrabah areas (area survey: al00, al01, al07, al10, al12, al13, al14). Its BDX script (9568 words, `cargo run -p kh2 -- bdx M_EX620`) registers modes including `appear`, `mode_battle`, `mode_revenge`, `btl_attack`,
`btl_short`, `btl_long`, `rvg_short`, `atk_ice`, `atk_crystal`, `footwork`, `idle_move`, `move_target`.

- **Stats.** The set-up routine R2331 (0x2331, called at 0x0f1a) takes eight values after the object; the first pushed, 62 (pushed at 0x0f02; R2331 stores it to local+0x20), goes to `set_obj_param` (host 2.9 at
  0x2358): `enmp` row 62 (Experience 150; log `experience: enmp 62 defeated: +6 (lvpm 4 x enmp 150)`). The harness enemy has 20 HP.
- **Death.** `hp 0: switching to mode dead`, then `drop (host 1.92) asked row 32, used row 32` and `prize: drop of row 32 ..: 6 objects`; `Kind::death_effect` is true as for the Dusk.
- **Host calls.** No arm was added: every call was answered already (Stub: 1.121, 1.171, 1.206, 1.59 and the mode-frozen check; Approx: 1.11, 1.111, 1.120, 1.23, 1.309, 1.56, 1.58, 1.60, 2.5 and mode-may-start). Host
  1.13 on itself was not used; `queues_motions` is unchanged.

Acceptance run (WP-EN10n), al01 group b_01 (btl program 2: six `M_EX620`, two Soldiers, thirteen `M_AL020_ICEE` skipped as not in the app; four kind-2 groups spawn on demand when Sora stands in their volumes;
test `m_ex620_is_an_enemy_kind_with_its_objentry_row`, disc). The harness placed Sora at the centre of the first kind-2 group's members (a temporary local patch, not committed: with the normal player entry no volume
contains him and nothing spawns): `KH2_PROGRAMS=0,2,0 KH2_WAVE_GROUP=b_01 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_NO_PARTY=1 KH2_KEYS=15:J,20:J,.. KH2_SCREENSHOT=<png>
KH2_SHOT_AT=125 kh2-game al01` through `local/capped.ps1 -CapMB 2500`. (1) Soak, 125 s with J every 5 s from 15 s only to press "Continue" after Sora's knock-outs: 0 `unimplemented host call`, 0 `script error`, 0 panics;
the two `M_EX620` ran up to tick 7213 (about 120 s of enemy time): `appear` -> `mode_battle` -> `btl_attack` / `btl_short` / `btl_long` -> `atk_ice` / `atk_crystal` / `footwork` / `idle_move`, `move_target`, and
`mode_revenge` -> `rvg_short` -> `atk_crystal`; longest mode run between `mode` lines: `move_target` 224 ticks, `atk_ice` 172, `footwork` 155, `atk_crystal` 139, `idle_move` 135, `appear` 68; no mode stuck, no invulnerable
state (hp fell on every hit). (2) Hitting run (party on, `KH2_KEYS` 9:Q then J every 0.5 s from 10 s): `M_EX620 hit: atkp 132 damage 3 hp 17` .. `hp 0` for both enemies (6 and 5 hits, `FlinchPush` reactions, a blow at
the last hit of the first), `hp 0: switching to mode dead` at ticks 830 and 1012, `experience: enmp 62 defeated: +6` twice, the drop above twice. PNGs looked at (`local\en10n\*.png`, not committed): idle (`p0.png`, 12 s:
debug text `M_EX620 Idle hp 20/20` twice at 9 and 10 m, the enemies hidden behind market awnings, Sora lower middle), a hit (`hit.png`, 20 s: golden hit stars and the damage number "3", debug text `M_EX620 Hurt hp
11/20` at 1.3 m, the enemy not visible on screen), death (`death.png`, 19 s: after both deaths, "OBTAINED MUNNY 10" and "OBTAINED Energy Gem" strips; the enemies themselves were off screen, so the model of the
dying enemy was not seen). `idle-hud`, `combat-ground` and `tt04-dusk` were not rerun: the change adds a kind and touches no shared code path [not verified].

Gaps: (1) Only al01 b_01 was run; al00, al07, al10, al12, al13, al14 were not. (2) No PNG shows the `M_EX620` model: it stayed off screen or hidden in every shot. (3) Not seen: `mode_battle_boss`-style modes, `rst_*`, `leave`, `event`. (4) Visuals of
`atk_ice` and `atk_crystal` were not looked at. (5) The ObjectTargetType of the objentry row was not looked up. (6) What hits Sora was not tied to log lines naming the kind [inferred: Sora lost HP and was knocked out in the soak].
## The enemy M_EX630 (WP-EN11c)

`M_EX630` (object 12, `00objentry` row 12: ObjectType 4, `NeoStatus` 1017, `SpawnLimiter` 15, no spawn objects; `enemy::M_EX630`, called by its model) is a Heartless of the Agrabah areas (area survey; al03 `btl` program 2 loads b_01 with ten of it, test `m_ex630_is_an_enemy_kind_with_its_objentry_row`).
Its BDX script (5509 words, `cargo run -p kh2 -- bdx M_EX630`) registers modes `appear`, `mode_battle`, `mode_revenge`, `mode_battle_boss`, `mode_revenge_boss`, `leave`, `dead`, `freeze`, `event`, `idle`, `idle_time`, `footwork`, `revenge`, `move_target`, `move_back`, `move_step`, `atk_kasume`, `atk_oi`, `atk_kiriage`, `atk_combo`, `atk_ring`, `atk_ring_end`, `atk_ring_ref` and the
`btl_*` / `rvg_*` / `rst_*` pickers. Stats: the constructor R1396 (0x1396, called at 0x0595) takes eight values after the object (pushed 0x057d..0x0592: 63, 33, 260.0, 400.0, 10.0, 4, 0.7, 0.35) and hands the first, 63, to host 2.9 at 0x13bd: `enmp` row 63 (20 HP at the app's level 1).
The second, 33, is kept at +0x1c and is the drop row of host 1.92 (log `drop (host 1.92) asked row 33`, host 1.92 at 0x077b).

Host calls: every call the script made in play is already answered (no `unimplemented host call`, no `script error`); no new arm. Stubs in its run: 1.121, 65535.0 (frozen); host 1.13 is not in `queues_motions` and the script never waited on it (the longest gap between mode changes was 232 ticks, 3.9 s, over 5862 ticks).

Harness recipe: `KH2_PROGRAMS=0,2,0 KH2_WAVE_GROUP=b_01 KH2_WAVE_AT_GROUP=1 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_KEYS=8:Q,<J every 0.45 s> kh2-game al03`. Kind-2 group 0x1b creates one `M_EX630` (serial 0xf9, wave 1) at the record's point (0, 0, 0).

Acceptance runs (WP-EN11c): (1) idle 98 s of enemy time, no keys after Q: `appear` -> `mode_battle` / `btl_attack` / `move_back` / `atk_oi`, `btl_short` / `move_step`, `atk_kasume`, `btl_long`, `move_target`; no stuck mode, Sora 20/20. (2) hits (J from 14 s): `M_EX630 hit: atkp 126 damage 3 hp 17` .. `atkp 129 damage 6 hp 0`, `experience: enmp 63 defeated: +4 (lvpm 4 x enmp 100)`, `hp 0: switching to mode dead`, `drop (host 1.92) asked row 33`, `prize: drop of row 33`; the instance enters `mode_revenge` 7 times while hit.
(3) Screenshots looked at: idle at 9 s (a dark hooded Heartless with a red scythe beside Sora, lock-on brackets, debug text `M_EX630 Idle hp 20/20`), hit at 12.5 s (`Hurt hp 8/20`, damage number 6, the figure blown behind Sora), death at 17 s ("OBTAINED MUNNY 10", no figure left).

Gaps: (1) Only one `M_EX630` appeared in the runs above because Sora's lunges carried him out of the kind-2 volume (box centred (-2.8, -100, -8.4) cm, half extents 571 x 100 x 910 cm), not because of limit +0x22; while he stays inside, wave 3 is created at once (reviewer's finding). (2) The attack names (`atk_kasume`, `atk_oi`, `atk_kiriage`, `atk_combo`, `atk_ring*`) are the script's; `atk_kiriage`, `atk_combo` and `atk_ring*` did not occur in the soak, and the damage Sora took was not compared with the original.

## The enemy M_EX680 (WP-EN12d)
`M_EX680` (object 16, `00objentry` row 16: ObjectType 21, flags 8, `NeoStatus` 1022, `SpawnLimiter` 17, no spawn objects; `enemy::M_EX680`) is a large purple Heartless with a hammer arm; the survey's "next kinds" row 5 (10 areas; hb04 `KH2_PROGRAMS=0,a,0` group b_20 holds three `M_EX770`, two `M_EX680` and two `M_EX420`; kind-2 group 0x7e is one `M_EX680` and two `M_EX420`).
Its BDX script (12124 words, `cargo run -p kh2 -- bdx M_EX680`) hands `enmp` row 66 to `set_obj_param` (R2d91 at 0x2d91, called at 0x19f4 with 66 as the first of eight values pushed 0x19dc..0x19f1; host 2.9 at 0x2db8), so the experience row is enmp 66 (`experience: enmp 66 defeated: +10 (lvpm 4 x enmp 250)`). No new host arm: every call it makes already exists (Impl, Approx or Stub; none `unimplemented`). Its drop is przt row 37 (host 1.92: six prize objects). The self motion queue (1.13) stays a Stub: the longest stay seen in `atk_punch` is about 140 ticks before it re-enters `mode_battle`.
Harness: the group's kind-2 volume centre is `[203, 325, 2047]` game units, a floor 3.25 m up, so the ground search of `KH2_WAVE_AT_GROUP` must start above it: `KH2_WAVE_AT_GROUP=2.03,20.47,6` (an optional third value is the search height; with two values Sora fell through at y 0 and the wave never started). Run: `KH2_PROGRAMS=0,a,0 KH2_WAVE_GROUP=b_20 KH2_WAVE_AT_GROUP=2.03,20.47,6 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 kh2-game hb04` through `local/capped.ps1 -CapMB 2500`.
Acceptance run (WP-EN12d): 0 `unimplemented`, 0 `script error` over all runs; modes seen: appear, mode_battle, btl_normal, atk_punch, atk_pale, atk_change, mode_revenge, dead. With the party (`KH2_PARTY_WARP=7`, J pressed) the three members die: `bdx M_EX680#1 t1102: hp 0: switching to mode dead`, `experience: enmp 66 defeated: +10`, `M_EX680 (serial 0x20) gone ... defeated for good`, the drop of row 37, and the group clears. Test `m_ex680_is_an_enemy_kind_with_its_objentry_row` (disc).
Screenshot looked at: `hb04` with the M_EX680 idle at 48/50 beside two M_EX420 (the debug text lists `M_EX680 Idle hp 48/50`), a large purple hammer arm over Sora, who is already down (HP 0/20).
Gaps: (1) Soak: Sora (20 HP, level 1) is knocked out about 12 to 28 s after the wave starts in every solo run (enemy tick 1313 to 1680), and the Continue key did not leave the game-over list within the run, so no solo soak reached 75 s of enemy time; with the party the fight ends at tick 678 to 1100 and the group clears. No mode was stuck in any run. (2) Its attack objects are stubs (2.10 family); the damage Sora took came through the enemy-hit path. (3) Only hb04 b_20 was run; the other nine areas were not.

## The enemy M_EX610 (WP-EN15a)
`M_EX610` (object 1114 = 0x45a, `00objentry` row 1114: ObjectType 4, `NeoStatus` 1015, `AnimationName` `M_EX610.mset`; `enemy::M_EX610`) is a slender black and yellow-green armoured Heartless (a pair of curled yellow horns, yellow-green shoulder blades and legs) of Hollow Bastion and Space Paranoids (survey: hb09, hb16, hb18, tr01, tr04, tr09). Its BDX script (7410 words, `cargo run -p kh2 -- bdx M_EX610`) hands `enmp` row 101 to `set_obj_param` (host 2.9 at 0x1b37 inside R1b10, which is called at 0x1024 with 101 pushed at 0x100c, stored to local+0x20), so experience is `experience: enmp 101 defeated: +4 (lvpm 4 x enmp 100)` and the drop is przt row 73 (4 prize objects). Modes seen: appear, mode_battle, btl_short, btl_long, btl_attack, move_slide, move_target, move_back, idle_time, footwork, atk_laser, atk_punch, atk_push, mode_revenge, rvg_short, dead. No new host arm was needed; no `unimplemented host call` and no `script error` in any run.
Run: `KH2_PROGRAMS=0,a,0 KH2_WAVE_GROUP=b_20 KH2_WAVE_AT_GROUP=10.55,16.49,9 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_SCREENSHOT=<png> KH2_SHOT_AT=<s> KH2_KEYS=8:J,8.7:J,.. kh2-game hb16` through `local/capped.ps1 -CapMB 2500`. hb16 `btl` program 0xa group b_20 holds 11 `M_EX610` and 4 `M_EX710` in four kind-2 volumes (0xbf: 3, 0xc0: 5, 0xc1: 2, 0xc2: 5); the game-unit volume centres are mapped to app metres as (x, -y, -z) / 100, so volume 0xc0 (five instances, all M_EX610; its first box is at (10.55, 9.0, 16.49)) was used; standing in 0xc1 (-4.07, 4.0, -3.97) and in 0xc2 (1.86, 1.5, -28.83) first spawned M_EX710, and 0xbf (-2.8, 12.0, 22.77) spawned M_EX610 with no error (not soaked).
Acceptance run (WP-EN15a): (1) kill run (J every 0.7 s from 8 s, 60 s): all five `M_EX610` reach `hp 0: switching to mode dead` (enemy ticks 772, 836, 838, 936, 1051), each `experience: enmp 101 defeated: +4` and `hidden: despawned`; 0 `unimplemented`, 0 `script error`. (2) unattended soak (`KH2_NO_PARTY=1`, J every 9 s from 12 s only to choose Continue, 150 s): one game over, enemy ticks up to 8348 (139 s of enemy time), 0 errors, no instance died (the five and two Continue re-creations), longest single mode 181 ticks (`move_slide`, 3 s), the mode counts show `mode_battle`, `btl_short` + `move_slide` and `btl_long` + `move_target` alternating, `atk_laser` 97, `atk_punch` 25, `atk_push` 7, `mode_revenge` 34 (re-entered by hits), so no mode is stuck. With the party the same group dies out at 38 s of enemy time. Test `m_ex610_is_an_enemy_kind_with_its_objentry_row` (disc).
Screenshots looked at (hb16, `en15a` in the scratchpad): idle at 8 s (`M_EX610 Idle hp 20/20` five times, black armoured figures with yellow-green horns and blades beside Sora and Goofy on a purple cave floor), hit at 14 s (`M_EX610 Idle hp 6/20`, `Idle hp 14/20`, two `Blown hp 0/20`, damage numbers 3, a flash of light with yellow-green gems around Sora), death at 19 s (no enemy text, six yellow-green prize gems on the floor, `NEXT LEVEL 20` and `OBTAINED MUNNY! 10` strips).
Gaps: (1) `M_EX610_RAW` (object 1875, Space Paranoids tr03 b_40 `KH2_PROGRAMS=0,36,0`, 24 members; enmp 101 by the identical script) is NOT in `KINDS`: its objentry `AnimationName` is `M_EX610.mset` (it shares the base kind's motion set) but the app loads `obj/<model>.mset` for a kind, so `obj/M_EX610_RAW.mset` is not in the archive and no instance spawns (`M_EX610_RAW: obj/M_EX610_RAW.mdlx or .mset not loadable`); it needs a motion-set name per kind (enemy.rs loader, outside this WP's hunks). (2) Only hb16 b_20 volume 0xc0 was run; hb09, hb18, tr01, tr04, tr09 were not. (3) Attack objects are stubs as for the other kinds; the damage Sora took was not compared with the original. (4) The `SpawnLimiter` byte (row 1114 +0x2c) was not read.

## The enemy M_EX610_RAW (WP-EN15c)
`M_EX610_RAW` (object 1875, `00objentry` row 1875: ObjectType 4, `NeoStatus` 1015, `AnimationName` `M_EX610.mset`; `enemy::M_EX610_RAW`, enmp 101, the script of `M_EX610`) is the `M_EX610` body of Space Paranoids (survey: tr03 and tr07; tr03 `btl` program 0x36 group b_40 holds 24). It was left out because the rig loader read `obj/<model>.mset`, which does not exist for the RAW model. The loader now takes the motion-set name from the model's objentry row (`enemy::mset_name`: `AnimationName` without ".mset", the model name when the row names nothing), so `M_EX020_RAW` (own `M_EX020_RAW.mset`) and every other kind load as before. The effect file follows the same rule (the model's `.apdx`, else the motion set's).
Run: `KH2_PROGRAMS=0,36,0 KH2_WAVE_GROUP=b_40 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_SCREENSHOT=<png> KH2_SHOT_AT=<s> KH2_KEYS=8:J,9:J,.. kh2-game tr03` through `local/capped.ps1 -CapMB 2500` (the group is placed at Sora's entry; three instances spawn near the player).
Host 1.278 (0x1710e0 -> FUN_001710e0): the death of the kind calls it with (0x20000000, 10) after 1.39; it builds a record from the first argument (FUN_0016b6e0) and starts that many particles with random velocities (FUN_001d0998 -> FUN_001a3b28 on the manager at 0x360668). Added as a Stub (cosmetic, not drawn); nothing reads a result.
Acceptance (WP-EN15c): (1) kill run with the party (J every second from 8 s): `hp 0: switching to mode dead` for #1, #2 and #3 (ticks 617, 768, 1071), each followed by `experience: enmp 101 defeated: +4` and `hidden: despawned`. (2) unattended soak (`KH2_NO_PARTY=1`, J every 9 s from 12 s, 150 s): enemy ticks up to 8400 (140 s), 0 `unimplemented`, 0 `script error`; modes seen appear, mode_battle, btl_short, btl_long, btl_attack, move_slide, move_target, move_back, footwork, idle_time, atk_laser, atk_punch, atk_push, mode_revenge, rvg_short, dead, so no mode is stuck. Tests `mset_name_follows_the_objentry_animation_name`, `m_ex610_raw_loads_with_the_base_motion_set` (disc).
Screenshots looked at (scratchpad `en15c_*`): idle at 10 s (`M_EX610_RAW Idle hp 8/20`, `20/20` twice, the black and yellow-green armoured figures near Sora and Goofy on a Space Paranoids floor), hit at 13 s (`Blown hp 0/20`, damage numbers 1, 4, 4 and gold sparks). The death frame at 58 s shows no enemies and no text (the area is empty after the knock-outs and a Continue); the death is shown by the log lines above.
Gaps: (1) `enemy_ai.rs` line 320 still reads `obj/<model>.mset` for the paired player motions, so the RAW kind logs `obj/M_EX610_RAW.mset: not in archive` and has no paired motions (outside this WP's files; whether `M_EX610` has any was not checked). (2) tr07 was not run. (3) The particles of 1.278 are not drawn.

