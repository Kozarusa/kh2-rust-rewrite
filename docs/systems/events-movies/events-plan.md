# Events: plan, unknowns, index

[Contract](contract.md) | [Index](../../README.md)

## 8. Where it goes in the app

| file | function / item | change |
|---|---|---|
| `kh2-ard` (done) | `Area`, `Group`, `parse_trigger`, `parse_event`, `progress` | readers used below |
| new `area.rs` (kh2-app) | `AreaRecord` resource (world, area, entrance, programs) and `AreaPrograms` (the 0xc0-byte table per world from the save block) | E1, E5 |
| new `area_script.rs` | port of `FUN_00181148` / `FUN_00180b48`: run `map`, `btl`, `evt` programs, spawn groups, record triggers, music, map visibility | E1 |
| `assets.rs` | `load_area`: take the player placement from spawn kind 1 entities of the entrance; load by the area record | E1 |
| `enemy.rs` | `area_groups` / waves: read the btl program from the area record instead of taking turns | E5; done in EN1 (docs/enemies.md "Waves") |
| new `area_jump.rs` | exit volumes (`Activator` shape tests), the jump task with fade kinds, triggers and `FUN_00197b70` events | E1 |
| new `npc.rs` (done) | class-5 objects with their BDX and the group 5 hosts (paths); `KH2_PROGRAMS`, `KH2_NPC_WATCH`, `KH2_NPCLOG`, `KH2_NO_NPC` | E2 |
| `enemy_host.rs` | hosts 0.14 / 0.15 on `Progress` story flags; 1.30 / 1.31 jumps; 1.32 / 1.33 message windows; 1.202 / 1.203 | E2, E3, E5 |
| `progress.rs` | story flag bits (0x33f6b0) and the area program table (0x33e86f), flag side effects from `00progress.bin` | E5 |
| `hud_msgwin.rs` | open/close from hosts and event commands (`ScriptWindow` `close` and `waiting`, E6c) | E3, E6, E6c |
| new `savepoint.rs` | type-7 objects, effect elements 1 and 2, the heal rule, the world point bit | E4 |
| `camera.rs` | `CamEvent` fed by the event player | E6 |
| new `event_player.rs` | `parse_event`, the setup and timeline readers, frame counter, message commands | E6 |
| `audio_bgm.rs` | music pair from opcode 0x10 | E1 |

## 9. Staged plan

Each stage ends with `verify.ps1` passing and the named check.

- **E1 Area transitions inside Twilight Town.** Port the interpreter (section 1) for `map` programs (spawn, music,
  triggers recorded), the area record and program table (start: the program bytes from a story state file or
  all 0), placement by entrance (spawn kind 1), exit volumes of kind-1 groups (option 0 walk-in; the prompt of
  options 1/2 logged only), the jump task with fade kind 0/1. Functions: `FUN_00181148`, `FUN_00180b48`,
  `FUN_00180ff8`, `FUN_00198e10`, `FUN_00198fa0`, `FUN_00104e18`, `FUN_00104bd0`, `FUN_001981a0`, `FUN_001d89a0`,
  `FUN_001d8a70`. Accept: `KH2_AREALOG=1` logs `area tt04 entrance 1 programs (..)`, a scenario walking from tt04
  out of the m_10 exit logs `exit -> tt03 entrance 1`; screenshot after the jump looked at (Sora in tt03).
  **Status (WP-E1 / E1b):** done for Twilight Town. `kh2_ard::run` is the interpreter (`run_area`,
  `player_start`, `Group::exit`, `Activator::contains`, oracle-checked); `kh2-app/src/area.rs` holds
  the `AreaRecord` (world 2, area from `CurrentArea`, entrance from a jump or `KH2_ENTRANCE`,
  programs (0, 0, 0) until E5) and runs the scripts when an area loads, placing Sora at the
  entrance's player entity after a jump; `area_jump.rs` tests the option-0 volumes of the loaded
  exit groups every tick (gate 0, 1, 2 as above), logs the option-1/2 prompt, and runs the jump task
  (fade out 30 ticks, load, place, fade in). Triggers on event 1 with an event tag log `event not
  played (E6)`; type-1 trigger jumps are taken, other types logged. Open after E1b (E1c status in
  brackets): the music pair of opcode 0x10 [applied]; the keyhole layout [decoded, not drawn];
  the exit lock [ported, halfwords 0 until E5]; the battle-state check [open]; the name window of
  the prompt [open]; the friends' entrance filter [done] and the camera snap after a teleport
  [done].
- **E2 NPC spawn and idle/walk.** Class-5 objects of `map` program groups with their BDX (`rtn_default`), group 5
  hosts for the walk path (+0x508), walk path ids. Accept: tt02 log of spawned `N_EX5xx_RTN` with no `missing`
  host calls in `KH2_BDXLOG`; screenshot with townspeople looked at.
  **Status (WP-E2):** done for the walk and the fixed idle (section 3, "The class-5 runtime and the walk").
  `kh2-app/src/npc.rs` spawns every type-15 entity of the loaded groups (any script: tt02's townspeople come
  from `evt` programs, `KH2_PROGRAMS=0,0,10`), runs `rtn_default` through `NpcHost` (group 5, the path hosts
  1.177 / 1.178, 0.21, 1.13 / 1.161 / 1.170 / 1.222 / 1.145 / 1.172..1.176 on the object) over the enemy host,
  follows the walk path's segments with the game's look-ahead rule and applies the walk motions' root motion.
  **Status (WP-E2b):** the native attention logic `FUN_0018a130` (kind-4 follower toward Sora in the talk part,
  kind-5 pairs of townspeople, the region draw of `FUN_0018b308`) and hosts 5.0 / 5.4 / 5.6 / 5.11 after it;
  the walk path's Hermite curve with the game's projection and step (`FUN_002d2a98`) and the random start of
  `FUN_002d5398`. No group 5 host outside `rtn_default`'s set was reached in the tt02 runs (`KH2_BDXLOG=1`: no
  `Missing` line for the townspeople). The head look-at controller is ported (section 3, "Head look-at"); not done: the
  companion objects of four townspeople (pets), the talk (E3).
- **E3 Talk and message windows in play.** First close the talk GAP (section 3), then offer 31 from the 0x83
  part, deliver event 0xb, open the window class the code opens; hosts 1.32 / 1.33. Accept: `KH2_REACTIONLOG`
  shows 31 offered and executed; a screenshot of the window looked at.
  **Status (WP-E3):** the GAP is closed (section 3, "Talk"): the talk is `FUN_00177b18` case 0x1f, not event
  0xb (the brief's "deliver event 0xb" does not happen in the original, so the port does not send it). Ported:
  `kh2_ard::libretto` (the talk table, oracle-free: layout read in the code, checked on the disc by
  `twilight_town_talk_programs`); `npc.rs` offers the part's command (`npc::part_offer` = `FUN_0019cb70` for an
  object: the 0x83 part takes the record's +0x2c, the gates, status and cone of `reaction_cmd`), accepts it with
  R / `KH2_REACTION_PRESS`, runs the talk task (Sora's velocity cleared and turned to the townsperson, the
  townsperson turned to Sora unless array bit 4 and playing motion 0x32, the pad locked while the talk runs, the
  program from the kept place, op 1 windows, op 3 logged), and ends it with `FUN_00189ff0` (motion, the previous
  follower kind 4, +0xb00 = +0xb18 + 60, mode "after_talk"). `hud_msgwin.rs` opens windows by message id
  (`ScriptWindow`), splits pages at command 0x10, measures the box over every page, draws the speech tail
  (group 47) of class 3, and for these windows reads the button in state 2 (J or K [inferred, as the
  tutorial], sound 0x18), types the next page or closes through prompt group 0x42, the tail's closing group 48
  and the frame's closing group. Run: `KH2_PROGRAMS=0,0,10 KH2_NPC_WATCH=1 KH2_NPC_WATCH_DIST=120
  KH2_REACTIONLOG=1 KH2_NPCLOG=1 KH2_REACTION_PRESS=7 KH2_KEYS=11:J,14:J kh2-game tt02`: `reaction: offered
  command 31 (message 33258) by N_EX500_RTN#1`, `reaction: accepted command 31 (Execute 31) ... talk id 0x58e,
  ops [Message(17945)]; no script event 0xb`, `hud: message window class 3 (ctor 0x1ab9e0) ... 2 page(s)`,
  `page 2 of 2`, `closed by the button`, `talk ends (FUN_00189ff0) ... mode after_talk`. Hosts 1.32 / 1.33 (the
  script message window) are enemy_host.rs's (outside this WP) and no talk path uses them.
  [inferred] the townsperson's own script and class pass keep running during the talk (no pause of the
  object was found: +0x108 bit 2, set by the task, is read only by `FUN_0019a530`); in the run above the
  kind-4 follower's timer (360..720 frames) ran out 1.8 s into the talk and `rtn_default` left
  `encount_player` for `path_move`, so the townsperson walked on while talking [to compare with footage].
  Not ported: the talk camera (`FUN_00159688` / `FUN_00159768`, camera.rs), the party's head turns, the
  wait's early end (`FUN_001a5b18`), op 3 events (0x65 goes to `FUN_0023f108`; the shop events 0x13, 0x66.. are menu kind 4, docs/menus.md 21),
  HUD record 0xc.
  **Status (WP-E8):** the talk camera is ported (docs/camera.md "Talk camera (mode 4)"): `npc::talk_step`
  builds the shot from the townsperson's collision record of type 0x13 with the talk part's index
  (`npc::talk_shot`, `FUN_0018f108`) and hands `camera::TalkCam::Start` / `End` to the camera, which cuts
  to the view on its next tick and warps back to the follow camera at the end. The townsperson's hold was
  looked for and is **not in the code**: the talk task sets the target's +0x108 bit 2, whose only reader
  `FUN_0019a530` is the body push-apart (`FUN_001b5bd0`, `FUN_001b5eb0`, `FUN_001d1908`: an object with the
  bit is not pushed); the class pass `FUN_0018a130` counts the follower timer (`FUN_0018b228`) and +0xb00
  every frame unless the object is a child (`FUN_001beb00`: a live parent at +0x570), with no test of the
  talk or the player's lock; host 5.11 (`FUN_0018b250`) holds a kind-4 follower only while the timer runs
  and the player meets the talk part. So in the code read the timer the follower got when Sora entered the
  part (360..720 frames, `FUN_00106408(+0xb10, +0xb14)`) keeps running through the talk, as the app does;
  no hold is added [the clips show no townsperson talk to compare with]. `FUN_00192c50(target, 0x21)` /
  `FUN_00192c68` run the entries of the target's +0x670 list through their method +0xc with 0x21
  (`FUN_0011e8c0` / `FUN_0011e930` -> `FUN_001330f0` / `FUN_00133168`) [GAP: what that list holds; not ported].
  **Status (WP-E3b):** the accept is `reaction_cmd::accept`: with no enemy offer it takes the candidate that
  `npc.rs` / `savepoint.rs` built with `npc::part_offer` (held in `Talk::offer`; `reaction_cmd::native_offers`
  now walks an enemy's parts through the same `part_offer` instead of a copy) and dispatches by the row's
  Execute: 0x1f (the talk, `FUN_00177b18` case 0x1f) and 0x37 (the save point, case 0x37) are left in
  `Talk::accepted` for `npc::talk_step` / `savepoint::point_use`; any other Execute (the shops' 30, ...) logs
  `reaction: accepted command N (Execute E): not ported`. `npc.rs`'s own accept key is gone (R and
  `KH2_REACTION_PRESS` are read once, by `reaction_cmd`), and the misleading `accept pressed, nothing offered`
  no longer follows a talk. Still only the HUD's capsule needs the candidate in `ReactionOffer`
  (`npc::talk_show_offer` / `talk_hide_offer` around `hud_menu::mirror_reaction`, outside this WP's files).
  The offer and the accept run one fixed tick apart (the accept reads the candidate of the tick before).
- **E4 Save points.** Type-7 objects, effect elements 1/2, heal while command 55 is offered, the world point bit.
  Accept: log `save point heal` when HP was below max; screenshot of the point.
  **Status (WP-E4):** `savepoint.rs` spawns every type-7 entity of the loaded groups (no mesh: an effect
  bound to the record's place), starts element 1, sets the world point's bit (kept in the save block's story,
  `progress::Story::set_world_point` / `world_point`, 0x33f910; WP-E3b; state-file key `world_points`; the bit is set
  when the point is created, `FUN_00194ab8` -> `FUN_00194fa8`, not when it is used), offers command 55 from the cylinder part with the status of
  `FUN_001ba2c0` case 0x37 (the drive form is the only bit of 0x35fec0 the app knows), heals Sora 1 HP and 1 MP a
  frame while the offer is available (`FUN_00194c08`; MP left alone during a charge), starts element 2 while
  something is restored and ends it after (`FUN_00194b98`), and logs the use (`FUN_001784b8`: menu kind 1 with
  mode 0 or 2, not ported; the save point does not open `menu_save.rs`'s kind 7, which the original's point does
  not open either). Run: `KH2_SAVEPOINT_WATCH=5 KH2_NO_ENEMIES=1 KH2_REACTION_PRESS=8 kh2-game tt02`: `save point:
  world point 47 of 00worldpoint.bin (world 2, area 2) seen`, `save point: offered command 55`, `save point
  heal: HP 5/20 MP 100/100`, `effect element 2 started`, `effect element 2 ended`, `save point heal: done, HP
  20/20`, `save point: accepted command 55 (Execute 0x37): ... menu kind 1 (scene 3) with mode 0`. Not ported:
  the friends' slots of the heal (party.rs), the MP charge shortening (`DAT_00360130` +0x7c), bits 2 and 3 of
  0x35fec0. **WP-E7:** the accept now opens menu kind 1 through `menu_save.rs` (docs/menus.md 18.5). Run
  `KH2_REACTION_PRESS=8 KH2_SAVEPOINT_WATCH=5 KH2_REACTIONLOG=1 KH2_NO_ENEMIES=1 KH2_KEYS=11:Down,12:J,13:K
  kh2-game tt02`: `... with mode 0 (FUN_0024f0b8(1, 0)): requested from menu_save`, `menu: save open (menu kind 1
  mode 0 from a save point, rows [8a7e, 8a80], ...)`, `menu: save point row 1 (0x8a80): ... not ported`, `menu:
  save point menu closed (menu kind 1 mode 0): back to the field (FUN_001038a0)`. tt02's world point (m_20) wins
  the offer over the save point (m_21) standing at the same place, so this run shows mode 0.
- **E5 Story flags selecting battle programs.** Story flags in `Progress` (bits, side effects op 1, 7, 0xc),
  hosts 0.14/0.15 real, the btl program from the area record, triggers with events 2/3 and the battle manager
  results (jump records type 1/2). Accept: with a state file setting flag 0x883, tt04 logs `btl program 1`; the
  Dusk drop rows of docs/enemies.md now follow real flags.
  **Status (WP-E5):** `progress::Story` holds the flag array (0x33f6b0, 19 pages of 0x20 bytes), the program
  table (0x33e86f) and the lock halfwords (0x33f918); `Story::set` is `FUN_0017ff60` with ops 1, 2, 3, 4, 5, 7
  and 0xc (oracle `story_flag_setter_matches_the_game`: 1415 flags of 19 worlds equal in kh2-ee, 870 of
  them change more than their own bit); `KH2_STATE` keys `story_flags` / `story_clear` (docs/progress.md).
  Hosts 0.14 / 0.15 / 0.70 use it (`progress::shared_story`). `area.rs` takes the record's program bytes
  from the table on every area load (`KH2_PROGRAMS` still overrides) and applies opcode 0x19. Exits read
  the lock halfwords (test `story_flag_ops_lock_and_unlock_exits`). Run (`KH2_AREALOG=1`, state file
  `{"story_flags": [2179]}`): `area tt04: map program 0, btl program 1, evt program 0 from the program
  table 0x33e86f (story flags, FUN_00104bd0)` and `btl program 0x1 loads group b_00`; flag 0x883 also
  writes tt03 (3, 1, 0), tt06..tt08 btl 1, tt13 btl 2, tt14 evt 3. Without a state file the log reads
  `programs (0, 0, 0)` as before and tt04's program-0 scripts have no opcode 0x19 (no `opcode 0x19` line). Not done (done since WP-EN1: `enemy.rs` sets up the btl
  program's groups at load, docs/enemies.md "Waves"): the waves of `enemy.rs` still took turns; trigger sub 2 (flag on fire), events
  2 / 3 and the battle manager's result jumps. (Jump types 2 and 3 are ported since WP-JT2: section 4,
  "Jump types, status".)
  **The prologue Seifer fight (TT04_MS500) with its story state** [measured]: flag 0x8db (the one `btl`
  program 0xb5 sets by opcode 0x19) has one side effect, op 6 (`FUN_001a7730(2, 1)`: `0x33fe0c[2]`), so
  it changes no program byte, lock or flag; nothing in the three 0xb5 programs tests a flag. The game
  reaches 0xb5 through a type-2 jump with key 0xb5 (`07localset.bin` gives tt04), which makes map, btl
  and evt all 0xb5. Run `KH2_PROGRAMS=b5,b5,b5 KH2_WAVE_GROUP=b_45 KH2_STATE=<{"story_flags":[2267]}>
  KH2_AREALOG=1 kh2-game tt04`: `opcode 0x19 story flags [8db], newly set []`, groups b_45, `e55_`,
  `f55_`; the `evt` groups spawn two townspeople, `N_EX580_RTN` (object 0x523) at app (206, 0, 1254)
  and `N_EX590_RTN` (0x524) at (-106, 0, 1257), both class-5 `_RTN` objects with no walk path (not a
  board or a prop). Screenshot at 5.2 s (during the intro camera) looked at: the INFORMATION bar
  "Defeat Seifer!" and those two townspeople in front of a house wall with two trees, no fighter in view.
  So the story state does not move Roxas or Seifer: b_45's places stand; the intro's first shots look
  at where `evt` 0xb5 puts the two onlookers [inferred: they are the spectators of the fight; the clip's
  notice board is still not found in any 0xb5 group].
- **E6 The event player for one short event.** Parse, setup reader, frame counter, actors (opcode 1, 2), motions
  (0x24 + `.anb`), message lines (0x15, 0x2d), the camera cut table (opcode 6) into `CamEvent`; skip later. Start
  with a tt01 event of under 600 bytes (`100`, `200`, `400`, `600`). Accept: log of commands at their frames; a
  screenshot with a subtitle line looked at.
  **Status (WP-E6):** `kh2-app/src/event_player.rs` (a submodule of `intro.rs`, plugin added by `IntroPlugin`).
  `Event::parse` sorts the commands (opcode 0 name, 8 end, 1 actors, 0x24 motion loads, 2 placements, 0x2c
  motions, 0x15 subtitle lines, 0x2d windows, 0x44 movies, the camera through `intro::Track`); `due` is the
  timeline cursor; the frame is game ticks times 0.5. Actors are made at their first 0x2c from the objentry
  row's model (`obj/<model>.mdlx`), their `.anb` read from `anm/<world>/<model>/` or, when absent there,
  `anm/us/<world>/<model>/` [inferred: the USA disc keeps the talking motions there; the loader's remap was
  not read], started with `Animator::play_external` (blend and start in game frames); opcode 2 places them
  (`game_to_app`, `app_yaw` of the yaw). Subtitle lines draw as a HUD text node: font 4, colour 3, at
  (256, 365), alignment 4 (docs/hud.md, class 0x1aae60 [inferred there]), the open lines' messages in open
  order. 0x2d opens `hud_msgwin::ScriptWindow` (kind 0 / 1 = class 0 / 1); its close and wait are not ported.
  The camera runs `CamEvent::track` while the event has records and cuts. The pad is locked while the event
  plays [inferred]; at the end frame the actors are removed and control returns. The short tt01 events are
  movies (above): no movie is played (no PSS decoder) [GAP], the counter runs on game ticks instead of the
  movie's frame [inferred]. Triggers: the `WalkIn::Event` arm of `area_jump.rs` hands the event to the player
  (`event_player::request`); the jump after it is not taken yet. No tt01 trigger names `100` .. `600` (they
  name `101`, `201`, `303`, `401`, `501`, `601` with event 0, test `twilight_town_tt01_short_events`).
  Harness: `KH2_EVENT=<tag>` plays the loaded area's event at `KH2_EVENT_AT` seconds (default 2);
  `KH2_EVENTLOG=1` logs every command at its frame and Sora's place around the end. Runs:
  `KH2_EVENT=102 KH2_EVENTLOG=1 KH2_NO_ENEMIES=1 KH2_NO_NPC=1 kh2-game tt02` logs `event: tt02 102
  (tt_event_102, arg 0) starts: 387 commands, end frame 5712, 9 actor(s)`, `actor 1 h_ex510 (object 0x2c3, ...)
  made (op 0x2c, FUN_0016e8c0)`, `op 2 actor 1 at (7, 0, -339) ...`, `frame 788 (cmd frame 788): op 0x15
  subtitle opens key 0 message 0x3376`; a screenshot at 29 s looked at: three of the friends in the event's
  camera view and one subtitle line at the bottom centre (the HUD still drawn, see below). `KH2_EVENT=200
  KH2_KEYS=30:W:3,74:W:3 kh2-game tt01`: `tt01 200 over at frame 2129 (end frame 2129, ...): ... control back
  to the player`, `Sora moved 0 cm while the event played; 6 s after its end ... 345 cm from where the event
  left him`. Not ported: the HUD hiding of the event start (`FUN_001bef48(0xc)`, `FUN_0014c748(4)` in
  `FUN_00196428`), hiding the player and party during the event, opcodes 3, 0xd, 0xf, 0x10, 0x12, 0x14, 0x16..0x19,
  0x1a, 0x1b, 0x22, 0x23, 0x29, 0x2a, 0x2b, 0x2e, 0x36, 0x3d (logged `not ported`), the voices
  (`voice/<world>/event/`), skipping, the event object's states `FUN_00195c08`, the end events 0x12 / 99
  (`FUN_00225028`) and the chain 0x4c.
  **Status (WP-E6b):** (1) The jump after a trigger's event (section 6, "What the start holds back"):
  `area_jump::AfterEvent` keeps the trigger's jump record while the event plays and takes it on the first tick
  the player no longer plays (`FUN_00196170` -> `FUN_0019d9d8(+0xc, 0x80000000)`), type 1 only, with no fade-out
  [inferred: bit 31 makes `FUN_00104e18` record the kind without fading, as the 0x4000 bit does]; an event that
  never starts holds it two ticks at most [inferred]. Test `the_jump_after_a_trigger_event_waits_for_the_events_end`.
  The disc's walk-in triggers with an event (test `twilight_town_walk_in_triggers_with_events`): tt03 `evt`
  program 4, groups 0x3a / 0x2a / 0x3c -> event `605` arg 5, then type-1 jumps to tt02 / tt04 / tt06 entrance 0.
  The harness cannot walk Sora into those option-0 volumes (he stops short of them at entrances 0 and 1), so
  `KH2_WALKIN=<group id hex>` (`KH2_WALKIN_AT`, default 3 s) fires a group's walk-in decision directly. Run
  `KH2_PROGRAMS=0,0,4 KH2_ENTRANCE=0 KH2_WALKIN=3a KH2_EVENTLOG=1 KH2_NO_ENEMIES=1 KH2_NO_NPC=1 kh2-game tt03`:
  `trigger (1, 0x3a) plays event 605 arg 5, then the jump Some(Jump { kind: 1, world: 2, area: 2, ...`,
  `tt03 605 over at frame 360`, `jump after the event (FUN_00196170 -> FUN_0019d9d8 with bit 31): type 1 -> tt02
  entrance 0`, `jump: tt02 loaded`. (2) The event kind (`evtp` row of `arg & 0x3f`): the pad is locked only for
  flag bit 3 clear, the field HUD hidden only for bit 2 clear (`event_player::field_hud_hide`, at the end of the
  HUD chain of `hud.rs`: every visible node but the subtitle line, given back at the end) [inferred which app
  nodes are the tags 3 to 6, as `hud_mission`]; `KH2_EVENT` takes `KH2_EVENT_ARG`, else the loaded trigger's
  arg for the tag, else 0. **The original's tt02 `102` (trigger arg 2, kind 2, flags 0x65) keeps the field HUD**
  (bit 2 set): the screenshot of `KH2_EVENT=102 KH2_EVENT_ARG=2` at 29 s shows the command menu, the gauges and
  the party faces under the subtitle; with `KH2_EVENT_ARG=12` (kind 0xc, flags 0x70) the same frame has no
  field HUD. Kind 0 has no row (the `evtp` rows are 1..9, 0xb..0x13): nothing is held back for it. No code was
  found that hides the player or party models (section 6). (3) The subtitle label is a `hud::Label` built
  directly. (4) Opcode 0x2d close and wait: rule read (section 6, "Message lines"); ported by WP-E6c (below).
  **Status (WP-E6c):** opcode 0x2d actions 1 and 2 are ported. `hud_msgwin::ScriptWindow` has a close request
  (`close`, the owner's vtable +0x20 = `FUN_001aacb0`: the closing groups start at once, from any open state; no
  prompt group 0x42, which only the button's method +0 starts [inferred]) and a state-3 flag (`waiting`, set when
  the button closes the last page, cleared by a new open). `event_player::WindowSlots` is the slot table of
  `FUN_002275a8` (32 slots keyed by key +6 and message +8): action 0 fills a slot and opens the window, action 1
  closes the slot's window and clears the slot (a window not in a slot: nothing), action 2 sets the hold
  (`DAT_0036b9c9`) only when the slot exists, and `step` then does not advance the frame counter
  (`FUN_00222728`) until `waiting` is set; `closed` also releases the hold [inferred: a window that never
  opened cannot reach state 3]. The skip test of `FUN_002225e0` is not ported (no skipping). Test
  `opcode_2d_close_clears_the_slot_and_wait_holds_the_counter`. Log lines (`op 0x2d ... action 1 (closed, no
  wait)`, `action 2 (counter held ...)`) appear without `KH2_EVENTLOG`. The disc's events: see the run in the report
  (tt02 `102` has no 0x2d action 1 or 2 if its log shows only action 0 or none).
  **The prologue Seifer intro (TT04_MS500)** [V, test `the_seifer_mission_events_have_no_actors`]: the mission
  file's two events (`re_msstart-1a`, end 130, one camera cut; `re_mscomp-0a`, end 15) have no opcode 1, 2 or
  0x2c: the events place no actor, so the b_45 places of Roxas and Seifer stand and `intro.rs` is unchanged.
  **Status (WP-E6d):** opcodes 3, 0xd, 0xf, 0x10, opcode 0x2c +0xa / +0xc and load sub-records 0x26 / 0x39 /
  0x3e read (section 6). None of 3, 0xd, 0xf, 0x10 is an actor show/hide: 3 has no handler, 0xf opens the
  timeline, 0x10 sets the length of the fade-in after the setup (E6d called it an "end fade"; that was wrong,
  see the E6e status below), 0xd calls a model-instance method (resolved in E6e: the drawn mesh groups).
  Ported in `event_player.rs`: `Event::begins` (no 0xf, no timeline), `Event::end_fade` (the opcode-0x10 value, logged), `Motion::loop_start` /
  `loop_end` with `MotionClock` (`FUN_0022ca18`, test `motion_slots_loop_past_their_end_frame`),
  `Event::voices` / `effects` / `layouts` with `voice_path` / `effect_path` / `layout_path` and `REGION` (test
  `twilight_town_event_load_records_name_files_on_the_disc`); the movie log now names `movie/us/`. Requests
  carry the area they were made in (`take_request`, test `a_request_made_in_another_area_is_dropped`). [E6d's
  "Not ported" list below is superseded by the E6e status for the fade, opcode 0xd and the put-back place.] Run
  `KH2_EVENT=102 KH2_EVENT_ARG=2 KH2_EVENTLOG=1 KH2_NO_ENEMIES=1 KH2_NO_NPC=1 kh2-game tt02`: `event: 102 load
  blocks name 32 voice(s) ["voice/us/event/tt10203or.vag", ...], layouts ["event/layout/us/tt_telop.2ld"] ...
  op 0xf present: the timeline runs (FUN_00226f50); end fade Some(20) frames`, `frame 3164 ...: op 0x2c actor 2
  n_ex500 plays anm/tt/n_ex500/ETT00008E.anb (start 99, blend 0, loops 99..164 (FUN_0022ca18))`, `frame 3229:
  motion anm_tt/ETT00008E loops back to frame 199`, `op 0xd actor 2 model instance (+0x670) method +0x34 ...
  not ported`. Not ported: voices, event effects and layouts (loaded only), the end fade, opcode 0xd, the place
  put back at a loop.
  **Status (WP-E6e):** the event object's screen fades are computed and the three E6d leftovers are ported
  (section 6, opcodes 0x10, 0xd, 0x2c loops). `kind_rows` reads the `evtp` rows; `start_fade` (state 0,
  `FUN_00195d28`: out to black or white over the row's length, times `DAT_0037b050`) holds the timeline until it
  has run (`FUN_001966f8`); `setup_fade` (`FUN_00195ff8`: in over twice the opcode-0x10 frames times
  `DAT_0037b054`, from black for row byte 1 or 3, from white for 2) starts after the setup;
  `EventPlayer::screen_fade` answers (alpha, white). Opcode 0xd hides the actor's mesh entities of unlisted groups
  (`mesh_groups`, `mesh_flags`); a looping motion puts its actor back at the place kept at the loop's first step
  (`keep_place`, `put_back`); opcode 0x4d is logged only (which objects fall below tag 2000 is unresolved, so
  nothing is hidden; ported in WP-EV1, below). Tests `the_event_object_fades_out_then_in_by_its_kind_row`,
  `opcode_0xd_draws_only_the_listed_mesh_groups`, `a_looping_motion_puts_its_actor_back_where_the_loop_began`.
  Not ported: the keyhole layout of fade kind 3, the flag-bit-4 faders, the end-of-event faders.
  **Status (WP-EV1):** opcode 0x4d ported (section 6: event actors that exist at the pass are hidden or shown), opcode 0xd's
  +0x34 and flag 2 and opcode 0x2c +6 recorded (section 6, Unknowns). Accept scenarios `event-fade` and `tt04-m_ex850`
  looked at after the change: unchanged (neither event nor script has an opcode 0x4d).
  **Status (WP-E6f):** `area_jump::draw_fader` draws the event fade: the one full-screen node takes the stronger
  alpha of the jump's black and the event's fade, white when the event's wins (`combined_fade`, test
  `the_event_fade_and_the_jump_fade_share_the_node`; `draw_fader` itself: `draw_fader_paints_the_node_from_the_jump_task`). The merge rule
  (the stronger alpha wins, a tie keeps the jump's black) is the port's own [inferred]: no rule for two faders at once was found.

## 10. Unknowns (with what was searched)

- The talk wait's early end (`FUN_001a5b18`: `FUN_0019abb0` of the player with command row 10) and `FUN_00192c50`'s
  +0x670 list; whether the townsperson's script is held during a talk (section 3, E3 status).
- The head look-at controller's joint turn (the pose code behind `FUN_001aead0`'s output, section 3 "Head
  look-at": how the joint is turned toward the aim point, and the +0x160 enable bit) and `FUN_0018af10`'s kind-3
  answer (+0xa80); the walk path's +0x87 mode and +0x8c stop index (no writer found in `FUN_002d3b40`). Host 5.8 / 5.9 / 5.10 / 5.12..5.15 / 5.18..5.25 / 5.28..5.32 (not called by `rtn_default`) are not read.
- Menu kind 1's World Map row (`FUN_00257e70` question, `FUN_002580b8` -> `FUN_0014a7c8(.., 0xf, 0)`, page 0x14
  `FUN_002580e8` -> `FUN_00103b18(1)`: what the exit loads was not read) and its page-0 help (docs/menus.md 18.5);
  `FUN_0024f300` meaning (trigger sub 7).
- Opcodes 0xa, 0xb, 0xe, 0x11, 0x16 meanings (readers named in the table); trigger subs 3, 6, 8; flag ops 2..5.
- Group kinds 7 and 9 (`FUN_001ccb10`, `FUN_0019a000` not read); activator +0x28..+0x3f beyond +0x2c/+0x2e/+0x2f.
- The event kind table `DAT_01d6a92c` (rows read by `FUN_001d1128`): its loader was not searched (the rows are
  the `evtp` entry of `03system.bin`, `struggle::event_kinds`).
- Whether anything else hides the field HUD during a kind-2 event such as tt02 `102` (flags 0x65, bit 2 set):
  `FUN_00196760` is also called by `FUN_00196258` (no `jal` or address reference found, `mips_scan --jal
  0x196258 --addr 0x196258`), `FUN_0024f0b8` (menu start) and `FUN_002c6a40`; `FUN_0014ef48` by `FUN_002cc1e0`,
  `FUN_002d16b8` (not read). The event commands 3, 0xd, 0xf, 0x10 were read (WP-E6d): none hides an actor. [WP-E6e] Hiding the player and
  the party during an event: searched `FUN_00195ae0`, `FUN_00195d28`, `FUN_00195e68`, `FUN_00195ef8`,
  `FUN_00195f68`, `FUN_00195fb8`, `FUN_00195ff8`, `FUN_00196138`, `FUN_00196170`, `FUN_00196280`, `FUN_00196760`,
  `FUN_00196348`, `FUN_001963e0`, `FUN_00223a88`, `FUN_002233d8`, `FUN_002235c0`, `FUN_002234e0`, and every
  `0x22xxxx` event handler that calls into `0x1a0000..0x1bffff`: the only object hide is opcode 0x4d
  (`FUN_00226e80`, section 6), whose selection (resource tag below 2000) is not resolved; 6 commands in tt.
  Not ported. [WP-E6f] The tag writers, traced: the tag (resource +0x28) is the second argument of
  `FUN_0016f9c0` (`FUN_0016f358` stores it); its only caller `FUN_0016fae0` is called by `FUN_0016e430` (the
  object's model load) and `FUN_001d1c80` (tag 5000, a fixed resource). `FUN_0016e430` takes the tag from its
  its second argument unless that is -1; with -1 it is the word table at 0x37a278 indexed by the object entry's ObjectType
  byte (entry +4; row = `FUN_001d1470`), except 0x960 (`FUN_0016f138`, an area-state tag) and a few ids
  `FUN_0016ef80` / `FUN_0016efd8` say are special. That table reads (types 0..23): 2300 2400 2500 2600 2700 1090
  1090 2100 1090 2500 1100 1110 2600 1090 5100 2500 2500 2500 1030 5000 2100 2700 2200 1090, so by the default
  path types 0..4 (the player and party types) get tags of 2300 or more and are NOT below 2000; types 5, 6, 8, 10,
  11, 13, 18 and 23 are [inferred: the objects opcode 0x4d hides]. Not proved: the callers of `FUN_0016e430`
  that pass an explicit tag (not enumerated) could override it for Sora. Result: no event flag or command found
  that hides Sora and the party; nothing ported then [superseded by WP-EV1, section 6] (opcode 0x4d stayed logged, because mapping the app's entities to
  ObjectType and the explicit-tag callers is not done).
- Opcode 0x2d: the event skip branch of action 2 (`FUN_002225e0`) is not ported; the window state used is the port's state 3 (the button closed it).
- What pause-menu record 0xc (`FUN_001bef48(0xc)`, table `DAT_01d5df08`) allows or forbids; bit 4 of
  `DAT_00356f1c` stops the play clock (`FUN_0014c7b8`) and is tested by the pause menu `FUN_0014a080`.
- Music commands of events, skip; opcode 2 +6 (not read by `FUN_002293b8`; no other reader searched). Opcode 0x2c
  +6 [WP-EV1]: `FUN_0022cd90` and `FUN_0022c9b0` (which copies +8, +0xa, +0xc, +0x10 into the motion row) never read it;
  the timeline loop `FUN_00223a88` reads only +2 (opcode) and +0 (length) before the switch; the setup reader
  `FUN_002245d0` and the pre-load reader `FUN_00222a88` have no case for 0x2c; `mips_scan --load-off 6` over
  0x220000..0x23ffff finds 0x2c's handler and callees absent from its hits (the others are other opcodes'
  handlers). So +6 is a field the original never reads [inferred: an editor value; the disc's values are not
  used]. Opcode 0xd flag 2 and opcode 0x4d: resolved in WP-EV1 (section 6). Which command plays the event voices that sub-record 0x26
  loads; the users of sub-record 0x39's effects and 0x3e's layout handle `DAT_01dc0078`; how `anm/`
  paths reach the `anm/us/` copies on the USA disc (`FUN_00167e58` not read).
- Where `arif` music pairs are read by the game (only opcode 0x10 writes the pair the area load reads).
- Exit lock: the new-game value of the halfwords at `0x33f918` (assumed 0; the initialiser was not
  read) and the world-0x10 branch `FUN_001d32e0` (not ported). The flag side effects ops 2..5
  that write the halfwords run since E5; the world-point bits ops 4 / 5 clear (`FUN_00194f68` on
  the table `DAT_01c8a984`, `FUN_00194fd8` on 0x33f910) are not kept (no world map).
- The keyhole wipe (fade kind 0, drawn since WP-E1d): whether the game hides the HUD while it
  plays (the app draws the command menu and area card over it), and the window sides beyond
  the 4:3 picture, which the layout does not cover [inferred: the original is 4:3].
- The meaning of the area special values 100 and 101 in exits beyond the code's mapping.

## Decompile index

| address | what |
|---|---|
| 0x1048e0, 0x1689c0, 0x180870, 0x181148, 0x180a68, 0x180ad8, 0x180b48 | area thread, load, script run, interpreter |
| 0x180ff8, 0x1975e0, 0x197588, 0x197670, 0x1978b0, 0x1979f8, 0x197a60, 0x197b70, 0x197780, 0x1976d8 | triggers and event firing |
| 0x196428, 0x195898, 0x195ae0, 0x1959f0, 0x1964d0, 0x195c08, 0x195d28, 0x196138, 0x196170, 0x19d9c8, 0x19d9d8, 0x196280, 0x1962a0, 0x196760, 0x196790, 0x196258, 0x1bef10, 0x1bef48 (disassembled), 0x1befc0, 0x14c748, 0x14c760, 0x14c780, 0x14c650, 0x138708, 0x138728, 0x1b7ce8, 0x1b7d18, 0x1d1128, 0x2275a8, 0x222528, 0x222538, 0x222588, 0x2225e0, 0x222728, 0x2c6a40 | event object start / end, kind flags, jump after the event, opcode 0x2d (WP-E6b) |
| 0x1aece8, 0x1aec78, 0x1aec50, 0x1aeda8, 0x1aea28, 0x1ae9e8, 0x1aee90, 0x1aee38, 0x1aead0, 0x1abf40, 0x1abf78, 0x171940, 0x171978, 0x1719a8, 0x189dc8, 0x228778, 0x1894d0, 0x16bbe8, 0x16a890, 0x11c2a0 | head look-at controller (WP-HL1) |
| 0x181728, 0x1817f0, 0x198430, 0x198750, 0x198920, 0x1981a0, 0x198220, 0x1982c8, 0x1989d8, 0x198b30 | spawn groups |
| 0x198e10, 0x198fa0, 0x199058, 0x1991a0, 0x199518, 0x199930, 0x199b18, 0x199e68, 0x199e98, 0x199f70, 0x199c48, 0x199c70 | group kinds |
| 0x1d8b48, 0x1d8600, 0x1d89a0, 0x1d8a70, 0x334388 | activator volumes |
| 0x1d88a0, 0x1d8908, 0x1d8958, 0x19a3f8, 0x19a440, 0x1b80e8, 0x1b7e00, 0x19b110, 0x17be98, 0x16a890, 0x195358 | exit gate and lock (E1b) |
| 0x13ef68, 0x13efe8, 0x13f148, 0x13f1a0, 0x14a7c8, 0x14a7f0, 0x14a710, 0x15d058, 0x15d1a8, 0x104d58 | fade kinds and the jump task (E1b) |
| 0x104e18, 0x104d58, 0x104c68, 0x104ce0, 0x104bd0, 0x104b90, 0x105188, 0x105010, 0x19d8f0, 0x19d9d8, 0x13ef68, 0x13efe8, 0x198080, 0x198058, 0x172118, 0x172170, 0x149df0, 0x1abfe8 | jumps |
| 0x1802e0, 0x180298, 0x17ff60, 0x17fd10, 0x180350 | story flags |
| 0x16d5a0, 0x16d5b8, 0x16d5e8, 0x180330, 0x104b90, 0x104bd0, 0x195260, 0x195288, 0x1952a0, 0x195300, 0x194f68, 0x194fd8, 0x1a7730, 0x1b4e80, 0x1a52b0, 0x1a52f0, 0x195130 | story flag hosts, side effects, program table (E5) |
| 0x16e8c0, 0x189858, 0x1a51e8, 0x2ea050..0x2ea978 | objects by type, NPC class, group 5 hosts |
| 0x1899d0, 0x189ae8, 0x189c68, 0x189dc8, 0x189e50, 0x189e70, 0x189938, 0x189960, 0x189990, 0x18a130, 0x18a4b0, 0x18a598, 0x18a7a0, 0x18a828, 0x18a8b0, 0x18a918, 0x18aa08, 0x18ae08, 0x18aef8, 0x18af08, 0x18af10, 0x18b0d8, 0x18b0f0, 0x18b110, 0x18b138, 0x18b148, 0x18b1e0 (table 0x37ab9c: 0x18b208, 0x18b210, 0x18b218, 0x18b220), 0x18b228, 0x18b250, 0x18b308, 0x199ca0, 0x199cc0, 0x199d18, 0x199d68, 0x1894d0, 0x189208, 0x1d8c18, 0x16b078, 0x16b960, 0x16bbe8, 0x16b558, 0x16b6e0, 0x16a890 | class 5: init, update, followers, behaviour entries, regions (E2) |
| 0x183c30, 0x183d08, 0x183dc8, 0x183e18, 0x1719d0, 0x171a08, 0x2d2100, 0x2d2230, 0x2d2328, 0x2d2350, 0x2d23b8, 0x2d2440, 0x2d2448, 0x2d2460, 0x2d25b0, 0x2d27c8, 0x2d2a98, 0x2d3148, 0x2d32f8, 0x2d3b40, 0x2d3d00, 0x2d3db8, 0x2d4040, 0x2d4298, 0x2d43a0, 0x2d4438, 0x2d4478, 0x2d4590, 0x2d4688, 0x2d4800, 0x2d4ad8, 0x2d4dc8, 0x2d50c0, 0x2d51e0, 0x2d5398 | the walk path record and its spline (E2) |
| 0x16cfd0, 0x1718d0, 0x171908, 0x171940, 0x171978, 0x1719a8, 0x1af878, 0x1af650, 0x1aece8, 0x1aec78, 0x1aec50, 0x1abf40, 0x17bf10, 0x1705d0, 0x1706c8, 0x171b30, 0x170808 | hosts 0.21, 1.172..1.176 (talk cone, look-at), 1.145, 1.161, 1.170, 1.222 (E2) |
| 0x18a130, 0x18a4b0, 0x18a598, 0x18a7a0, 0x18a828, 0x18a918, 0x18aa08, 0x18b148, 0x18b250, 0x18b308, 0x18ae08 (disassembled 0x18ae08..0x18af18), 0x18af10, 0x18b138, 0x199d68, 0x198750, 0x1d8600, 0x1d8b48, 0x1d89a0, 0x1d8c18, 0x16b078, 0x16b928, 0x16a890, 0x1b7c88, 0x19a3f8, 0x106308, 0x1063b8, 0x106408, 0x189ff0; data 0x37ab88, 0x37ab98, 0x37abd4, 0x3780e0, 0x3780e4, 0x356f40 | class-5 attention logic (E2b) |
| 0x2d3b40, 0x2d2100, 0x2d2350, 0x2d32f8, 0x2d4298, 0x2d2460, 0x2d43a0, 0x2d4438, 0x2d4040, 0x2d4dc8, 0x2d4ad8, 0x2d4800, 0x2d4688, 0x2d50c0, 0x2d51e0, 0x2d5398, 0x2d2a98, 0x2d25b0, 0x2d27c8, 0x2ea080, 0x183e18; data 0x38379c..0x3837fc, 0x383814..0x383874, 0x383e80 | the walk path curve (E2b) |
| 0x1aece8, 0x1aec78, 0x1aec50, 0x1aeda8, 0x1aee38, 0x1aead0, 0x1ae9e8, 0x1aee90 (header only), 0x189dc8 | look-at controller, not ported (E2b) |
| 0x177c20, 0x16b420, 0x16b470, 0x172360, 0x1723f0, 0x2c5cf0 | commands to targets, script message window |
| 0x1cf030 (decompiled for E3), 0x177b18, 0x177d28, 0x1a56c0, 0x1a54a8, 0x1a53f8, 0x1a5728, 0x1a5a60, 0x1a5510, 0x1a5608 (0x1a5728/0x1a5a60/0x1a5510 decompiled for E3), 0x1a53b8, 0x1a5b18, 0x1b6408, 0x1b6398, 0x1b64b0, 0x1b64e0, 0x1b6510, 0x1b6578, 0x1b65e0, 0x1b6740, 0x1b67a0, 0x1d8d18, 0x30ff40, 0x332dc8, 0x16b870, 0x1b7ce8, 0x1b7d18, 0x1b7d50, 0x176a18, 0x169978, 0x189ff0, 0x189f28, 0x189f40, 0x189e90, 0x189eb8, 0x189ae8, 0x19a530, 0x192c50, 0x192c68, 0x196418, 0x102460, 0x159688, 0x159768, 0x19abb0, 0x1b6f48; data 0x35f1d4, 0x37ab68, 0x37ab78, 0x37ab84, 0x37bb08 | the talk (E3) |
| 0x1aba58, 0x1ab9e0, 0x1aa628, 0x1aa6b8, 0x1aa680, 0x1aa780, 0x1ab170, 0x1ab328, 0x1ab358, 0x1aacb0, 0x1aacd8, 0x1aa430, 0x1aa3a0, 0x1aa3f8, 0x1aa4e8, 0x1ab370, 0x1ab4e8, 0x1a8c78, 0x1a8b48, 0x190fa8, 0x190f68, 0x1aad78; data 0x35eb40, 0x35ea68 | the talk window: tail, button, pages, closing (E3) |
| 0x1784b8, 0x16b940, 0x1699a8, 0x169958, 0x176758, 0x255598, 0x254bf0, 0x1ba2c0, 0x1c6c08, 0x1c6bd0, 0x194c08, 0x194b98 (decompiled), 0x1a0140, 0x1a0210, 0x1a0240, 0x1a0100, 0x1a0180, 0x1a05d0, 0x1a0540, 0x186368, 0x1792f0, 0x176c98, 0x19fe48, 0x19ffb8, 0x194f68, 0x194f08, 0x189208, 0x188fd0, 0x1890d8, 0x188f08, 0x16a570, 0x1d9f20, 0x1d9f48, 0x1d9f80; data 0x380560, 0x37ab20, 0x37ab24 | save points: offer, heal, use (E4) |
| 0x194ab8, 0x194b68, 0x194b98 (decompiled for WP-7G and E4), 0x194c08, 0x194d78, 0x194e68, 0x194ee0, 0x194f68, 0x194fa8, 0x195010, 0x19c0f0 | save and world points |
| 0x196428, 0x195898, 0x195ae0, 0x195c08, 0x1964d0, 0x2232c8, 0x2233d8, 0x223910, 0x223a88, 0x2245d0, 0x224a90, 0x224b18, 0x225028, 0x2275a8, 0x227478, 0x22b0f8, 0x22cbb0 | event player |
| 0x229120, 0x224548, 0x224418, 0x2293b8, 0x2292b0, 0x222a48, 0x222a88, 0x22cc88, 0x2290e8, 0x2244b0, 0x2242a0, 0x2241c8, 0x224030, 0x22cd90, 0x22c928, 0x22c970, 0x17e1c8, 0x17dd38, 0x226fb0, 0x222728, 0x222668, 0x2226d0, 0x222588, 0x2225e0, 0x222630, 0x2237b8, 0x2235c0, 0x224b98, 0x224fa8, 0x225330, 0x224e98, 0x226830, 0x225860, 0x226f50, 0x226fa0; data 0x37f208, 0x37f210, 0x37f218, 0x37f0c0, 0x37f0c4, 0x37f0c8, 0x37ec50, 0x356f40 | event player: actors, loads, motions, placements, counter, end (E6) |
| 0x137150, 0x1371d8, 0x1a7700, 0x1a7720 | music |
| 0x137300, 0x136ec0, 0x195358, 0x195208, 0x1951f8, 0x1953e0, 0x195260, 0x195288, 0x1952a0, 0x195300, 0x1d32c0, 0x1d32e0, 0x17fd10, 0x17ff60, 0x195130 | music slots, exit lock and its writers (E1c) |
| 0x13a7e8, 0x13a8a8, 0x13a980, 0x13a998, 0x13a9d0, 0x15d058, 0x15d1a8, 0x15cf58, 0x15d0e8, 0x15d248, 0x236650, 0x2368a8, 0x2369c0, 0x2369e0, 0x2378a0, 0x2379a8, 0x13ef00, 0x13ef48 | keyhole layout (E1c) |
| 0x1047a8, 0x149bb8, 0x140c00, 0x140e90, 0x13f470, 0x13efe8 | area start camera (E1c) |
| 0x194188, 0x1be828, 0x1d7970, 0x1a6ff0, 0x195778, 0x1d3a68, 0x1d30d8 | other opcode targets and area start |
| 0x226f50, 0x226fa0, 0x196348, 0x195ff8, 0x228b48, 0x11c550, 0x168d30, 0x22c9b0, 0x22c928, 0x22c970, 0x22ca18, 0x22c070, 0x22bff8, 0x224200, 0x227df0, 0x227d80, 0x2241c8, 0x226100, 0x2260b0, 0x222cc0 | event opcodes 0xd, 0xf, 0x10, the motion loop, load records 0x26 / 0x39 / 0x3e (E6d) |
| 0x195d28, 0x195e68, 0x195ef8, 0x195f68, 0x195fb8, 0x195ff8, 0x196138, 0x196170, 0x196320, 0x1963e0, 0x1966f8, 0x13f078, 0x14a710, 0x14a7c8, 0x14a7f0, 0x14a918..0x14a978, 0x2233d8, 0x2234e0, 0x2235c0, 0x2273d0, 0x223a88 | event object states and fades (E6e) |
| 0x11c550, 0x11c580, 0x11c5b0, 0x11c088, 0x11c1f0, 0x11c268, 0x11f390, 0x11f4a0, 0x120b48, 0x120fd0, 0x121178, 0x121278, 0x11eda0, 0x1205b8, 0x123be0, 0x123f68, 0x121a78 (mesh flag test), 0x1485a8, 0x192cd0, 0x192ce8; vtables 0x354b18, 0x354b98, 0x354c00 | opcode 0xd mesh groups (E6e) |
| 0x226e80, 0x1b7fa8, 0x1b8040, 0x19af68, 0x19af28, 0x19b100, 0x191c18, 0x16bc68, 0x16ed08, 0x170078, 0x16f8e0, 0x16f648; vtable 0x35b950 | opcode 0x4d (E6e) |
| 0x16e430, 0x16e638, 0x16e818, 0x16ef80, 0x16efd8, 0x229198, 0x1986c8, 0x1b3f78, 0x174fb0, 0x2a18a0, 0x2c4c58, 0x222a88, 0x223a88, 0x22c9b0, 0x22cd90, 0x224a90, 0x121a78, 0x11ee30, 0x11c5e0, 0x11c648; vtables 0x35ced8, 0x35eea8, 0x35f580, 0x35c9a8, 0x35c908, 0x35e300 (+0x3c); table 0x37a278 | opcode 0x4d tags, opcode 0x2c +6, opcode 0xd flag 2 (EV1) |
| 0x22ca18, 0x16a618, 0x16b960, 0x1beb00 | loop put-back (E6e) |

## Tutorial windows opened by mission scripts (WP-T1)

Host 1.296 (`FUN_001744a0`) opens a message window of class `FUN_001abba0` (class 1 of the window table: top centre, dark frame kind 4) with the message id on the stack; `msn/us/TT04_MS107.bar` event 3 calls it with 0x3ef7 at its first instruction. Details, the pointer command (0x0f) and the ids: docs/hud.md "Tutorial windows (WP-T1)". The port opens it on `struggle::MissionStart` of `TT04_MS107`. Host 1.284 (`FUN_00174430`) is the page-sequence object (0x2f46xx family) the same script uses for 0x3ef9: not ported.
