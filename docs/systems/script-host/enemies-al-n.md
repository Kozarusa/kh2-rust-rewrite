# Enemies: M_AL020, N_EX and RAW variants

[Contract](contract.md) | [Index](../../README.md)

## The second enemy: N_EX570_BTL (WP-6H)

`N_EX570_BTL` (object 0x678, ObjectType 4; Seifer by the community's model names, `enemy::SEIFER`) is the next
most spawned enemy (4 groups). It is a combat opponent, not an NPC: its script registers attack modes
(`atk_swing_down`, `atk_cut_off`, `atk_cut_off_short`, `atk_jump`). Model, motion set (206 slots, 31 with motions),
effect file and BDX script (6619 words) are on the disc (`cargo test -p kh2-ard --test disc second`).

- **Spawn parameters.** FUN_0016b870 copies the record words at +0x20 and +0x24 to object +0x820/+0x824, which
  host 1.39 (FUN_0016b940) reads as spawn parameters 1 and 2. Parameter 1 per group: b_41 1, b_42 2, b_45 4, b_51 5.
  The script branches on it: 1 never attacks (`btl_attack` R13ab: walk, idle, footwork, backstep only), 3 or 4
  take `enmp` row 155 (R1864), others row 135; 5 (b_51) sets the lowest HP to 1 (host 1.231) and sends
  `player_finish_struggle` / goes to `faint` when hit at HP 2 or less (event 6 handler at 0x84).
- **Stats.** Kind row 135 (60 HP at world level 1); host 2.9 rebuilds the status block (FUN_001b2148) when the script
  asks for another row (b_45: row 155, 100 HP).
- **HP record.** The status block holds 32 records (current, maximum, lowest) (FUN_0019f770); every HP change
  goes through FUN_001a0100, which holds the result between lowest and maximum. `Enemy::min_hp` is that lowest.
- **Death.** Its `dead` mode (0x174d) asks for voice line 37, plays motion 16 (or 10), stops its dash and waits for
  ever: no death effect and no fade (`Kind::death_effect` false), so the body stays.
- **Gaps.** The Struggle rules (orbs, timer, the win/lose event 10 with 159/160, `player_finish_struggle`) are not in
  the app; messages (1.90) stay missing. Voice lines (1.262 -> FUN_001970b8 -> FUN_001ae798, lines 17, 26, 37)
  are stubbed: no `voice/us/battle` bank names this speaker and the app has no enemy voice slot. Its motions' voice
  triggers (13) are only played for Sora by `audio.rs`. Script motions 48..50 (R0b83, which also calls host 4.0, an
  empty table entry) are not in the set; that path was not reached.
- **Next enemies.** `M_EX930` (b_21), `N_EX500_BTL` (b_50), `N_EX600_BTL` (b_52): WP-7D, below.

Acceptance runs (WP-6H, `KH2_BDXLOG=1 KH2_WAVE_GROUP=<g>`, Sora idle until 25 s then J every 0.45 s, Q lock):
b_42 modes `start_voice`, `mode_battle`, `check_arg_mode`, `btl_long`, `btl_attack`, `footwork`, `walk_forward`,
`run_forward`, `atk_swing_down`, `atk_cut_off`, `atk_jump`, `backstep`, `mode_revenge`, `dead`; b_45 also `btl_short`,
stats rebuilt to row 155. No script error, no unimplemented host call. Its claw 1042 flinched Sora (push 200).

## The fourth enemy: N_EX500_BTL (WP-7D)

`N_EX500_BTL` (object 0x4d7, ObjectType 4, `enemy::N_EX500`; the app calls it by its model, its name is not in the
data it reads) is the Struggle opponent of b_50 (btl program 0xb6, with a player entry: Sora is put there). Model
(4 meshes, 3200 triangles, 166 bones, 3 textures), motion set (206 slots, 29 with motions), effect file and BDX
script (6638 words) are on the disc (`cargo test -p kh2-bdx --test disc n_ex500`). Its family is N_EX570_BTL's (the
same mode names: `start_voice`, `check_mode`, `reflect_struggle`, `player_finish_struggle`, `gameset_voice_*`).

- **Stats.** Init hands `enmp` row 150 to `set_obj_param` (0x0736): 200 HP at world level 1. It reads no spawn
  parameter (no host 1.39 call), and sets the lowest HP of part 0 to 1 right after (1.231 at 0x0742): it cannot
  die in a fight. Distance thresholds 300 / 400 cm (R187e, context +0x30 / +0x34).
- **Damage (event 6).** A real hit whose effective reaction (host 2.7, the hit record's byte +0x26) is 4, 7, 9 or 11
  sends `player_finish_struggle` and then goes to `faint` when its HP is 2 or less, else drops prize row 122 (1.92:
  ten prizes of kind 0 and five of kind 1 in the runs: the Struggle orbs [inferred]); other real hits switch to
  `mode_revenge`. The app now hands the effective reaction with every hit (`AiHit::reaction`).
- **Struggle score.** Host 4.29 (FUN_001d6738) reads the score of side 0 or 1 from the manager at DAT_01d5dd20,
  which an event command creates (FUN_00180b48 -> FUN_001b9338); without it the function answers 0, as it does in
  the app (the Struggle rules are WP-7E's). The script compares 70 with side 1's value and picks
  `gameset_voice_win` / `_lose` / `_draw` from the two.
- **Host table.** 4.29 is not in `kh2_bdx::names`; the app now gives every enemy runtime the executable's host table
  (`HostTable::from_exe`, as the disc and oracle tests do), so such calls reach the host as `Call::Unknown` instead
  of stopping the script with `UnknownHost`.
- **Death.** Its `dead` mode (R0900) is the Dusk's (drop, element 2, flags 3 and 6, fade, hide), so
  `Kind::death_effect` is true; with the lowest HP at 1 the app never reaches it.
- **Not reached.** Event 15 (its handler compares host 2.25, the hit's attack id, with 1046..1048) and event 13
  are not delivered by the app; 4.0 / 4.1 (empty table entries) sit on the 1.32 task path, which was not reached.

Acceptance run (WP-7D): `KH2_BDXLOG=1 KH2_WAVE_GROUP=b_50 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_KEYS="6:Q,<J every
0.45 s from 6.3 to 36 s>" kh2-game tt04`: modes `start_voice`, `mode_battle`, `check_mode`, `run_origin`,
`idle_short`, `idle_fix`, `move_target_near`, `atk_blow_2`, `atk_spin`, `back_step`, `mode_revenge`; no script
error, no unimplemented host call; Sora's hits flinch and blow it (HP 200 down to 176 in the first eight hits), its
blow-class hits send `player_finish_struggle` and drop prize row 122.

## The fifth enemy: N_EX600_BTL (WP-7D)

`N_EX600_BTL` (object 0x82f, ObjectType 4, `enemy::N_EX600`, called by its model) is the Struggle opponent of b_52
(btl program 0xb7, with a player entry). Model (14 meshes, 3135 triangles, 262 bones, 5 textures), motion set
(208 slots, 34 with motions), effect file and BDX script (7087 words) are on the disc (`cargo test -p kh2-bdx --test
disc n_ex600`). Same family as N_EX500_BTL: no spawn parameter, lowest HP 1 (1.231 at 0x0809), 4.29 for the score,
the same event 6 rule (effective reaction 4, 7, 9 or 11: `player_finish_struggle`, then `faint` at HP 2 or less,
else prize row 132), plus two branches of that handler that switch to `atk_air_counter`.

- **Stats.** `enmp` row 158 (0x07fd): 100 HP at world level 1. Distance thresholds 350 / 450 cm (R1a39).
- **Host 1.95** (0x173038 -> FUN_00183470), new: mover 1 continuing the object's last real movement (+0x860, host
  1.62): its horizontal part as the velocity, its vertical speed vy giving the apex time vy / g and the apex height
  (g = DAT_0037a884 = 20/49 per tick squared, the factor 0.5 at DAT_0037a888). Called by the second routine of
  `atk_air_counter` (R13e7), which then plays motion 4 when motion 202 was playing. The app starts its `Motor` jump to
  the same apex (rising) or a fall already that fast (falling) with the horizontal speed kept; approx: the eased
  rise of `Motor` is not the game's parabola. 1.95 is not in `kh2_bdx::names`: it arrives undecoded and is taken as
  the object's own (its only caller passes its own context).
- **Death.** `dead` (R09c8) is the Dusk's: `Kind::death_effect` true, not reached with the lowest HP at 1.

Acceptance run (WP-7D): `KH2_BDXLOG=1 KH2_WAVE_GROUP=b_52 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_KEYS="6:Q,<J every
0.45 s from 6.3 to 36 s>" kh2-game tt04`: modes `start_voice`, `mode_battle`, `check_mode`, `btl_attack`,
`btl_long`, `footwork`, `footwork_not_voice`, `move_target_near`, `atk_swing_side`, `atk_combo_thrust`, `atk_guard`,
`atk_air_counter`, `land_check`, `back_step`, `mode_revenge`; no script error, no unimplemented host call (1.95
reached twice, `first host call 1.95 ? [Approx]`); its blow 168 knocked Sora down (reaction 4).

Not done for the three (WP-7D): the Struggle rules (WP-7E), enemy voice lines (1.262 stays a stub), event 13 and
event 15. `docs/systems/combat-effects/hit-reaction.md`: all three use the default reaction slots of their sets.

## The enemy M_AL020_FIRE (WP-EN11e)

`M_AL020_FIRE` (object 76, `00objentry` row 76: ObjectType 4, `NeoStatus` 1030, `SpawnLimiter` 6, no spawn objects; `enemy::M_AL020_FIRE`, called by its model; the task named it exactly so, and it is an objentry model) is a fire Heartless of Agrabah (a dark body with a flame crest). The survey lists it absent in al00 and al01; al00 `btl` program 0xa loads b_20 with eighteen of it and program 2 loads b_01 with many more (test `m_al020_fire_is_an_enemy_kind_with_its_objentry_row`, which checks the row and the b_20 count). The rows 1670 (`M_AL020_FIRE_RAW`) and 1673 (`M_AL020_FIRE2`) share its `NeoStatus` and were not run.
Its BDX script (4137 words, `cargo run -p kh2 -- bdx M_AL020_FIRE`) registers modes `appear`, `mode_battle`, `mode_revenge`, `mode_battle_boss`, `mode_revenge_boss`, `leave`, `dead`, `freeze`, `event`, `idle`, `idle_time`, `footwork`, `revenge`, `wait`, `move_target_near`, `jump_target`, `jump_target_back`, `jump_target_side`, `highjump`, `atk_body_hit` and the `btl_*` / `rvg_*` / `rst_*` pickers.
Stats: the constructor (called at 0x046d) takes eight values after the object (pushed 0x0455..0x046a: 5, 65, 100.0, 250.0, 8.0, 4, 0.7, 0.35); R0e42 (0x0e42) stores the first, 5, to local+0x20 and hands it to host 2.9 at 0x0e69: `enmp` row 5 (10 HP at the app's level 1). The second, 65, is the drop row of host 1.92 (log `drop (host 1.92) asked row 65`).

Host calls: no new arm for the script's calls (no `unimplemented host call`, no `script error`). One kind-specific arm was appended to host 1.60 (`busy`): the jump modes (R0c2d at 0x0c2d: `start_move`, wait for take-off, then R0731 at 0x0731 waits while `busy` or in hit stun) ended their wait at once, because the app reads `busy` as a hit reaction only, so the script played motion 202 and called 1.74 mover_reset in the same tick, which cancels the jump (the reviewer's check of EN11f; the objects climbing to the ceiling (13 m, d 13.1) seen in the first EN11e screenshot was a different symptom). For tag `M_AL020_FIRE#` `busy` now also holds while the object is airborne [inferred; the executable's FUN_001729f8 flags at +0x160 were not decoded for jumps]. After that the group jumps, lands and attacks (`atk_body_hit`).
Stubs in its run: 1.121, 1.171, 1.206, 65535.0; Approx: 1.11, 1.12, 1.23, 1.55, 1.56, 1.60, 1.120, 2.5. Host 1.13 is not in the script's calls seen in play (`queues_motions` is unchanged).

Harness recipe (group 0x51 stands in b_01, which has a player entry only for other groups; kind-2 volumes are boxes in game centimetres, app x = game x / 100, app z = -game z / 100): `KH2_PROGRAMS=0,2,0 KH2_WAVE_GROUP=b_01 KH2_WAVE_AT_GROUP=-19.14,1.51 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_SCREENSHOT=<png> KH2_SHOT_AT=<s> KH2_KEYS=8:Q,<J every 0.45 s from 9> kh2-game al00`.
The harness reads `KH2_KEYS` only when `KH2_SCREENSHOT` is set, and the screenshot ends the run. Kind-2 group 0x51 (volume centre (-19.1, -4.5 m, 1.5), half extents 9.3 x 4.7 x 5.1 m) creates three `M_AL020_FIRE` (serials 0x3b9 .. 0x3bb, wave 0) near Sora; 0x4e (four, wave 1) and 0x4f (wave 3) create more; 0x4b holds thirteen but its volume is 5.8 m up (Sora is placed at ground by the hook, so it is not entered). 0x4e at (-17.5, 13.94) put the group on the overhang (y 13 m) before the busy arm; the roof counts as ground in the app's probe.

Acceptance runs (WP-EN11e): (1) idle 118 s of enemy time (7100 ticks), no keys: modes `appear`, `mode_battle` / `btl_long` / `btl_short` / `btl_attack`, `jump_target`, `jump_target_back`, `jump_target_side`, `highjump`, `idle_time`, `footwork`, `move_target_near`, `atk_body_hit`; no stuck mode (mode counts over the run: `mode_battle` 235, `idle_time` 164, `jump_target` 129, `btl_long` 166, `btl_attack` 45, `footwork` 40, `highjump` 41); Sora, left alone, was worn down by the body hits (20 HP, `sora hit: atkp 443`). An earlier 15 s run before the busy arm logged a mode change on every tick (`mode_battle`, `btl_short`, `jump_target_side` each tick, objects at 13 m).
(2) hits (Q at 8 s, J every 0.45 s from 9 s): `M_AL020_FIRE hit: atkp 127 damage 2 hp 8`, `atkp 126 damage 3 hp 5`, `atkp 129 damage 6 hp 0`, `experience: enmp 5 defeated: +1 (lvpm 4 x enmp 20)`, `hp 0: switching to mode dead`, `drop (host 1.92) asked row 65, used row 65 ... 2 prize objects`, `prize: drop of row 65`. A run to 88 s killed all three of the group (serials 3, 2 and 1 at ticks 630, 1008, 2644) and the Shadows and `M_EX060` of other groups on the way.
(3) Screenshots looked at: idle at 9 s (three dark armoured spiked balls with a red flame crest and a heart emblem, two touching Sora; debug text `M_AL020_FIRE Idle hp 10/10`, Sora 17/20), hit at 12.5 s (damage number 3, `Hurt hp 7/10`, a prize gem on the ground), death at 10.9 s (`Hurt hp 0/10`, a dark dissolving figure, an HP heart and munny gems).

Gaps: (1) The `busy` reading for jumps is inferred; the real flag is object +0x160 inside FUN_001729f8 and was not traced for the jump motion. (2) The attack name `atk_body_hit` is the script's; its damage (`atkp 443`, 3 HP a hit) was not compared with the original. (3) Group 0x4b (thirteen, on a ledge 5.8 m up), 0x4f (wave 3) and the b_20 groups were not entered. (4) al01 and the other areas that hold it were not run. (5) `M_AL020_FIRE_RAW` / `M_AL020_FIRE2` (objentry rows 1670, 1673) are separate kinds and are not in the app.

## The enemy M_EX020_RAW (WP-EN11d)

The brief's name `M_EX020_RAW` is an `00objentry` model name (row 1365: ObjectType 4, flags 4, `NeoStatus` 1001, Weight 100, `SpawnLimiter` 4, `AnimationName` `M_EX020_RAW.mset`) and the disc has `obj/M_EX020_RAW.mdlx`, `.apdx` and `.mset`, so no other name is needed. It is the Shadow's twin: the model and effect files are the same size as the Shadow's
(`M_EX020`, row 302: `NeoStatus` 1001, `SpawnLimiter` 8) and `cargo run -p kh2 -- bdx M_EX020_RAW` prints the same 5333-word script as `bdx M_EX020` (identical listing), so its `set_obj_param` (host 2.9) row is the Shadow's `enmp` 34 and the kind is `enemy::M_EX020_RAW` (`enmp` 34, `status` and `death_effect` true), called by its model name.
What differs is the motion set: `obj/M_EX020_RAW.mset` is 4.1 MB, the Shadow's 365 KB (the rows 1841 `M_EX020_NM_RAW` and 1842 `M_EX020_WI_RAW` use it too; those models are not in the area survey's list). That the extra size is more or other motions was not looked into; the script's slot numbers all resolve (no `motion` problem in the log).
The area survey lists it in bb01, dc02, eh02, hb16, lk03 and mu05; `area_groups` finds it in bb01 `btl` program 0x44 b_40 (16 entities, nothing else), dc02 programs 1, 4, 26 (b_00 67, b_10 80, b_98, b_99), eh02 programs 25, 26 (b_10 75, b_01 6 of 22), hb16 programs 63 to 65 (b_41 36 of 48, b_42, b_43), lk03 program 25 b_99 (2 of 5, the rest `M_EX780` / `M_EX720`, kinds the app lacks) and mu05 program 72 b_40 (38).

- **Host calls.** The script runs the Shadow's host calls (docs/systems/script-host/contract.md). The only change: host 1.134 `obj_method_48` was Missing; an on-demand member (spawn kind 3, created by the kind-2 group) calls it after a random wait following 1.133 (script address 0x0a36). The vtable entry +0x48 of 0x35eea8 is FUN_0032cb20 (read from the executable): it clears bit 8 of object +0x10c and calls FUN_00193048, which sets or clears bit 9 of +0x10c and bits of +0x768 by the byte at +0x9ac. The app has no reader of either field, so it is a Stub (docs/systems/script-host/contract.md). The first run logged `unimplemented host call 1.134`; none after.
- **Lifecycle.** Same as the Shadow: `appear_shadow` (the kind-2 group's delay shows as a long first mode of 130 to 265 ticks), `mode_battle`, `battle_idle`, `move_shadow_round`, `move_target`, `btl_attack`, `btl_short`, `btl_long`, `atk_craw_f`, `atk_craw_r`, `atk_craw_l`, `mode_revenge`, `rvg_short`, `rvg_long`, `dead` and `leave` (FadeOut, `Hide`, `hidden: despawned` 30 ticks after).

Acceptance runs (WP-EN11d), bb01 b_40 (`KH2_PROGRAMS=0,44,0 KH2_WAVE_GROUP=b_40 KH2_WAVE_AT_GROUP=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1`, `KH2_KEYS=9:Q,` then J every 0.4 s from 10 s, party on, `kh2-game.exe bb01` through `local/capped.ps1 -CapMB 2500`; the group puts Sora in the group's centre, among the Shadows): (1) 0 `unimplemented host call`,
0 `script error` in every run. (2) Soak: 11886 enemy ticks (198 s) in one run with the party on: 49 kills (`hp 0: switching to mode dead`, `experience: enmp 34 defeated: +1`, `drop (host 1.92)` 49 times each), 50 `hidden: despawned`, one `leave`; the longest mode of a living instance was the first `appear_shadow` (264 ticks); the kind-2 group made new ones after each loss (group header limit 100) (every dead one was replaced until that limit; clearing the whole group was not reached in a run). A run with Sora alone (no party)
was knocked out at 19 s (game over, J = Continue); a run with the party on and 8 game overs (Continue by J) went 4302 ticks without a stuck mode. (3) PNGs looked at (`scratchpad\idle.png`, `hit2.png`, `death7.3.png`, not committed): idle (20 s: debug text `M_EX020_RAW Idle hp 10/10` sixteen times, black Shadow-like Heartless crouching round Sora on a staircase before double doors, bb01's purple-papered hall, HUD present); a hit (22 s, party on: damage numbers "1" over a Shadow, `Idle hp 5/10`,
Sora with the keyblade raised, Goofy and Donald icons on the right); a death (`death7.3.png`, 7.3 s: debug line `Blown hp 0/10`, white damage numbers "4" from Donald's attack, a green orb and a "1p" prize number, Sora and Donald in a pale burst). Regression: tt00 combat (Shadows hit and hurt, HUD), tt04 Dusk group b_44 (a silver Dusk beside Sora, Goofy behind, hit lines 42) and tt00 idle (`KH2_NO_ENEMIES=1`, HUD present) all ran with 0 `unimplemented` and 0 `script error`.

Gaps: (1) Only bb01 b_40 was run; dc02, eh02, hb16, lk03 and mu05 were not entered, and the groups that mix the kind with others (eh02 b_01, hb16, lk03 b_99) were not seen. (2) What the RAW motion set adds over the Shadow's (4.1 MB against 365 KB) was not inspected; the debug text and the log show the same modes and the same motion slots. (3) Host 1.134's method is an inference (FUN_0032cb20 is the vtable entry, that it is the method reached is [inferred]); no fight difference follows from it in the app.

WP-FIX2: M_EX420 (mode at 0x0e20) calls host 1.2 `move_toward` (0x170228, FUN_00182538, see docs/systems/script-host/contract.md). It was unnamed, so every ca10 run logged `unimplemented host call 1.2`. Recipe: ca10, `KH2_PROGRAMS=0,a,0 KH2_WAVE_GROUP=b_20 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1`, J presses 13..92 s, shot at 95 s, through `local/capped.ps1 -CapMB 2500`: 0 `unimplemented`, 0 `script error`, `bdx M_EX420#2 t114: first host call 1.2 move_toward [Approx]`, `1.2 move_toward Approx x112` per 15 s, `M_EX510 hit ... hp 0`, `experience: enmp 55 defeated: +2`. The 95 s run went through the whole enemy time without a stuck mode; the PNG shows M_EX510 (blue horned Heartless) beside Sora in the treasure cave, Sora HP 4/20.

## The enemy M_AL020_ICEE (WP-EN11f)
`M_AL020_ICEE` (object 77, `00objentry` row 77: ObjectType 4, `NeoStatus` 1031, `SpawnLimiter` 7, no spawn objects; `enemy::M_AL020_ICEE`) is the ice twin of `M_AL020_FIRE`: a Heartless inside an ice block. Chosen because the survey's top kinds are all in `KINDS` (the ranking table predates EN9 to EN11e), the remaining absent ones are bosses (`B_CA020`, `B_CA040`) or appear in few areas, and `M_AL020_ICEE` fills al01 `btl` program 2 b_01 (thirteen of its 21 members, EN10n) and is a sibling of the kind EN11e just proved. `M_EX590` (5 survey mentions) and `M_EX580` were not examined.
Its BDX script (5129 words, `cargo run -p kh2 -- bdx M_AL020_ICEE`) hands `enmp` row 6 to `set_obj_param` (R1224 at 0x1224, called at 0x046d with 6 as the first of the eight values pushed 0x0455..0x046a; host 2.9 at 0x124b), so the experience row is enmp 6 (`experience: enmp 6 defeated: +1 (lvpm 4 x enmp 20)`).
Host calls: one new arm, host 1.100 (0x173178 -> FUN_0017bee0): `sqrt(2 x / g)` of a float word with g = 20/49 (`DAT_0037a538`, as in docs/systems/player-camera/contract.md and the summons' arm), Impl; the script asked it (`unimplemented host call 1.100`) in a jump mode. The 1.60 `busy` arm of EN11e now covers `M_AL020_ICEE#` too, and it is needed: without it the wait after `start_move` ends in the same tick, the script plays motion 202 and calls 1.74 `mover_reset`, which cancels the jump, so the jump motions in slots 3 and 4 never play (checked by the reviewer by removing the arm).
Acceptance run (WP-EN11f), al01 `KH2_PROGRAMS=0,2,0 KH2_WAVE_GROUP=b_01 KH2_WAVE_AT_GROUP=-9.17,17.04 KH2_NO_PARTY=1` (the centre of kind-2 group 0x33 from `kh2 coct-walls al01 b_01`), `KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 kh2-game al01` through `local/capped.ps1 -CapMB 2500`; test `m_al020_icee_is_an_enemy_kind_with_its_objentry_row` (disc). Soak 100 s with J pressed 10..80 s: 0 `unimplemented`, 0 `script error`; 7 ICEE `dead`, each paying `experience: enmp 6 defeated`; the modes seen are appear, mode_battle, mode_revenge, move_target_near, idle_time, idle, footwork, atk_slip, btl_*/rvg_*, dead; the one survivor sat in idle_time (not hit), no mode stuck. Screenshots looked at: idle/hit (a Heartless in a translucent blue ice cube beside Sora with stars and HP 8/10 in the debug text), mid-fight with orbs and a prize gem, and a hit with Sora's blade through the cube.
Gaps: (1) Only al01 b_01 was run; the other Agrabah areas that load it were not. (2) `M_AL020_ICEE_RAW` (a separate objentry row in al00 and al01 lists) was not examined. (3) Whether ice-block cosmetics (the block breaking) follow from the model's own motions was not compared with the original. (4) Its attack objects are stubs as for the other kinds (2.10 family).

## The enemies M_AL020_FIRE_RAW and M_AL020_ICEE_RAW (WP-EN16a)
Chosen from docs/systems/script-host/area-survey.md: the kinds with no missing host call (the survey's "host calls the enemy host lacks" column is 1 and 0), a script identical to a kind the app already runs (`cargo run -p kh2 -- bdx` lists the same 4137 words for `M_AL020_FIRE` and `M_AL020_FIRE_RAW`, the same 5129 for `M_AL020_ICEE` and `M_AL020_ICEE_RAW`; only the header line differs) and 47 members between them in al10 and he09. The `M_EX010_WI`, `_NM` and `_TR` Soldier twins were not taken: the world variants need their areas' scripts and models and were not looked at.
`00objentry` row 1670 `M_AL020_FIRE_RAW` and row 1671 `M_AL020_ICEE_RAW`: ObjectType 4, `NeoStatus` 1030 and 1031 (those of rows 76 and 77), `SpawnLimiter` 4, `AnimationName` `M_AL020_FIRE_RAW.mset` and `M_AL020_ICEE_RAW.mset` (files of their own, so no `mset_name` redirect as for `M_EX610_RAW`). Both hand `enmp` rows 5 and 6 to host 2.9 as their bases do (BDX R0e42 and R1224), so `enemy::M_AL020_FIRE_RAW` (enmp 5) and `enemy::M_AL020_ICEE_RAW` (enmp 6) are in `KINDS` (54). Groups: he09 `btl` program 0x25 b_9d (two `M_EX620`, three of each kind, one kind-2 group 0x19f whose volume is at the origin, `kh2 coct-walls he09 b_9d`) and program 0x2d b_9l (one `M_AL020_FIRE_RAW` among others); al10 program 0x3a b_40 (14 `M_AL020_ICEE_RAW`, 26 `M_AL020_FIRE_RAW`, 7 `M_EX110`; kind-2 volumes at game (-246, 687) and (25, 742)). Tests `m_al020_fire_raw_is_an_enemy_kind_with_its_objentry_row` and `m_al020_icee_raw_is_an_enemy_kind_with_its_objentry_row` (disc).
Host calls: no new arm. The one kind-specific arm, host 1.60 `busy` while airborne (EN11e, EN11f), now also matches the tags `M_AL020_FIRE_RAW#` and `M_AL020_ICEE_RAW#` (the jump modes need it). 0 `unimplemented host call`, 0 `script error` in every run below.
Runs (all through `local/capped.ps1 -CapMB 2500`, `KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1`): (1) `KH2_PROGRAMS=0,25,0 KH2_WAVE_GROUP=b_9d KH2_WAVE_AT_GROUP=0,0 kh2-game he09`, Q at 8 s and J every 0.45 s from 9 s, to 125 s (7620 script ticks in the last run): modes appear, mode_battle, mode_revenge, btl_*, rvg_*, idle, idle_time, footwork, jump_target, jump_target_back, jump_target_side, highjump, move_target_near, atk_body_hit, atk_slip, dead; three `M_AL020_FIRE_RAW` reached `hp 0: switching to mode dead`, each with `experience: enmp 5 defeated` and `drop (host 1.92) asked row 65` (the three `M_AL020_ICEE_RAW` of b_9d had left before they were hit). (2) `KH2_PROGRAMS=0,3a,0 KH2_WAVE_GROUP=b_40 KH2_WAVE_AT_GROUP=-2.46,-6.87 kh2-game al10`, the same keys, to 125 s: 4 deaths (3 `M_AL020_ICEE_RAW`, each with `experience: enmp 6 defeated` and a drop; 1 `M_AL020_FIRE_RAW`), `ICEE_RAW` modes as above plus `leave` (11 times) and `FIRE_RAW` `leave`; one game over was continued with J. Two earlier runs of this scenario left one `M_AL020_FIRE_RAW` in `jump_target` / `highjump` until the end (review of EN16a): the object had jumped off the walkable ground, had no floor to land on (status line `distance 33087 cm hp 4 grounded false` at tick 7620, 78 s after its jump) and so stayed "busy" in the airborne arm of host 1.60 for ever. Fix in `enemy_ai.rs`: an airborne enemy more than 30 m below Sora is removed like a leaver (log `fell out of the world: removed`, `FALL_OUT_UNITS`; the rule of the original was not found [inferred]). Two re-soaks of 125 s each (7500 ticks): every one of the 40 members of b_40 ended dead or hidden (`hidden: despawned` 40 times in both runs), 0 `unimplemented`, 0 `script error`; the fall-out line fired once in the second run and not in the first.
`leave`: the script's R0864 / R0876 switch `mode_battle` to `leave` (motion 45, effect, flag 3, `FadeOut`) when the host flags 1.206 or 1.171 allow it and the object is farther from its target than the radius at +0x38 (off screen) or +0x3c (on screen) of its state (R0876 at 0x0876, R08bf at 0x08bf); the app hides it ("hidden: despawned") as for the other kinds. A group member that left no longer fights, so a group is cleared by the deaths plus the leavers; Sora, standing still at the group's centre, was not left by the base kinds (b_93 with `M_AL020_ICEE` ran 25 s with no `leave`) and was left by the RAW ones only after he had run off to fight others [inferred from the distances in the HUD debug text: 9.6 to 10.7 m at the first `leave`].
Screenshots looked at (scratchpad, not committed): he09 idle at 12 s (two pink hooded `M_EX620`, three ice cubes each holding a blue Heartless and three dark spiked fire balls with a red heart emblem, debug text `M_AL020_ICEE_RAW Idle hp 10/10`, `M_AL020_FIRE_RAW Idle hp 10/10`), hit at 13 s (damage numbers 2, `M_AL020_FIRE_RAW Hurt hp 8/10`, `Hurt hp 0/10`, a prize gem and an orb on the floor), al10 at 20 s (a dozen ice cubes in a sand-coloured cave, `M_AL020_ICEE_RAW Idle hp 8/10` and `5/10`, Sora in a hit stagger with a damage ring). A death frame itself was not captured: the figure was already gone in the one frame taken after `hp 0`.
EN15c gap closed: `enemy_ai.rs` `load_programs` and `player_reaction.rs` read the paired motions from the motion set the objentry row names (`enemy::mset_name`, now `pub(crate)`), so `M_EX610_RAW` no longer logs `obj/M_EX610_RAW.mset: not in archive` (0 such lines in the runs above). `M_EX590` is in the oracle's script list (`crates/kh2-bdx/tests/oracle.rs`); host 1.41 is named `raise_event` (`names.rs`, docs/systems/script-host/contract.md): FUN_001727b0 forwards (event, argument) to FUN_00197b70.
Gaps: (1) A mode-cycle test for `M_EX590` was not added: the mock host of `crates/kh2-bdx/tests/disc.rs` does not know host 1.320, which its script calls (`UnknownHost { group: 1, index: 320 }` at step 3816), so a dry run errors. (2) The motion sets of the RAW kinds were not compared with the base kinds'; their paired player motions, if any, were not listed. (3) al10 b_00 and b_20, he09 b_9l and the world-map entries were not run; `M_AL020_FIRE2` / `M_AL020_ICEE2` (rows 1673, 1674) are separate kinds not in the app. (4) The leave radius values were not read from the constructor (the fields at +0x38 and +0x3c).
Accept scenario proposed: `@{ Name = 'he09-m_al020_raw'; Area = 'he09'; Env = @{ KH2_PROGRAMS = '0,25,0'; KH2_WAVE_GROUP = 'b_9d'; KH2_WAVE_AT_GROUP = '0,0'; KH2_NO_PARTY = '1'; KH2_AREALOG = '1'; KH2_BDXLOG = '1'; KH2_HITLOG = '1' }; Keys = '8:Q,9:J,9.45:J,9.9:J,10.35:J,10.8:J,11.25:J,11.7:J,12.15:J'; ShotAt = 13; Expect = @('M_AL020_ICEE_RAW#', 'M_AL020_FIRE_RAW#', 'mode appear'); Look = 'ice cubes and dark fire balls near Sora, damage numbers' }`.
