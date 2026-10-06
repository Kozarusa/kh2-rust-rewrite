# Enemies: which ones the app has, and why

Work package 6C. The app's enemies are Shadow (`M_EX020`), Soldier (`M_EX010`) and, since 6C, the Dusk (`M_EX990`).
All three run under their own BDX scripts (`docs/enemy-ai.md`) and take hits through the reaction code
(`docs/hit-reaction.md`). Nothing from the disc is stored in the repository: area files, models, motions,
scripts and tables are read at start-up or when an area loads.

## Choosing the next enemy from the data

The areas the app loads are tt00, tt02 and tt04. Their area files (`ard/<area>.ard`) hold the area scripts
(BAR type 13: `map`, `btl`, `evt`) that load spawn groups (BAR type 12). `crates/kh2-ard` reads both
(layout in its header); its disc test lists every object the three scripts can spawn:

```
cargo test -p kh2-ard --test disc -- --nocapture
```

Objects with `00objentry` ObjectType 4 (the enemies; Shadow, Soldier and the Dusk have it), counted over every
spawn group every program of every script loads (a group loaded by two programs counts twice):

| object | model | count | where |
| --- | --- | --- | --- |
| 0x13e | `M_EX990` (Dusk) | 13 | tt04 `btl`: b_00 (program 0x1, 3), b_44 (0x50, 7), b_43 (0x4f, 1), b_21 (0x7, 2) |
| 0x678 | `N_EX570_BTL` | 4 | tt04 `btl`: b_41, b_42, b_45, b_51 (Struggle opponents) |
| 0x138 | `M_EX930` | 1 | tt04 `btl`: b_21 |
| 0x4d7 | `N_EX500_BTL` | 1 | tt04 `btl`: b_50 |
| 0x82f | `N_EX600_BTL` | 1 | tt04 `btl`: b_52 |

tt00 spawns only the player; tt02 spawns the player, the two friends, save and world points, `F_EX000` and event
actors (ObjectType 15). Neither Shadow nor Soldier is in any of the three scripts: the app's Shadow/Soldier waves
are its own (see "Waves"). `M_EX940` (object 0x139, four in b_44) has ObjectType 21, not 4.

The most common enemy is the Dusk. Its model, motion set (`obj/M_EX990.mset`, 46 motions), effect file
(`obj/M_EX990.apdx`) and BDX script (BAR type 3 of the model file, 6378 words) are on the disc; the test asserts that.

## The Dusk

- **Stats.** The script calls `set_obj_param` (host 2.9) with the constant 41 (BDX `M_EX990` at 0x073c), the
  `enmp` row the game builds the status block from (FUN_001b2148). `Kind::DUSK` uses row 41 with the app's
  world level 1 (as for Shadow and Soldier): 32 HP.
- **Model, motions, hit and attack shapes** come from `obj/M_EX990.mdlx`/`.mset` like the others. Attack windows
  are motion range triggers 10 and 33 (claw 201: atkp 16, kick 202: atkp 17, walk attack 203/204: atkp 602 and
  18); they need no new code.
- **Reactions** use the default slots (10/12 flinch, 16..19 blow and landing) through `enemy.rs` unchanged.
- **Death.** The `dead` mode starts element 2 of its own effect file (`spawn_attached(obj, 2, 1, 0)`), the same id
  as the Heartless, so `GameEffect::Death` plays the Dusk's own death effect; then fade and hide.
- **Spawn kind.** Host 1.110 reads byte +0x1c of the object's spawn record (FUN_0016b858). The Dusks of b_00 and b_21
  have 2, those of b_43 and part of b_44 have 3; the app passes the record's value (the app's own waves pass -1,
  no record). 2 and -1 take the same appear path in the script.

New host functions the Dusk needs are in the table of `docs/enemy-ai.md` (0.15, 1.62/1.64, 1.63, 1.66, 1.69, 1.74,
1.98, 1.150).

## Waves

`enemy::spawn_waves` keeps its timer (a wave 4 s after the last enemy is gone) and the F3 key. What a wave is:

- In an area whose `btl` script loads groups with enemies of `enemy::KINDS` (tt04), a wave is the next such
  group in script order: b_00, b_44, b_43, b_21. Each enemy stands where its record puts it (game space converted:
  `(x, -y, -z)`, then put on the floor under it) with the record's facing (game yaw `r` is the app's `PI - r`) and
  spawn kind. Members the app does not have are logged and left out (none left in tt04 since WP-7D).
- Elsewhere (tt00, tt02) and with `KH2_AREA_WAVES=0`: the app's own wave of Shadows around Sora with a Soldier from
  the second wave on, as before. These are not from the disc.
- `KH2_WAVE_GROUP=<tag>` (harness) makes every wave that group; when the group holds the player's entity (0x236)
  Sora is put there first (b_44: the centre of the Sandlot). With `KH2_WAVE_AT_GROUP=1` a forced group without a
  player entry (b_00, b_21) puts Sora at the centre of its members instead (a test aid, WP-7D; not the game's rule).

**Gaps.** Which `btl` program the game runs, and when, depends on story progress and the event scripts, which the
app does not have: the groups take turns instead. Whether the game moves the player to a battle group's player
entry when the battle starts was not read (inferred from the group listing the player; only the harness does it).
Sora's own start point in tt04 is the app's (`assets::load_area`), about 22 m from b_00, outside the Dusks' battle
radius (2000 cm, set by the script at +0x38): a b_00 wave appears, finds Sora too far, and leaves (mode `leave`)
unless Sora walks into the Sandlot.

## Acceptance run (6C)

`KH2_BDXLOG=1 KH2_FXLOG=1 KH2_WAVE_GROUP=b_44 KH2_KEYS="6:Q,<J every 0.45 s from 6.3 to 44 s, Q every ~7 s>"
KH2_SCREENSHOT=... KH2_SHOT_AT=45 kh2-game tt04`: seven Dusks appear around Sora. Modes entered: `appear`,
`mode_battle`, `btl_attack`, `btl_short`, `btl_long`, `idle_time`, `footwork`, `move_target`, `move_step`,
`move_front`, `move_down`, `escape`, `jump`, `atk_craw`, `atk_kick`, `atk_walk`, `mode_revenge`, `rvg_short`,
`rvg_long`, `leave`, `dead`. No script error. Four Dusks died with `death effect: element id 2 of
Some("obj/M_EX990.apdx")`, two left (mode `leave`, about 15 m from Sora), one still fought at the end; the only missing host function reached was 1.92 (the drop, as for the
Heartless). Screenshots were looked at: Dusks around Sora in the Sandlot, posed mid-motion (not a rest pose), one
locked on with its HP bar. Nobody listened to the sounds.

## Follow-ups (6G)

The Dusk's remaining host calls are wired (details in docs/enemy-ai.md): its reaction commands 24 and 69 send
`dusk_surprise` (host 1.90) and start a request thread that plays its paired motion 241 / 242 on Sora (now read: the
paired-motion and collision-table lists iterate `enemy::KINDS`), which also reaches 0.42, 0.43 and 1.94; its death's
drop (1.92) is computed from `przt` (row 6: four prizes of kind 5, no item in the runs) and logged. Runs (`KH2_BDXLOG=1
KH2_HITLOG=1 KH2_REACTIONLOG=1 KH2_WAVE_GROUP=b_44 ... kh2-game tt04`, Sora attacking from 4.3 s): with
`KH2_REACTION_FAKE=24 KH2_REACTION_FAKE_ON=Dusk KH2_REACTION_PRESS=10..40`, 31 accepts, every Dusk got the message and
the near ones entered `dusk_surprise`; with `KH2_CLASH_FORCE_ENEMY=8`, four Dusks died with a drop line each and the
forced clash reached event 19 (handler absent). No `unimplemented host call` and no script error in either run. Only
logs were read: no screenshot was looked at for these runs.

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
