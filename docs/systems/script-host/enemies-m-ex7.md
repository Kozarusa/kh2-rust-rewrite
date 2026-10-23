# Enemies: kinds M_EX7xx

[Contract](contract.md) | [Index](../../README.md)

## The enemy M_EX700 (WP-EN9f)

`M_EX700` (object 18 = 0x12, `00objentry` row 18: ObjectType 4, `NeoStatus` 1024, flags 0, weight 10, `SpawnLimiter` 9; `enemy::M_EX700`,
called by its model; row 1833 `M_EX700_NM` is another object with the same model) is a Nobody that the area survey found in 9 areas
(he05, he07, he09, he11, tt13, tt25, ..). tt13's `btl` program 4 loads b_10 with eleven of them beside Soldiers and Shadows (test
`tt13_btl_program_4_holds_eleven_m_ex700`, disc). Its BDX script (5322 words, `cargo run -p kh2 -- bdx M_EX700`) has the modes `appear`,
`mode_battle`, `mode_battle_boss`, `mode_revenge`, `mode_revenge_boss`, `idle`, `idle_time`, `in_look`, `in_move_target`,
`out_move_target`, `in_to_out`, `out_to_in`, `atk_drill`, `atk_upper`, `footwork`, `freeze`, `leave`, `dead`.

- **Stats.** The set-up routine R12d5 (0x12d5) takes eight arguments from its one call (0x0415); the first, 68 (pushed at 0x03fd), goes to
  `set_obj_param` (host 2.9, 0x12fc): `enmp` row 68 (the harness enemy has 14 HP; the log shows `experience: enmp 68 defeated: +2 (lvpm 4 x enmp 50)`).
- **Death.** The `dead` mode (registered at 0x012d, routine R05f0) hands the object's +0x1c to host 1.92, starts element 2 of its effect
  file (host 1.87 at 0x060e), releases it (0.79), sets bits 3 and 6 and fades out: `Kind::death_effect` true.
- **Hosts.** No new host call: every one the script reaches is already in `enemy_host.rs`; the run shows no `Missing` line.

Acceptance run (WP-EN9f), tt13: `KH2_PROGRAMS=0,4,0 KH2_WAVE_GROUP=b_10 KH2_WAVE_AT_GROUP=1 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1
KH2_NO_SOUND=1 KH2_NO_MUSIC=1 kh2-game tt13` (b_10 holds only spawn-kind-2 members, so Sora is put among them). Log: `area group b_10
kind-2 group 0x6b: M_EX700 (serial 0x2e, wave 0) created`, `SetObjParam { a: 68 }`, then the modes `appear` (about 90 script ticks),
`out_move_target`, `atk_drill`, `mode_battle`, `out_to_in`, `in_move_target`, `mode_revenge`. With `KH2_KEYS` (Q at 4 s, J every 0.5 s
from 5.5 s): `M_EX700 hit: atkp 127 damage 2 hp 12` .. `atkp 126 damage 3 hp 0`, `experience: enmp 68 defeated: +2`, `hp 0: switching to
mode dead`, `drop (host 1.92) asked row 39` and `prize: drop of row 39 .. 3 objects`; the screenshot at 10.5 s shows the OBTAINED MUNNY strip
and a prize orb. It is hittable, takes damage, dies and pays, so it is enabled (no invulnerable state was met).

Gaps: only tt13 was run (not he05, he07, he09, he11, tt25); the modes `atk_upper`, `footwork`, `freeze`, `in_look`, `in_to_out` and the
`_boss` modes were not seen; row 39 of the drop table was not traced further; the motion controller is Approx, so the drill and upper
attacks show only what `PlayMotion` gives.

## The enemy M_EX770 (WP-EN10e)

`M_EX770` (object 306, `00objentry` row 306: ObjectType 4, `NeoStatus` 1029, flags 8, Weight 10, `SpawnLimiter` 10, no spawn objects;
`enemy::M_EX770`, called by its model: the data gives no name; rows 221 `M_EX770_NPC`, 1469 `M_EX770_RAW` and 1847 `M_EX770_TR` are other
objects) is a spiked, top-like Heartless of the 11 areas the area survey lists (ca01, ca02, hb01, hb02, hb04, hb11, ..). Its BDX script
(7953 words, `cargo run -p kh2 -- bdx M_EX770`) has the modes `appear`, `mode_battle`, `mode_battle_boss`, `mode_revenge`, `mode_revenge_boss`,
`btl_normal`, `btl_hard`, `btl_super_hard`, `btl_short`, `btl_long`, `btl_attack`, `idle`, `idle_time`, `interval_time`, `footwork`, `fly_target`,
`atk_s_shot`, `atk_s_shot_all`, `atk_b_shot`, `atk_brush`, `rc_catch`, `roll`, `stun`, `rvg_*`, `rst_*`, `leave`, `leave_raw`, `freeze`, `dead`.

- **Stats.** The set-up routine R1d23 (0x1d23, called at 0x0db1) takes eight arguments; the eighth, 37 (pushed at 0x0d99), goes to
  `set_obj_param` (host 2.9 at 0x1d4a): `enmp` row 37 (`experience: enmp 37 defeated: +2 (lvpm 4 x enmp 50)`). The harness enemy has 14 HP.
- **Death.** The `dead` mode hands the object's +0x1c to host 1.92 (`drop (host 1.92) asked row 5`) like the other kinds, fades out and hides:
  `Kind::death_effect` true.
- **Host calls.** One new arm: 1.161 `has_link` on the enemy itself (Approx false, see `docs/systems/script-host/contract.md`; asked at 0x1479 by R145b, which waits
  for the link to end after the `rc_catch` grab, and at 0x165f by R1659). Others it reaches that other kinds already answer: 1.23, 1.58, 1.11, 1.12,
  1.206, 1.171, 1.56, 1.121, 1.111, 1.120, 1.90, 65535.0 / .1 / .2.
- **Grab.** R086f (0x086f: motion 241, mode `rc_catch`, `link_start` 1.18 on the player with 0x4002 / 1, then motion 253) is the catch: 1.18 on the
  player is a Stub, so Sora is not attached; R145b's wait ends at once and the enemy returns to `idle`.

Acceptance run (WP-EN10e), hb01 (one `M_EX770` in a group of only this kind; test `m_ex770_is_an_enemy_kind_with_its_objentry_row`, disc):
`KH2_PROGRAMS=0,1,0 KH2_WAVE_GROUP=b_00 KH2_WAVE_AT_GROUP=1 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1
kh2-game hb01` (`KH2_PROGRAMS` is hex: the M_EX660 section's `KH2_PROGRAMS=0,86,0` for btl program 0x56 therefore means 0x86, the program for 0x56 is `0,56,0`; `KH2_KEYS` Q at 8 s then J every 0.45 s from 10 s). Log: `area group b_00 kind-2 group 0xe4: M_EX770 (serial
0x18, wave 0) created at the record's point`, `SetObjParam { obj: 16781312, a: 37, b: 0 }`, `M_EX770 hit: atkp 126 damage 3 hp 11` .. `atkp 128 damage
2 hp 0`, `experience: enmp 37 defeated: +2`, `hp 0: switching to mode dead`, `drop (host 1.92) asked row 5`, `prize: drop of row 5`. Soak with no
input for 95 s (shot at 95 s): one `M_EX770`, Sora stayed at 20/20, modes `appear`, `mode_battle` -> `btl_normal` -> `fly_target` (every 120 to 240
ticks), `idle`, `atk_s_shot`, no stuck state, no `script error`; its one `Missing` host call (1.161 on the object itself) is now answered. Second
real area, ca02 (`KH2_PROGRAMS=0,a,0 KH2_WAVE_GROUP=b_20`, nine `M_EX770` and eight `M_EX420` in the group, three created at once: two `M_EX770` and one
`M_EX420`): 95 s unattended soak (Sora knocked out by `atkp 336` / `338` hits, which are not `M_EX770`'s [inferred: the `M_EX420` claws]; both `M_EX770` kept cycling `mode_battle`,
`btl_normal`, `fly_target`, `idle`, `footwork`, `atk_s_shot`, `has_link` Approx x4, no `unimplemented host call`), and a 70 s hitting run (J from 10 s):
`M_EX420 defeated: +4`, one `M_EX770` defeated (`switching to mode dead`, +2), 8 `M_EX770 hit` lines. Harness PNGs looked at (`local\ex770_*.png`):
idle (hb01: a brown, spiked top-shaped figure on the ice beside Sora, `M_EX770 Idle hp 14/14`), a hit (hb01 at 11.6 s: the figure in front of
the camera under the lock-on bracket, Sora mid-attack with stars and a damage number, `M_EX770 Hurt hp 8/14`), after the defeat (damage number, NEXT
LEVEL strip), ca02 (two `M_EX770 Hurt hp 12/14` and `11/14` entries, Sora HP 13/20, the figure beside a wall). `idle-hud`, `combat-ground` and
`tt04-dusk` of `accept.ps1` were run, see the report.

Gaps: (1) `M_EX770` never hurt Sora in the runs: its `atk_s_shot` / `atk_s_shot_all` / `atk_b_shot` / `atk_brush` hit paths (the shot objects and the
host calls that hand a hit record to the player) were not checked; the `rc_catch` grab is not carried (1.18 on the player is a Stub). (2) Only
hb01 b_00 and ca02 b_20 create `M_EX770` in the app; ca01 b_21, hb02 b_21, hb04 b_20 / b_21 / b_00 and hb11 b_00 / b_01 / b_02 are kind-2 groups
that create nothing with the harness (`limit 0 (+0x22)`, Sora not in their volumes), so they were not run; hb02's mixed groups were not run at all.
(3) The boss variants (`mode_battle_boss`, `mode_revenge_boss`), `roll`, `stun`, `rc_catch`, `leave`, `rvg_*` and `rst_*` modes were not seen. (4) The `_RAW` and
`_TR` objects were not traced. (5) The reader of +0x570 for an enemy (`FUN_001beb00`) and the writer of an enemy's link (host 1.18 only
writes the first object's, the player's) were not searched with `mips_scan --store-off 0x570`.

WP-FIX4 (M_EX770's `atk_b_shot` in hb04 b_20, group 0x7d): with the party present (no `KH2_NO_PARTY`; `KH2_PROGRAMS=0,a,0 KH2_WAVE_GROUP=b_20
KH2_WAVE_AT_GROUP=1,-1,6`, Sora at the centre of kind-2 group 0x7d) an `M_EX770` in `atk_b_shot` logged `unimplemented host call` 1.238, 1.21, 0.38,
0.41, 0.51 and also 0.50, 2.39, 2.14. The mode (its effect block at script words 0x0370..0x0580 and 0x1908..0x1b40, both reached only through the
mode table, so `kh2 bdx` shows them as `.data`) starts effect element 7 on the object (1.21), places and turns it (0.38, 0.41), waits for it
(0.50, the attack's 2.14 / 2.39, a 60.0 countdown at 0x0538) and destroys it (0.51); R06f9 asks 1.238 of its own handle at 0x0708. Now: 1.238 on any
object other than the player answers 0 (Approx), 1.21 on the object itself starts the element on the owner (Approx, `Obj::effects`), 0.38 / 0.41 are
Stubs (the effect stays on its owner), 0.49 / 0.50 answer live until 0.51 destroys the handle (`Obj::effect_live`, `Obj::destroy_effect`), and 2.14 / 2.39
answer false (Stub: the app has no free attack objects, so none has hit). Soaks (130 s, with Continue, one with the party, one without): no
`unimplemented host call`, `atk_b_shot` entered and left, no mode longer than 24 s (`fly_target`). The shot's damage is still not simulated (gap 1).
## The enemy M_EX730 (WP-EN10i)

`M_EX730` (object 123, `00objentry` row 123: ObjectType 4, `NeoStatus` 1027, flags 0, Weight 100, `SpawnLimiter` 10, no spawn objects; `enemy::M_EX730`, called by its model: the data gives no
name; in the PNGs it is a teal Heartless with a pointed hat) is a Heartless of the areas ca01, ca09, ca12, ca13, ca18, ca21 (area survey, 10 areas). Its BDX script (5215 words,
`cargo run -p kh2 -- bdx M_EX730`) registers the modes `appear`, `mode_battle`, `mode_revenge`, `mode_battle_boss`, `mode_revenge_boss`, `btl_attack`, `btl_short`, `btl_long`, `btl_normal`,
`btl_hard`, `btl_super_hard`, `rvg_*`, `rst_*`, `atk_slap`, `atk_hit`, `atk_charge`, `move_back`, `move_back_jump`, `move_roll_jump`, `move_target`, `footwork`, `idle`, `idle_time`, `freeze`,
`leave`, `rc_tornado`, `test_rc_tornado`, `dead`, `event`, `revenge`.

- **Stats.** The set-up routine R1268 (0x1268, called at 0x05ba) takes eight values after the object; the first pushed, 12 (pushed at 0x05a2; R1268 stores it to local+0x20), goes to `set_obj_param`
  (host 2.9 at 0x128f): `enmp` row 12 (Health 100, MaxDamage 10000, Experience 100, Prize 0; log `experience: enmp 12 defeated: +4 (lvpm 4 x enmp 100)`). The harness enemy has 20 HP.
- **Death.** `hp 0: switching to mode dead`, then `drop (host 1.92) asked row 44, used row 44` and `prize: drop of row 44 at ..: 4 objects, kinds [0, 0, 3, 3]`; `Kind::death_effect` is true as for the Dusk.
- **Host calls.** Two arms were appended to `enemy_host.rs`: 1.22 `spawn_effect_at` (0x1717c0, called at 0x00e9 with element 12 at a bone and at 0x0c8c with element 9; Stub: a handle is returned,
  the element at the script's vector is not drawn because the app only starts effects on their owner) and 0.52 `effect_fade` (R0d4f at 0x0d63 fades a held handle; Stub, nothing to fade). Every other call
  the runs made was answered already. Host 1.13 on itself (Stub unless the kind is tagged in `queues_motions`) never left a mode waiting: the longest mode run in the soaks was `atk_charge`
  with 333 ticks (about 5.5 s); `queues_motions` is unchanged. Host 1.12 `play_motion_checked` answered None and was repeated every tick by `M_EX730#1` in the first soak run for about 10 ticks at least (motion 2, blend 8); no stuck mode followed.
Acceptance run (WP-EN10i), ca09 group b_02 (btl program 3: three `M_EX730`, plus three `M_EX560` the app lacks; kind-2 groups 0x1ad (two, volume at game (-657, 25, -649)) and 0x1ae (one, volume at
(1084, 25, 291)); test `m_ex730_is_an_enemy_kind_with_its_objentry_row`, disc). The run starts at entrance 1 (game (1641, -39, -555)) and walks: `KH2_PROGRAMS=0,3,0 KH2_WAVE_GROUP=b_02 KH2_ENTRANCE=1
KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_KEYS=7:A:0.7,8:W:4,.. KH2_SCREENSHOT=<png> KH2_SHOT_AT=<s> kh2-game ca09` through `local/capped.ps1 -CapMB 2500`
(KH2_KEYS only works with KH2_SCREENSHOT set; A held 0.7 s at 7 s then W held 4 s at 8 s walks Sora into volume 0x1ae). (1) Soak, 125 s with J every 5 s from 14 s (Sora was knocked out
at least once and came back through the game-over menu on its own), run twice, the second with the final code: 0 `unimplemented host call`, 0 `script error`; the three enemies ran `appear`, `mode_battle` (-> `btl_attack`
-> `move_back_jump` / `atk_slap`, `btl_short` -> `atk_hit` / `idle_time`, `btl_long` -> `move_target`), `footwork`, `idle`, `mode_revenge` -> `rvg_short` -> `atk_charge`, about 110 s of enemy time (tick
6731); the longest mode runs between `mode` lines: `atk_charge` 333 ticks, `footwork` 200, `move_target` 166, `idle` 88, `appear` 81, `atk_slap` 74. (2) Hitting run, `KH2_KEYS` `7:A:0.7,8:W:4,12:Q` then
J every 0.45 s from 13 s: `M_EX730 hit: atkp 128 damage 2 hp 18` .. `atkp 129 damage 6 hp 0` (8 hits per kill, `FlinchPush` and `Blow` reactions), `hp 0: switching to mode dead` for `M_EX730#3` (about 21 s) and
`#2` (about 37 s), `experience: enmp 12 defeated: +4` twice, the drop above; 0 `unimplemented host call`. The third (`#1`, volume 0x1ae) was not hit in that run. PNGs looked at (`local\ex730_*.png`, not
committed): idle at 12 s (`ex730_s12.png`: Sora on a rocky ledge under a night sky; debug text `M_EX730 Idle hp 20/20`; the enemy itself is 13 m away and off screen), a hit at 16.5 s (`ex730_s16.5.png`:
Sora under the lock-on with golden hit stars and damage numbers "1" and "2", a large teal-hatted Heartless in a hurt pose, entry `Hurt hp 17/20`), a death at 21.6 s (`ex730_s21.6.png`: a pink heart,
green and yellow prize gems, a "Obtained munny" strip and "4p" in the air). `idle-hud`, `combat-ground` and `tt04-dusk` were run by hand through `capped.ps1` (accept.ps1 stops `kh2-game`, rule A10):
log expectations met (`hud: edge layout`, `move:   chosen key`, `Dusk` / `mode_battle`) and the PNGs looked at (idle HUD without enemies and reticle; Sora mid-combo with Shadows flinching; Dusk hurt in the Sandlot).

Gaps: (1) Only ca09 b_02 was run; ca01 b_00 (with Shadows), ca12 b_01 (with `M_EX210`), ca13 b_01 (with `M_EX650`, `Soldier`, `M_EX210`), ca18 b_03, ca21 b_00 (with `M_EX650`) were not. (2) Not seen: the modes
`mode_battle_boss`, `mode_revenge_boss`, `btl_normal`, `btl_hard`, `btl_super_hard`, `rvg_*` beyond `rvg_short`, `rst_*`, `move_back`, `move_roll_jump`, `freeze`, `leave`, `rc_tornado` (a reaction command: the app sends
no event 11 message), `event`, `revenge`. (3) The elements of host 1.22 (12 at a bone, 9 at a position) and the `atk_charge` visuals are not drawn; which file holds them was not looked up. (4) Host 1.13
and 1.12 were not traced further than the soak (no mode stuck). (5) The ObjectTargetType of the objentry row was not looked up. (6) What hits Sora (`atk_slap`, `atk_hit`, `atk_charge`) was not tied to the log
lines naming the kind [inferred: Sora lost HP and was knocked out during the soak].

## The enemy M_EX750 (WP-EN10m)

`M_EX750` (object 125, `00objentry` row 125: ObjectType 4, `NeoStatus` 1045, flags 0, Weight 100, `SpawnLimiter` 13, no spawn objects; `enemy::M_EX750`, called by its model: the data gives no
name; in the PNGs it is a small orange four-petal flower with a teal centre, a stationary seed shooter) is a Heartless of the areas he05 and he07 (area survey and the he05 group log). Its BDX script (5899 words,
`cargo run -p kh2 -- bdx M_EX750`) registers the modes `appear`, `mode_battle`, `mode_revenge`, `btl_short`, `btl_long`, `btl_down`, `rvg_short`, `rvg_long`, `atk_seed_shot`, `atk_needle`, `footwork`, `idle_time`,
`route_panic`, `leave`, `dead` and more (see the dump).

- **Stats.** The set-up routine R14f0 (0x14f0, called at 0x09b9) takes eight values after the object; the first pushed, 14 (pushed at 0x09a1; R14f0 stores it to local+0x20), goes to `set_obj_param`
  (host 2.9 at 0x1517): `enmp` row 14 (Experience 100; log `experience: enmp 14 defeated: +4 (lvpm 4 x enmp 100)`). The harness enemy has 20 HP.
- **Death.** `hp 0: switching to mode dead`, then `drop (host 1.92) asked row 123, used row 123: .. 10 prize objects` and a second drop of row 52 (4 objects, kinds [3, 3, 5, 5]) in some kills; `Kind::death_effect` is true as for the Dusk.
- **Host calls.** One arm was appended to `enemy_host.rs`: 1.77 `has_word_144` (0x172d60, `*(*(*obj)+4)+0x144 != 0`), Stub answering 0. Its sites: 0x0561 (the damage handler after hit reaction 7, 11 or 5: with 0 the script prints `motion_hook?` and calls R057f / R059e), 0x0e72 (with 0 it enters `route_panic` near a distance test of 4000), 0x13c3 and 0x14c3 (a non-zero word selects `btl_down`). The scan of word stores
  (`mips_scan.py --store-off 0x144`) gave 43 stores in functions of several classes; which one writes an enemy actor was not traced. Every other call the runs made was answered already; host 1.13 never left a mode waiting
  (`queues_motions` is unchanged).
Acceptance run (WP-EN10m), he05 group b_00 / b_01 (kind-2 groups: b_00 0x38 three, 0x39 four, 0x3a four, 0x3d two `M_EX750` beside `M_EX520` and `M_EX690`; btl program 2 b_01: four `M_EX750` and three `M_EX050`; test
`m_ex750_is_an_enemy_kind_with_its_objentry_row`, disc). The kind-2 volumes are far from the group's player entry, so the first run used a throw-away local patch that put Sora at the first volume of group 0x38 (a later run reached it without a patch: `KH2_ENTRANCE=0 KH2_YAW=3.14 KH2_KEYS=2:W:8,...` walks Sora from entrance 0 into the volume) (game (-2613.88,
-125, 3316.37), not committed; `KH2_PROGRAMS=0,1,0 KH2_WAVE_GROUP=b_00 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_SCREENSHOT=<png> KH2_SHOT_AT=<s> KH2_KEYS=..
kh2-game he05` through `local/capped.ps1 -CapMB 2500`). Three `M_EX750` were created at the record's points (about 4 m from Sora). (1) Soak, 172 s of wall time (tick 6060, about 101 s of enemy time) with J every
second from 14 s: 0 `unimplemented host call` after the 1.77 arm, 0 `script error`; modes run: `appear`, `mode_battle`, `btl_short` / `btl_long`, `atk_seed_shot`, `idle_time`, `footwork`, `mode_revenge` -> `rvg_short` ->
`atk_needle`, `dead`; the longest mode runs: `footwork` 197 ticks, `atk_seed_shot` 108, `idle_time` about 107; no mode stuck. (2) Hitting run, Q at 11 s, J every 0.45 s from 12 s and S held 0.5 s every 1.2 s from 13 s
(130 s): 24 hits (`M_EX750 hit: atkp 126 damage 3 hp 17 ..` with `FlinchPush` and `Blow` reactions), `hp 0: switching to mode dead` for `#2` (t987), `#3` (t1739) and `#1` (t3121), each followed by
`experience: enmp 14 defeated: +4` and the drops. PNGs looked at (`local\ex750_*.png`, not committed): idle at 10 s (`ex750_idle.png`: Sora in a green rocky valley with two orange four-petal flowers with teal centres near
him; debug text `M_EX750 Idle hp 20/20` three times, d 3.8 to 4.5), a hit at 16 s (`ex750_hit.png`: golden hit sparks and a damage number "1" at Sora, lock-on reticle, green prize orbs; entries `Hurt hp 5/20`,
`Idle hp 20/20` twice), a death at 17.6 s (`ex750_d1.png`: only two enemies listed, a green prize orb at the right edge, Sora with the keyblade, sparks). `idle-hud`, `combat-ground` and `tt04-dusk` were run
by hand (see the report).

Gaps: (1) Only he05 was run; he07 was not. The kind-2 spawn volumes of he05 are far from the group's player entry (a gap of the harness, not of the kind): a harness has to walk Sora to a volume (no committed aid; use `KH2_ENTRANCE=0 KH2_YAW=3.14 KH2_KEYS=2:W:8,...`). (2) Not seen: `btl_down`, `route_panic`, `leave`, `rvg_long`,
the boss modes and the paths where host 1.77 would answer non-zero (the writer of +0x144 is untraced: searched `mips_scan.py --store-off 0x144`, 43 stores, none read as the enemy actor's). (3) The seed shot's own effects were not looked up.
## The enemy M_EX780 (WP-EN12f)
`M_EX780` (object 20, `00objentry` row 20: ObjectType 4, `NeoStatus` 1039, `SpawnLimiter` 7, no spawn objects; `enemy::M_EX780`) is a Pride Lands Heartless; the survey's "next kinds" row 8 (lk03, lk04, lk05, lk07, lk08; lk03 `KH2_PROGRAMS=1` group b_00 holds it beside Shadows and `M_EX110`; kind-2 group 0x21 has three, 0x22 six members).
Its BDX script (5588 words, `cargo run -p kh2 -- bdx M_EX780`) hands `enmp` row 81 to `set_obj_param` (host 2.9 at 0x1429 inside R1402; called at 0x0aa8 with 81 pushed at 0x0a90), so experience is `experience: enmp 81 defeated: +4 (lvpm 4 x enmp 100)`. No new host arm: its calls already exist (Impl, Approx or Stub; none `unimplemented`; Stubs seen: GlobalFlag80, GlobalFlag4, SetVelocity, ReleaseEffect). The self motion queue (1.13) stays a Stub: no mode stayed beyond about 270 ticks.
Run: `KH2_PROGRAMS=1 KH2_WAVE_GROUP=b_00 KH2_WAVE_AT_GROUP=12,21 KH2_PARTY_WARP=7 KH2_AREALOG=1 KH2_BDXLOG=1 kh2-game lk03` through `local/capped.ps1 -CapMB 2500`, keys `6:Q` then J every second from 8 s; `KH2_WAVE_AT_GROUP=12,21` stands Sora in kind-2 group 0x22's volume (without it the group is not entered, and `-11,-30` from the survey spawned nothing). Without `KH2_SCREENSHOT` the game runs until it ends itself; a screenshot ends the run.
Acceptance run (WP-EN12f): 0 `unimplemented`, 0 `script error`; modes seen: appear, mode_battle, btl_attack, btl_short, btl_long, move_target, atk_jab, atk_charge, atk_fook, mode_revenge, rvg_short, dead. About 6300 enemy ticks (105 s of enemy time at 60 Hz) in one run: both M_EX780 reach `hp 0: switching to mode dead` (ticks 3540 and 4915), pay `experience: enmp 81 defeated: +4`, `M_EX780 (serial 0xd) gone ... defeated for good`, and group 0x22 clears (two M_EX780, three Shadows, one M_EX110); Sora was not knocked out, so no Continue was needed. Test `m_ex780_is_an_enemy_kind_with_its_objentry_row` (disc).
Screenshot looked at: lk03 at 20 s: Sora in melee with sparks, debug text `M_EX780 Idle hp 16/20` beside three Shadows and an M_EX110 (idle and hit). No death screenshot of M_EX780 was taken (a later shot at 30 s shows only a Shadow at 1/10 left); death is shown by the log lines above.
Gaps: (1) Only lk03 b_00 was run; lk04, lk05, lk07, lk08 were not. (2) Attack objects are stubs (2.10 family): the damage seen is small (Sora 20/20 to 17/20 at 59 s). (3) `M_EX670` and `M_EX720`, which share its groups, are not kinds.

## The enemy M_EX740 (WP-EN13a)
`M_EX740` (object 124 = 0x7c, `00objentry` row 124: ObjectType 4, flags 1, `NeoStatus` 1044, `SpawnLimiter` 7, no spawn objects; `enemy::M_EX740`) is a large red, yellow and blue Heartless that heals the others; the survey's "next kinds" row 9 (he09, he15, he16, tt39; he16 `KH2_PROGRAMS=a` group b_20, kind-2 group 0x3b: three `M_EX740` and two `M_EX420`; he15 holds groups of 9 and 15 with `M_EX420` and `M_EX730`).
Its BDX script (5677 words, `cargo run -p kh2 -- bdx M_EX740`) hands `enmp` row 13 to `set_obj_param` (host 2.9 at 0x1474), so experience is `experience: enmp 13 defeated: +2 (lvpm 4 x enmp 50)`. Modes seen: appear, mode_battle, move_random, idle_time, footwork, step_lr, atk_heal (it heals the others; no damage to Sora was seen from it), atk_hit, atk_call, rvg_short, mode_revenge, leave, dead.
New host arms (both Approx, in `enemy_host.rs`): `7.33` (wrapper FUN_002ef8f8 -> FUN_002f4080, read from the decomp) and `1.0` create_object of id 0. The script's event 17 (motion frame trigger 7, BDX R0782 at 0x0782) asks 7.33 for an objentry id and makes it beside itself with 1.0 (facing vector times 50, 250 below). FUN_002f4080 walks the eight ids at DAT_00371580 (on the disc `M_EX710`, `M_EX130`, `M_EX730`, `M_EX750`, `M_EX700`, `M_EX550`, `M_EX770`, `M_EX760`), keeps those whose object flags (FUN_0016f038) allow it and whose `SpawnLimiter` byte fits the budget DAT_01c893b8 minus DAT_01c893bc (FUN_00181408), and returns a random one; with none it returns the first word of the record at +8 (FUN_00169958). The app does not read the list or the budget (their writers were not traced: gap), so 7.33 returns 0, the fallback, and 1.0 with id 0 answers a null handle (`objentry` has no row 0 [inferred]); the script's validity test then skips the placement. The original's M_EX740 therefore calls other Heartless in on that trigger; the app's does not.
Run: `KH2_PROGRAMS=a KH2_WAVE_GROUP=b_20 KH2_WAVE_AT_GROUP=-4,3.5 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_SCREENSHOT=<png> KH2_SHOT_AT=<s> KH2_KEYS=7:Q,8:J,9:J,.. kh2-game he16` through `local/capped.ps1 -CapMB 2500`. `KH2_KEYS` runs only when `KH2_SCREENSHOT` is set, and the game quits at the shot, so a long soak sets `KH2_SHOT_AT=170`. The survey's centre (-11, 5) puts Sora 8 to 10 away from the three `M_EX740`, which stay passive, so (-4, 3.5) was used.
Acceptance run (WP-EN13a): (1) unattended 151 s of enemy time (9049 ticks, no keys, Sora not hit): 0 `unimplemented`, 0 `script error`; the three `M_EX740` cycle appear, mode_battle, move_random, idle_time, footwork, atk_heal, step_lr; the longest time in one mode was 4.1 s. (2) hit and kill run (J every second from 8 s, Q every 10 s, 173 s): 8 `M_EX740 hit` lines, two reach `hp 0: switching to mode dead` (ticks 1941 and 2121), each `experience: enmp 13 defeated: +2` and `M_EX740 (serial ..) gone ... defeated for good`; the third enters `leave` after being hit (`gone ... defeated for good` with no experience), so the group stays clearable; the two `M_EX420` die at ticks 921 and 1461 (`experience: enmp 9 defeated: +4`). After 36 s of enemy time no enemy is left and the run is quiet (kind-2 groups 0x3c and 0x3d did not spawn: not entered). Before the 7.33 and 1.0 arms the same run printed `unimplemented host call 7.33` and `1.0 (create_object)` once, from `atk_call` (the script then went on: the handle was null).
Screenshots looked at (he16, `local\en13a\*.png` in the scratchpad): idle at 8 s (debug text `M_EX740 Idle hp 14/14` three times beside two `M_EX420`; a horn-shaped Heartless with music notes in front of Sora and an `M_EX420` in a lock-on ring), hit at 14 s (`M_EX740 Idle hp 14/14` beside an `M_EX420 Hurt hp 14/20`, damage number 3, one `M_EX740` large in the foreground), hurt at 33.5 s (`M_EX740 Hurt hp 5/14` beside Sora, "OBTAINED Bright Crystal" strip from the `M_EX420` drop), death at 36.5 s (no enemy in the debug text, damage number 2, "NEXT LEVEL 28", a prize orb in the air).
Gaps: (1) The 7.33 candidate list and budget (above): the original's summon of Heartless by `atk_call` is not done. (2) Only he16 b_20 group 0x3b was run; he09, he15 (groups of 9 and 15) and tt39 were not, and the large he15 waves were not soaked. (3) Whether `leave` after a hit is the original's behaviour or an effect of the approximated 7.33 was not traced. (4) Attack objects are stubs as for the other kinds: no damage from `M_EX740` was seen.

## The enemy M_EX710 (WP-EN14a)
`M_EX710` (object 122 = 0x7a, `00objentry` row 122: ObjectType 21, `NeoStatus` 1025, no spawn objects; `enemy::M_EX710`) is a large green spiked ball with two yellow spiked fists on springs; the survey's "next kinds" row 3 and its widest kind (28 areas, 37 sole groups). Its BDX script (6782 words, `cargo run -p kh2 -- bdx M_EX710`) hands `enmp` row 11 to `set_obj_param` (host 2.9 at 0x189e inside R1877; the 11 is pushed at 0x0809 before the drop row 43), so experience is `experience: enmp 11 defeated: +10 (lvpm 4 x enmp 250)` and the drop is przt row 43 (6 prize objects). Modes seen: appear, mode_battle, btl_normal, atk_mow, atk_strike, atk_revolve, atk_press, change_base_pos, mode_revenge (re-entered by each hit), idle, jump, leave, dead. Registered but never entered by any script (no `set_mode`/`queue_mode` names them and no run reached them): seek_target, move_back, idle_time, footwork, spike.
Mode choice: `mode_battle` (R1768) picks `btl_normal` (R16f6: `atk_mow` or `atk_strike` by distance, `idle` then `jump` or `move_front`, or `change_base_pos` + `atk_revolve`) until the first damage drops the hit-point ratio under the threshold at context +0x40 (R03ca writes context +0x18 = 1 in the damage handler); after that `mode_battle` picks `atk_press` every time (jump with motion 203, press with 204, land 205).
New host arms (`enemy_host.rs`): 1.247 random_free_point (Approx, shared `party_rules::free_point`), 2.24 hit_attacker_block (Approx: the player block) and 2.36 nop_2_36 (Impl, empty function). The survey also listed 1.247 and 2.36 as the missing calls; 2.24 appeared at the first hit (`unimplemented host call 2.24`, then `script error BadAddress(0) at 0x11e5` in the damage routine R11d8).
Run: `KH2_PROGRAMS=0,4,0 KH2_WAVE_GROUP=b_10 KH2_WAVE_AT_GROUP=0,-6,40.13 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_SCREENSHOT=<png> KH2_SHOT_AT=<s> KH2_KEYS=7:Q,6:J,6.6:J,.. kh2-game tt39` through `local/capped.ps1 -CapMB 2500`. The third value of `KH2_WAVE_AT_GROUP` is the ground search height in app metres: the survey's y 4013 is game units, 40.13 m (with 4013 Sora falls for ever). The kind-2 group 0x9c (two `M_EX710`) spawns at once and its members appear 1.4 m from Sora.
Acceptance run (WP-EN14a): (1) kill run (J every 0.6 s from 6 s, Q every 8 s, 180 s): the two `M_EX710` of group 0x9c: one reaches `hp 0: switching to mode dead` (enemy tick 1601, `experience: enmp 11 defeated: +10`, `gone ... defeated for good`, drop of row 43), the other enters `leave` after damage (tick 1345) and `hidden: despawned`, so the group clears; 0 `unimplemented`, 0 `script error`. (2) unattended soak (J every 9 s from 12 s only to choose Continue, 175 s): Sora is knocked out ten times and Continue chosen nine times, 20 instances, enemy ticks up to 7462 (124 s of enemy time), longest single mode 4 s (241 ticks), no mode stuck, 0 `unimplemented`, 0 `script error`. A first kill run (120 s) hit while angry: `atk_press` repeats with the facing vector fixed (0.36, 0, 0.93) and the instance jumps away from Sora (distance 190 to 1398 cm) until it would leave at 2000 cm: each jump ends and re-enters `atk_press`, so it is not stuck, but nothing in the script turns it toward Sora in that mode (see gaps). Test `m_ex710_is_an_enemy_kind_with_its_objentry_row` (disc) and `random_free_point_walks_the_shared_generator_and_answers_the_centre_without_a_floor`.
Screenshots looked at (tt39, `en14a` in the scratchpad): idle at 11 s (`M_EX710 Idle hp 50/50` twice, two large green spiked balls with yellow fists on Sora, who is already down, HP 0/20), hit at 14 s (`M_EX710 Idle hp 38/50` and `45/50`, damage taken, a lock-on ring on the front one, Sora hurt HP 1/20), hit with one enemy left at 28 s (`M_EX710 Idle hp 7/50` high on the stair landing, 5.0 away), after the kills at 34 s (no enemy text, a green and a yellow prize orb beside Sora, `drive +91%`).
Gaps: (1) 1.247 and 2.36 were never called in a run (seek_target is never entered; 2.36 sits in the event-15 handler under motion state 242), so the arms are covered by the unit test only. (2) The angry `atk_press` does not turn toward Sora in the app; whether the original turns it (the engine's own facing, host 1.121 target mode 0 = FUN_0018f640, which only sets the target record to the player) was not found: R0d24 has no FaceDirection and no caller of R0a65. (3) `M_EX710` was only run in tt39 b_10 (the other 27 areas and group b_20 with seven mixed members were not). (4) Attack objects are stubs as for the other kinds; the damage Sora took (atkp 165, 164, 940, reaction 4 Blow and 3 FlinchPush) was not compared with the original. (5) The object's `SpawnLimiter` byte was not read.

