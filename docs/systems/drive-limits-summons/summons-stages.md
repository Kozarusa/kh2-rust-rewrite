# Summons: staged plan and status

[Contract](contract.md) | [Index](../../README.md)

## 6. Staged plan

Each stage ends with `verify.ps1` passing and the named check.

1. **Rules only.** The pure functions of section 5 (`drive.rs` rules) with unit tests on the real rows and
   an oracle run of FUN_001a0480 / FUN_001a0420 / FUN_00155bf0 / FUN_001d4e68 (the last needs a fake
   inventory). Accept: tests green; `KH2_SUMMONLOG=1`-style log not needed yet.
2. **Gauge and command without the object.** Summon and Dismiss in the menu and as harness keys; cost,
   mode 3 start after the `sstm[0x88]` wait, drain, experience, end at 0 and Dismiss; HUD mode 3 (digit and
   bar groups 139/140/141, digits 144..153). Accept: a harness run logs "summon start", the timer per
   second, "summon end (timer)"; a screenshot shows the mode-3 gauge (look at it before claiming it).
3. **The object, skip arrival.** Spawn Entity1 with its model, motion set and effect file at the
   200..300 spot; actor stop id 2 during the wait; effect 0x13 on the player; the summon's BDX attached and
   switched to `appear_skip`, then `leave` at the end (object removed when the script ends it). Needs the
   party-member class basics. Accept: log of mode switches, the object present for the timer's length,
   no crash when the script calls an unknown host (logged).
4. **Friend AI and group 10.** Port the friend AI engine (0x10b000..0x11b000) and the group 10 hosts the
   summon scripts call, one summon at a time (Stitch has the fewest unknown hosts after Tinker Bell).
   Accept per summon: its script's host calls all answered (trace shows none `missing`), it follows the
   player and attacks.
5. **Summon limits.** Script offers (host 1.165 slot 2), the limit task with the summon as participant,
   cost from the timer, the limit BAR script and the sub-commands as event 23. Depends on the limit task
   work (limit.rs gaps). Accept: the prompt appears with the gauge above 0 and disappears in a limit.
6. **Event arrival and the rest.** The config option and seen bits; `appear` with the link and HUD layers
   hidden; Genie's "Drive!"; Tinker Bell's reraise; area re-entry with `appear_area_start`; the summon face
   panel. Accept: logs per path; visuals only after someone looks at them.

### Status (WP-7J: stages 1 and 2)

Stage 1 is done. The rules are pure functions in `drive.rs` `rules`: `GaugeMode`, `SummonTuning`
(`sstm` 0x70, 0x84, 0x88), `summon_timer`, `mode_drain_factor`, `segments_drained` and
`FormTimer::change` (FUN_001a0358 with the segment count in mode 3), `summon_level_cap`, `SummonExp::add`
(FUN_001d4ec8), `summon_damage_to_timer` (FUN_001c60f8), `summon_gauge_change` and `summon_limit_cost`
(FUN_001788d8), `segment_display` (FUN_00155bf0), `summon_available` and `dismiss_available`. Tests:
`summon_rules_on_the_disc_rows`, `summon_availability_and_dismiss`, and the oracle
`summon_gauge_matches_the_game`, which runs the game's FUN_001a0480, FUN_001a0358 (with the experience
add and its level-up loop), FUN_001d4e68 (with the charms set in a fake inventory), FUN_00155bf0 and
FUN_001c60f8 in kh2-ee and compares every result. The level-up notice routines FUN_001d4fa0 and
FUN_001b5528 are stubbed in that run.

Stage 2 is done without the summon object. `summon.rs` keeps `SummonState` (bytes 5..7, the experience,
the state, the sequence bit, the arrival wait, the object slots, the use counters, the SUMMONS list, the
cap) and `SummonQueue`. The command menu (`items.rs`) lists the owned charms' commands under Summon,
shows Dismiss (0x13c) on the second deck while the state is set (the earlier "map command" reading of
0x13c in `items.rs` was wrong: FUN_001ceda8 looks Dismiss up by 0x13c), and issues both to the queue.
A summon pays its cost, sets the state to 2, waits `sstm[0x88]` frames, then sets gauge mode 3. Mode 3
drains with the Summon Boost factor, feeds the experience, takes hits Sora lands as `DriveDrain x
sstm[0x70]`, and ends at 0 or on Dismiss (gauge mode 1). The HUD's drive gauge shows mode 3 with the
FormB groups, the digit and the in-segment fill of FUN_00155bf0. Drive is refused while a summon runs.

Inferred in stage 2: the arrival wait counts down one frame unit per tick; the end-at-0 test runs only in
mode 3 (the game tests +0x1b4 in any mode, and during the arrival the player is busy through +0x108 bit
4, which is not modelled); the state goes back to 0 when the leave finds both object slots empty (the
game does it in the object's destructor). Left for later stages (WP-7J2 below took up most of them): the companions' fade out and return
(the arrival does not wait for FUN_0017d470), the player's motions and flags, effect 0x13, actor stop
id 2, the arrival event and its config option, Genie's form byte (FUN_001c6a88 divides a sign-extended
random word; not ported, the generator is not stepped), the summon face panel, the limit task test of
Dismiss (FUN_001b3c00 reads no task), and summon limits (limit.rs refuses summon rows until a summon
object exists, because FUN_00175ba0 needs it). The summon level and experience are kept in
`SummonState`, not written back to `Progress` (it has no setter for them).

### Status (WP-7J2: stage 3, with parts of stages 5 and 6)

The arrival thread was decompiled (0x1c6260, ghidra-cli `DecompileAt.java`) together with the Summon case of
FUN_001cf030; section 3 matches it, with these details: the wait loop runs while the fade list is busy, while
`0 < wait` (the wait loses the frame unit `DAT_00356f40` per frame) or while `FUN_0017d470` finds a party slot
filled; motion 0x8a takes its blend from `DAT_0037c358`, motion 0x82 has blend 0; the mode names are the strings
at 0x37c000 (`appear_skip`) and 0x37bff8 (`appear`); FUN_001a5ea8 gets flags 0x10000 and the player's position.

What `summon.rs` now does, in the order of the game:

- Summon: motion 0x8a on Sora (`State::Reaction` until it ends), system effect 0x13 bound to his body
  (`fx::start_placed`, the handle kept as +0xb68), the cost, byte 5, the sequence bit, state 2, byte 7 (Genie:
  `rules::genie_form`, the port of FUN_001c6a88; Peter Pan 1), the player's +0x108 |= 0x104 (0x100: no hit
  reaches Sora while it is set), the arrival choice (`rules::arrival_event`, FUN_001c65f0, with the option
  word's bits 9..10 and the seen bits), the companions' fade-out over `DAT_0037bff0` frames (component `Away`),
  actor stop id 2 (below) and the use counter.
- The arrival: the wait (`sstm[0x84]` with the event, `sstm[0x88]` without) and the companions gone, then +0x108
  |= 1, gauge mode 3 and the summon object (`sumn` `Entity1`: objentry row, model, motion set, script; component
  `SummonObject` in the first free of the two slots). Without the event: effect 0x13 told to end, motion 0x82,
  the free spot (`rules::spot_start` = FUN_001a6068, oracle-checked; `rules::spot_find` = FUN_001a6198 with a
  floor under the point standing in for FUN_001a5c30 and no test for FUN_001a5bf8), script mode `appear_skip`.
  With the event: the effect killed, mode `appear`, state 1. Then state 2, +0x108 &= ~0x105, the stop lifted.
- The object: its script runs every tick through the enemy host (as the party's in `friend_ai.rs`); its motion
  requests are logged (`KH2_SUMMONLOG=1`) and not played (the friend AI's motion mapping is stage 4); the body
  loops motion 0. A hide by the script removes it (the destructor empties its slot; with both slots empty the
  state is 0). `SummonState::present` lists the objects' NeoStatus.
- Leave: the first object's script switches to `leave`; once both slots are empty the companions fade back in
  over `DAT_0037c064` frames to `DAT_0037c068`, and the sequence bit is cleared.
- Summon limits (stage 5, the rule side): limit.rs refuses a summon row unless its summon object is present
  (FUN_00175ba0 by NeoStatus) and the timer is above 0, and charges `Cost x sstm[0x70]` to the timer
  (`SummonRequest::Gauge`, FUN_001788d8 with the experience). The limit task is the resource `LimitTask`
  (`DAT_01d5d6b0`); Dismiss is refused while it exists (FUN_001b3c00).
- The summon level and experience are written back to `Progress` (`set_summon_level`, `set_summon_exp`).
- Actor stop id 2 during the arrival: the enemies (object types in the mask 0xfff7ffdc) stop their motion
  (`Animator::speed` 0) and no hit reaches them.

Tests: `summon_arrival_choice_follows_the_option_and_the_seen_bits`, `summon_genie_form_picks_an_owned_form`,
`summon_consts_and_spot_on_a_flat_floor`, the oracles `summon_spot_start_and_genie_form_match_the_game`
(FUN_001a6068 and FUN_001c6a88 with the form items in a fake inventory), `summon_spot_trig_is_sin_and_cos`
(FUN_00300278, FUN_003000d8) and `summon_arrival_choice_matches_the_game` (FUN_001c65f0), and limit.rs's
`summon_limits_need_the_gauge_not_a_partner`.

Not done, and why:

- (Closed in WP-F1 item 3.) The enemies' scripts now get no tick while `SummonState::stop` is set: `enemy_ai::ai_update`
  skips `Runtime::update` for every scripted enemy (the game's actor stop id 2 leaves the actor's update out, so its
  script thread does not advance), logging `bdx: enemy scripts paused (summon arrival actor stop id 2, 0x35fec0 bit 3)`
  once and `... resume` when the stop lifts; the trace of each enemy gets `summon arrival stop: no script tick`.
- (Closed in WP-F1 item 4.) The companions come back at FUN_0017d290's spot: `PartyMember::place` (party.rs) puts
  the member at the free spot the summon's own search finds (`spot_start` / `spot_find`, FUN_001a5ea8, with
  `ret_spot` 200..300 units from a point `ret_lift` above the player; the collision stand-ins of the summon's
  spot apply) facing the player (FUN_003003e8 -> FUN_0010be80), then fades in. Log: `summon: companions: <model>
  comes back at .. (N units from the player, FUN_0017d290: ..), facing him, fading in over 15 frames`. Seen in
  tt02 (`KH2_STATE` with item 25, `KH2_DRIVE_POINTS=5 KH2_SUMMON=3 KH2_SUMMON_AT=4 KH2_DISMISS_AT=12`): both members
  come back at the same spot 200 units from the player: the search runs once per member in the same frame and
  the first member placed is not an obstacle to the second [inferred; whether the game's FUN_001a5ea8 sees the
  just-created member was not read].
- The event that the `appear` mode and the thread wait for is never started (section 7), so the wait ends at once;
  the HUD layers 7..10 are not hidden. The fade list (FUN_0016ff50, FUN_0016f8e0, FUN_0016fb20) is not modelled.
- The free-spot collision tests FUN_001a5c30 and FUN_001a5bf8 were not read: a floor under the point (`World::ground`)
  stands in for the first, the second always passes [inferred].
- No friend AI, no motion requests (stage 4; WP-SUM4 below answers Stitch's group 10 hosts); summon limits are not offered by the scripts
  (host 1.165) and the limit task body is not run (stage 5); the summon face panel, Genie's "Drive!", Tinker
  Bell's reraise and the area re-entry (stage 6).
- Byte 5 and byte 7 are kept in `SummonState`, not written to `Progress` (no setters for them).
- An area change during a summon (FUN_00168ba8 -> FUN_001c68f8 clears the slots and the state, FUN_001c6710
  creates the summon again in the new area) is not handled: the object is not an area part and stays where it was.
- The game steps one LCG (`DAT_003716a0`) for many systems; the app keeps a copy for the summon only, started
  from the executable's initial word, which is 0 in the file. A multiplicative LCG at 0 stays 0, so the game
  must seed it somewhere [GAP: none of the 34 functions `mips_scan.py --addr 0x3716a0` lists stores anything but
  the next step; a store through a computed base or an overlay was not searched]. With 0 the port gives Genie
  the first owned form and the free spot its minimum radius.
- System effect 0x13 is element 19 of `00effect.bar` (`kh2 pax 00effect.bar`: effect 18, bone 67, flags 0x805,
  sound 180). WP-FX13 status: it starts and draws (`KH2_FXLOG=1`: `summon: system effect 0x13 (00effect.bar) bound
  to the player: started`, then `fx 19 objs 9 batches 7 verts 1176` per frame); the earlier "Effect::new refuses
  its particle program" was the app's DPX place lookup, fixed by WP-7Bc (docs/effects-damage.md "The summon
  arrival effect 0x13"). The `why` string in `summon.rs` that names `Effect::new` is stale.
- The free spot's direction is (sin, 0, cos) of the angle and the LCG word is read unsigned (both oracle-checked).

Harness (the runs behind the status, tt02, Donald and Goofy spawned by the map script):
`KH2_STATE=<file with "items": {"25": 1}, "summon": {"level": 1, "exp": 0}, "option_word": 11>` (11 = the default
option word with Summon Effects 0, so the arrival skips the event) `KH2_DRIVE_POINTS=5 KH2_SUMMON=3 KH2_SUMMON_AT=4`,
plus `KH2_DISMISS_AT=12` (Dismiss) or `KH2_SUMMON_AT=2 KH2_LIMIT=519 KH2_DISMISS_AT=4.5` (Ohana with Stitch out:
`limit: summon limit cost 100 x 12 = -1200 to the summon timer`, `summon: Dismiss refused (summon state 2, limit
task true)`). Without `option_word` the default Auto plays the event path on the first summon.

### Status (WP-SUM4: stage 4 for Stitch, the group 10 hosts)

Stitch's script (`N_HB040_BTL`, `kh2 bdx N_HB040_BTL`) calls 88 distinct host functions; its group 10 entries are
10.1, 10.3, 10.4, 10.12, 10.13, 10.16, 10.19, 10.32, 10.39, 10.41 and 10.45. All eleven now have a name and a
signature in `kh2-bdx` `names.rs` and docs/bdx.md (checked against the executable's table at 0x371810 by
`signatures_match_the_executable`), so the runtime decodes their arguments (the AI calls take the script's
context, whose word +4 is the object; 10.1 a vector). Every wrapper that names the AI reads object +0xb60
(FUN_0010c5f0), the AI object FUN_0010b780 made.

The summon object's script now runs through `summon::SummonHost`: the party members' host (`FriendHost`:
attack objects 2.10 / 2.11 / 2.12 / 2.13 / 2.22, effects, the target record) with the summon's own entries in
front of it. The AI object's fields the hosts reach are `summon::ai::SummonAi` (one field per game field, each
with its offset). Motion requests still go to the enemy host unremapped and are logged, not played.

Host calls of Stitch's script, as the harness run reaches them (`KH2_BDXLOG=1 KH2_SUMMONLOG=1 KH2_STATE=<items
{"25": 1}, summon level 1, option_word 11> KH2_DRIVE_POINTS=5 KH2_SUMMON=3 KH2_SUMMON_AT=4 kh2-game tt02`; the
`appear_skip` mode, the main thread and the battle start). No call of `N_HB040_BTL#summon0` is `missing` (before
this work package: 1.84, 1.272, 2.10, 2.22, 2.74, 10.1, 10.12, 10.19, 10.32, 10.45 were):

| host | class | what the app does |
|---|---|---|
| 1.84 gauges on the player | Impl | the player's status block handle (his +0x48c) |
| 10.45 status_f_1b4 (0x2f73b0) | Impl | the float +0x1b4 of that block: `DriveState::timer.left` (the mode-3 timer) |
| 10.19 in_limit (0x2f6e60) | Approx | object +0x10c bit 0x10000 = a limit task runs (`LimitTask`) [inferred: while a summon is out only summon limits can run, FUN_001d1ce0, and the limit task marks its participants, FUN_001b2c10] |
| 1.272 flag_7f8_4 (0x171cb0) | Impl | (object +0x7f8 & 4) != 0; set only by FUN_00186580 (callers FUN_001865a8 = host wrapper 0x171cd0, FUN_00175f10, FUN_001b25d0), never for the summon in the app |
| 1.165 offer_command | Stub | Ohana (519, slot 2) is offered every frame while the timer is above 0; summon limit offers are stage 5, the menu does not show it |
| 10.12 ai_slot_start (0x2f6d00 -> FUN_00117708) | Impl | the AI's 0x18-slot list at +0x20 (FUN_00117668 takes the first slot with kind < 0 and no effect) |
| 10.1 ai_walk_to (0x2f6a98 -> FUN_001175a0) | Approx | AI +0x630 bit 0 and the goal +0x640 kept; FUN_00117320, FUN_00193228, FUN_0017ecf0 (the class's motion toward it) not ported |
| 10.32 ai_stitch_32 (0x2f7138 -> FUN_00118aa8) | Stub | a goal (FUN_00116780), motion request 0xa1 and a record's action (FUN_00114280 -> FUN_0010ecb0): the class update is not ported |
| 2.10, 2.11, 2.12 | Impl | (the party host) Stitch's body attack object: `atkp` row, size, position each frame |
| 2.22 | Approx | (the party host) the anchor is the object's own effect file |
| 2.74 attack_callback_98 | Impl | the routine and argument kept; FUN_0018d430 (a clash of two attacks) is its only caller and never happens for the summon |
| 1.13, 1.11, 1.94, 1.262 | Approx | as for enemies (enemy host) |

Not reached in the harness (classed as the host would answer them): 10.3 / 10.4 (Stitch's vtable +0x28
FUN_001191a8 / +0x2c FUN_00119360: the player and his hand objects linked to Stitch for Ohana, stage 5) Stub;
10.13 (FUN_001195e8, event 17 code 1000: AI slots of kind 1 and attack objects of `atkp` 0x373 around the target,
stage 5) Stub; 10.16 (FUN_00117840: slot flags |= 0x40000000) Impl; 10.39 (FUN_0010ecb0(object, 0, 0)) Impl;
10.41 (FUN_001129f0: object +0xb7c |= 0x40000000) Impl. The battle modes (`mode_battle`, `btl_*`) never run:
the native AI starts them.

Tests: `stitch_summon_script_offers_ohana_while_the_timer_runs` (kh2-bdx disc: the script in `appear_skip` on the
mock world decodes every group 10 call, asks for AI slots, offers Ohana only with the timer above 0 and no
limit), `summon_ai_slots_take_the_first_free_one` and the oracle `summon_ai_slot_start_matches_the_game`
(FUN_00117708 with FUN_00117668 in kh2-ee over 200 random slot lists), the interpreter oracle with
`N_HB040_BTL` added (`cargo test -p kh2-bdx --test oracle`: 0 failures).

Not done: Stitch neither follows the player nor attacks. Both are native: Stitch's AI class FUN_001179c0 (vtable
0x354870) binds 7 action records at 0x354708 (stride 0x24) whose handlers are Stitch's own, FUN_00117cf8 (stand:
the wait +0x70..+0x74, motion request `DAT_00378870`), FUN_00117ea0 (a goal from FUN_00116780 and motion request
`DAT_00378874` / 0xa1, facing FUN_00116658), FUN_00118348 and FUN_00118730 (AI slots through FUN_00117708,
motions through FUN_0017e220 / FUN_0017e378, FUN_00118730 also an effect FUN_0016a570), FUN_00118db8 (motion 0xc9
and the player linked with 0xfe), chosen by the friend engine's record choice (FUN_0010fcd0, ported for Donald
and Goofy in `party_battle.rs`); the class update FUN_00116b48 (531 lines: the slot list, effects, FUN_0016aa18)
runs the slots and the goal. None of these were ported [GAP: read only as far as the calls listed here; the
summon object has no locomotion of its own in the app (party.rs owns the members' movement)]. So no hit by
Stitch's attacks is logged. (WP-SUM4b below ports them.)

### Status (WP-SUM4b: Stitch moves and attacks)

**What drives Stitch.** FUN_0010b780 gives NeoStatus 200 the class 0xe (+0xb6c), the AI object FUN_001179c0 and,
at object +0xb64, the update FUN_001155e0 in place of the members' FUN_00111070. FUN_001155e0 never calls the
members' follow (FUN_00110468, FUN_00110f90, FUN_0010f8d8): Stitch does not follow on the ground. Each frame,
while the object is free (+0x160 bits 4/8/0x10 or 0x40000000), it

1. places the AI once at the zero vector if +0x630 bit 0 is clear (FUN_001175a0; the script's 10.1 does it first),
2. wraps the position and the goal across the plane's edges (FUN_00116530) and stops the direction (+0x660 = 0),
3. in battle (FUN_001903d8), without a limit task (FUN_001b3c00), when the choices counted at +0x788 exceed the
   entry's +0x10, resets the count and with the entry's chance (+0x14) forces a record: the MP refill
   (FUN_00118730) when the player's MP is the lower fraction (`mp x maxhp < hp x maxmp`, the status block's +0x180,
   +4, +0, +0x184), else the blast (FUN_00118348); the record runs its phase 0 at once (`_PTR_FUN_003787a0` /
   `_PTR_FUN_003787a8` found by FUN_00114280);
4. otherwise runs the friend engine's choice FUN_0010fcd0 over the seven records and counts a change of record
   (or a record that is not continuing) at +0x788;
5. turns the yaw +0x64c toward +0x65c by +0x62c (FUN_0017bc30) and adds direction x speed (+0x660 x +0x66c) to
   the position +0x640;
6. every entry +4 frames (+0x674) calls the player's vtable +0x78 with (1, 0, 0) while his HP is below its maximum
   [GAP: the player's method at vtable +0x78 was not read; not ported].

The entry is the class block's (0x34ea00 + offset[0xe]) parameter entry with id 0x80 (FUN_00114228 over the list at
block +0xb0 / +0xb4), kept at AI +0x14. FUN_001179c0 sets +0x628 (`DAT_0037885c`, speed cap and arrival distance)
and +0x62c (`DAT_00378860`, turn per frame and arrival angle).

**The plane.** The position +0x640 is (x, y, depth, yaw): FUN_001175a0 and FUN_00116780 hold the depth at
`DAT_00378840` / `DAT_003787d0`; FUN_00116530 moves x by `DAT_003787bc` past `DAT_003787b8` / `DAT_003787c0` and y
by `DAT_003787c8` past `DAT_003787c4` / `DAT_003787cc`, position and goal together. FUN_00117320 makes a matrix of
it (rotation by the yaw less the motion's root rotation from FUN_0017ecf0, translation `+0x640 x DAT_0037882c`) at
AI +0x6c0, and FUN_00193228 gives the object that matrix as its parent (+0x790, +0x10c |= 0x40, +0x768 |= 0x1000);
with +0x10c bit 0x40 the object's movement FUN_00181e00 adds no gravity and no collision. [inferred] The parent is
the camera: the constant depth, the screen-sized wrap, the class update's placement type 3 through the camera's
inverse view (FUN_00116b48 with FUN_001068c0(1)) and the blast aimed from the camera's position all point there;
who reads object +0x790 was not found [GAP: `mips_scan.py --load-off 0x790` finds no load, the decompilation has
only the writers FUN_00193228 / FUN_00193248; +0x768 bit 0x1000 has no reader by the patterns `0x768) & 0x1000`,
`0x768) >> 0xc`]. The app puts the object at the camera-space point (x, y, depth) x `DAT_0037882c` (game camera
axes: x right, y down, z forward) through the game camera's transform, turned by the yaw about the camera's
vertical axis; the root rotation is taken as 0. So Stitch moves after Sora because the camera does.

**The records** (0x354708, stride 0x24; handler at +4, flags at +0):

| # | flags | handler | phase 1 (test) | phase 0 |
|---|---|---|---|---|
| 0 | 0x201 | FUN_00117cf8 stand | no limit task, +0x10c bit 0x800 clear, no flag-2 record feasible (FUN_0010ed20); continuing: the wait +0xcd4 above 0 | new: the wait from block +0x70..+0x74 (FUN_0010f648; in battle `2w + DAT_00378864`; yaw beyond `DAT_00378868`: times `DAT_0037886c`); motion 0 (FUN_0017e2d8 with `DAT_00378870`), stopped |
| 1 | 0x201 | FUN_00117ea0 move | as 0; new: not the current record (FUN_0010f7a8); continuing: not arrived (FUN_00116aa0) | new: a random goal (FUN_00116780(-1)) and motion 0xa1; the step toward the goal (direction, speed = distance x `DAT_00356f48` capped at +0x628), the facing (FUN_00116658) |
| 2 | 0x212 | FUN_00118348 blast | gates, +0x160 bit 0x40000000 clear; new: FUN_001180c0 (player HP below entry +8 of its maximum, FUN_0010f490, and an enemy in view, FUN_00118068); continuing: phase +0x634 not 4 | new: phase 0, goal case 6; phase 0: step and face, nearer than `DAT_00378878` -> wanted yaw 0, phase 1; phase 1: step, arrived -> phase 2; phase 2: snap to the goal, motion 0xcb, AI slots 0xc and 0xd (argument 0x13, flag 0x10), phase 3; phase 3: while motion 0xcb plays, its end -> phase 4; phases 2 and 3 run FUN_00118110 |
| 3 | 0x220 | FUN_00118730 MP refill | gates; new: player MP below entry +0xc of its maximum (FUN_0010f4d0); continuing: phase not 3 | as the blast with goal case 7 and `DAT_0037887c`; phase 2: motion 0xcc, FUN_001a0240 on the player's status (MP to its maximum, a charge ended), effect 10 on the player (FUN_0016a570), AI slot 0xb |
| 4 | 0x212 | FUN_00118db8 | never new; continuing while motion 0xc9 plays | motion 0xc9, the player linked with motion 0xfe (FUN_0017f160, FUN_0017e390) [not reached] |
| 5, 6 | 0x212 | FUN_00118ec0, FUN_00119068 | not read [GAP] | not read (both call FUN_001175a0 with the vectors at 0x3b4050 / 0x3b4060) |

FUN_00118110 (the blast): an attack object (FUN_0018bef0 with `atkp` 0x4ba, owner Stitch) kept at +0x620 with the
motion at +0x624; every object the target walk (FUN_0018f348) gives that is a valid enemy (FUN_00112b28 ->
FUN_00112ac0), not out of view (FUN_0019ada8) and has a hurt shape (FUN_00112d20, the nearest) is hit once
(FUN_0018c5f8, FUN_0018c7f0, the target's vtable +0x64) with the direction from the camera's position
(FUN_001071c8). The class update FUN_00116b48 frees the attack object (FUN_0018dc98) when the motion is no longer
+0x624 or it ended (+0x160 bit 2).

**Motions.** FUN_0017da30 uses the four-slot order only for a motion set whose header +0xc has bit 0 (FUN_0017d960 ->
FUN_001a7d70); otherwise the number is the slot. N_HB040_BTL's set has slots 0, 161, 201..206, 231, 241, 242: the
requests 0, 0xa1, 0xcb, 0xcc play at their own numbers. [inferred: the header bit is not read; a slot at the number
wins, the order is the fallback.]

**In the app** (`summon.rs` module `stitch`, run from `objects` for NeoStatus 200 while the script is not in
`leave`): `Consts::from_exe` (every address above), `records`, `goal` (FUN_00116780), `wrap` (FUN_00116530),
`toward`, `arrived` (FUN_00116aa0), `face` (FUN_00116658), `place` (FUN_001175a0), `can` / `run` (the records'
phases), `update` (FUN_001155e0 with `party_battle::choose` for FUN_0010fcd0). Hosts: 10.1 Approx (the root rotation
taken as 0), 10.32 Approx (FUN_00118aa8: its FUN_00116780 case is a register the wrapper leaves, taken as the random
branch), 10.39 also clears the current record. The blast writes `combat::Hit { atkp: 0x4ba }` for each enemy whose
nearest hurt volume projects inside the screen; the refill calls `magic::refill_mp`.

Inferred or left out:

- [inferred] The generator FUN_00106308 behind FUN_0010f420 / FUN_0010f438 / FUN_00106408 is party.rs's xorshift
  stand-in (one per summon object), not the game's.
- [inferred] The stand wait +0xcd4 counts down one frame unit per frame (as party.rs).
- [inferred] When FUN_0010fcd0 chooses nothing, the members' FUN_001103f8 runs; for Stitch it was not read: he
  stands.
- [inferred] The two-argument calls of FUN_001166d0 / FUN_00116728 in FUN_00116780's random branch write y too (the
  third argument register still holds +0x654).
- [inferred] "Out of view" (FUN_001a5db0 -> FUN_00146648, a frustum test of the object's +0x740 block) is the hurt
  volume's centre projecting inside the screen; "free" (+0x160) is always true; +0x10c bit 0x800 and +0x160 bit
  0x40000000 are never set.
- The AI slots' effects (FUN_00116b48 starts `FUN_0016a530(object, kind, ..)` and places them by type 1..3), the
  refill's effect 10 on the player and the 0x674 call are logged, not done [GAP].
- The attacker's strength for the blast: `enemy.rs` takes Sora's stats for an attacker that is not a party member
  (the summon's status block is the open gap above); the harness shows damage 0 with `atkp` 1210 and a blow.

Harness (tt04, the Dusk group `b_44`, `KH2_STATE` with item 25 / summon level 1 / option word 11,
`KH2_DRIVE_POINTS=5 KH2_SUMMON=3 KH2_SUMMON_AT=7 KH2_SUMMON_PARTY=1 KH2_SUMMONLOG=1 KH2_HITLOG=1`): `summon:
N_HB040_BTL#summon0 record 2 (handler 0x118348, FUN_001155e0's forced pick) starts`, `... blast (FUN_00118110,
atkp 0x4ba) hits <enemy>`, then `enemy: Dusk hit: atkp 1210 damage 0 ... Blow`; the once-a-second `plane (x, y,
1024) ... -> world ... (N units from the player)` line shows the plane position and the world point following the
camera. (`KH2_SUMMON_AT=4` there is refused: the player is airborne at 4 s.)

Tests: `stitch_follow_decision_and_blast` (the choice out of battle is stand or move, a move ends at its goal, the
plane wraps, the blast is the only choice with low HP and an enemy in view and fires from its spot until its motion
ends, the forced pick takes the refill when MP is the lower fraction), `stitch_class_from_the_executable`, and the
oracle `stitch_plane_rules_match_the_game` (FUN_00116530, FUN_00116aa0, FUN_00116658 and FUN_00116780 cases 0..7
in kh2-ee over 300 random rounds).

### Status (WP-SUM5: the next summon, Genie)

**A. The measure.** The SUM4b harness (tt04, `KH2_WAVE_GROUP=b_44`, `KH2_STATE` with the summon's charm / summon level 1 /
option word 11, `KH2_DRIVE_POINTS=5 KH2_SUMMON=<row> KH2_SUMMON_AT=7 KH2_SUMMON_PARTY=1 KH2_BDXLOG=1 KH2_SUMMONLOG=1`, 22 s;
charms: row 1 item 383, row 2 item 159, row 4 item 160) on the base of this work package. "Reached" is what each summon
object's script asked of the host in that run (the `first host call` lines of its tag, the `unimplemented host call` lines
for `missing`); "listing" counts the distinct host functions of the whole script (`kh2 bdx <model>`) and those without a
name in `names.rs`.

| summon | script | reached in the run (class) | listing: distinct / unnamed | unnamed group 10 |
|---|---|---|---|---|
| Chicken Little (1) | P_EX350 | 1.13 Approx, 1.87 Approx, 2.10 Impl, **2.21 missing**; then `script error BadAddress(4)` at 0x1638 and 0x1268 (the thread stops) | 107 / 24 | 10.5, 10.8, 10.17, 10.18, 10.20, 10.29, 10.31, 10.33, 10.34, 10.36, 10.37, 10.51, 10.59 |
| Genie (2) | P_AL010 | 1.13 Approx, 1.84 Impl, 1.94 Approx, 1.165 Stub, 1.272 Impl, 10.19 Approx, 10.45 Impl, **10.10 missing** (x2040, twice a frame) | 81 / 12 | 10.9, 10.10 |
| Peter Pan (4) | P_EX330 | 0.49 Approx, 0.51 Impl, 1.13 Approx, 1.21 Approx, 1.84 Impl, 1.87 Approx, 1.94 Approx, 1.165 Stub, 1.272 Impl, 10.19 Approx, 10.45 Impl, **2.75 missing** | 133 / 34 (Tinker Bell P_EX340: 37 / 1) | 10.17, 10.18, 10.20, 10.23, 10.25, 10.26, 10.27, 10.28, 10.30, 10.31, 10.36, 10.37 |

Chosen: **Genie**. It reaches one missing host (as the others), its whole script has the fewest distinct and unnamed host
functions (81 / 12 against 107 / 24 and 133 / 34) and only two unnamed group 10 entries (10.9, 10.10), and its script
runs without an error (Chicken Little's stops on a bad address after the missing 2.21).

**B. Genie's class** (`summon.rs` module `genie`, run from `objects` for NeoStatus 201 while the script is not in
`leave`).

- FUN_0010b780 gives NeoStatus 201 the class 0xf, the AI object FUN_001199f8 (0x18 bytes: FUN_001133c0, then +0x10 =
  0x3548a8, vtable 0x354910, +0xc = 2 records) and +0xb7c |= 0x800, but keeps the members' update (+0xb64 =
  `PTR_FUN_00378488` = FUN_00111070). So Genie runs the same decision loop as Donald and Goofy: out of battle
  FUN_001102c8 (the player relation), in battle FUN_0010f830 (summon class: only the player can be in danger),
  FUN_0010f8d8 (the emergency pass over flag 0x10 records) or FUN_0010fcd0, then the steer FUN_00110468. The app
  runs party_rules' follow point (FUN_00111670, party slot 0), follow step (row of the battle state) and steer with
  the class 0xf thresholds and the `prty` row of Genie's NeoMoveset (walk +0x110, run +0x114, turn +0x11c).
- The records (0x3548a8, stride 0x24): 0 = flags 0x201, FUN_00119af8 (always feasible; phase 0 FUN_001102c8, whose
  class 0xf branch raises a speed above the walk speed to the player's movement +0x860 plus `DAT_00378574` when
  that is faster); 1 = flags 0x10, FUN_00119b28 (feasible while the player's HP is below `DAT_003788d0` of its
  maximum, FUN_0010f490, which also targets the player; phase 0 FUN_0010c9d0(object, 1, player, 0): speed mode 0,
  object +0xb2c = the command of item 1 (FUN_001a1fc0(1): FUN_001a66e8 finds item 1 in 03system.bin `item`,
  loaded by FUN_00193610 -> FUN_001a66b8, its +0x10 is the command), +0xb30 = the target, motion request 9 through
  FUN_00175e20).
- The form: the first update (FUN_001112e8, +0xb7c bit 0x20000000 clear) calls the object's vtable +0x94
  (FUN_0010c630, at 0x352e1c and 0x35feac), which calls the AI's +0x18: Genie's FUN_00119a40 = FUN_00119c18 with
  FUN_001c6b28's command (byte 7 through the table at 0x37c036). FUN_00119c18 (also host 10.9 from the event 11
  handler with the "Drive!" command): AI +0x14 = the command, parts 2..6 hidden and part row + 2 shown (FUN_00192ce8
  / FUN_00192cd0 -> object +0x670's vtable +0x40 / +0x3c), byte 7 = row + 1 (6 when the command is not in the table).
  Host 10.10 (FUN_00119ca8) gives the limit command paired with AI +0x14; the main thread (event 1) offers it every
  frame in slot 2 through 1.165 while the timer is above 0 (as Stitch's Ohana).

Hosts reached by `P_AL010#summon0` in the harness (`KH2_STATE` with item 159 / level 1 / option word 11,
`KH2_DRIVE_POINTS=5 KH2_SUMMON=2 KH2_SUMMON_AT=7 KH2_SUMMON_PARTY=1 KH2_BDXLOG=1 KH2_SUMMONLOG=1`, tt04 `b_44`):
1.13 Approx, 1.84 Impl, 1.94 Approx, 1.165 Stub (summon limit offers: stage 5), 1.272 Impl, 10.10 Impl, 10.19 Approx,
10.45 Impl: none `missing`. 10.9 (event 11, Genie's "Drive!" commands) was not reached in that run (the menu entry came with WP-FX12b,
see "Genie's Drive!" below); it is Approx (the parts are not drawn).

Log lines of that run: `summon: P_AL010#summon0 first update (FUN_00119a40): byte 7 = 0 -> form command 0`, `... form
command 0 (FUN_00119c18): parts [2, 3, 4, 5, 6] hidden, part None shown (not drawn), byte 7 = 6, limit command 0`,
`... record 0 (handler 0x119af8) starts`, and once a second `... follow: N units from the player, speed mode m speed
s (walk 2.0 run 8.0)`: with Sora walking (`KH2_KEYS=12:W:3`) the distance rose to 356 with mode 2 (run 8.0), then
fell back to 270 and Genie stopped (mode 0). With no drive form owned (the harness state) byte 7 is 0, so the first
update shows no form part and 10.10 gives 0 (the script offers command 0).

Inferred or left out:

- [inferred] Genie flies (FUN_002f6320 picks his motions by NeoStatus): the app keeps him at the player's
  height and does no ground probe. WP-SM6 found the reader of +0xb7c bit 0x800: it is the chooser FUN_0010fcd0, not a flight
  test (see "WP-SM6" below); the flight itself rests on the flying motion table only [inferred].
- His motions: only record 1's motion 9 is played; the follow's motions (FUN_002f6320's flying set) are not ported,
  so the body keeps the idle slot the spawn chose (slot 1 in the harness; the screenshot shows him with his arms
  spread) [GAP].
- The model parts of FUN_00119c18 (object +0x670, vtable +0x40 / +0x3c with +0x104) are logged, not drawn [GAP: the
  part object at +0x670 was not read].
- Record 1's command (item 1's) is not carried out: the item use of the members (motion 9's trigger, party.rs) is not
  run for a summon; the log says `the command's effect is not carried out` [GAP]. Genie is not free until motion 9
  ends [inferred, as the members' item use].
- [inferred] When FUN_0010fcd0 chooses nothing the follow runs (FUN_001103f8's player relation).
- [inferred] The player's movement +0x860 is the length of Sora's horizontal velocity per tick (`Motor::vel`).
- The generator is party.rs's xorshift stand-in (`stitch::Rng`), as for Stitch.
- The fifth table pair (607, 608) has no byte 7 command in the table at 0x37c036 (it reads 0 there).

**C. Tests.** `genie_class_from_the_executable_and_its_decision` (the records, the form table both ways, the decision
out of battle, healthy in battle, in danger through the emergency pass, the class 0xf speed rule); the oracle
`genie_form_and_limit_match_the_game` (FUN_00119ca8, FUN_00119c18 with its part calls logged by stubs at FUN_00192ce8
/ FUN_00192cd0 and the game's FUN_001c6b48 writing byte 7, FUN_001c6b10, and FUN_0010f490 over 300 random rounds, in
kh2-ee); kh2-bdx `genie_summon_script_offers_the_form_limit_while_the_timer_runs` (P_AL010 in `appear_skip` on the
mock host: no group 10 call without a signature, 10.10 asked every frame, the limit 544 of the form command 536
offered only with the timer above 0 and no limit task); `P_AL010` in the interpreter oracle (0 failures).

### Status (WP-SUM6 / WP-SUM7: Chicken Little and Peter Pan run without a missing host)

Measured with the SUM5 harness (`KH2_STATE` = items 160 and 383, summon level 1, option word 11; `KH2_WAVE_GROUP=b_44 KH2_DRIVE_POINTS=5
KH2_SUMMON=<1|4> KH2_SUMMON_AT=7 KH2_SUMMON_PARTY=1 KH2_BDXLOG=1 KH2_SUMMONLOG=1 kh2-game tt04`, 20 s): no `P_EX350` or `P_EX330` line
among the `unimplemented host call` / `script error` warnings (the end-of-run table's only `Missing` rows, 1.161 and 1.83, are Goofy's
`P_EX030`).

- **Chicken Little**: the `BadAddress(4)` at 0x1638 / 0x1268 was the missing answer of host 2.21 (SUM6, FUN_001a5040 keyed buffers,
  `kh2_bdx::shared`: the script stores through the returned pointer). Test `disc.rs` (the with / without buffer pair, SUM6) reproduces
  it; the run now reaches 2.21 Impl, 2.10, 1.87, 1.13, 1.272, 10.19, 1.84, 10.45, 1.165 and stays running.
- **Peter Pan** (`P_EX330#summon0`, `appear_skip`): the two hosts left were **1.155** and **0.40**.
  - 1.155 (`obj_method_78`, wrapper 0x171c48: the receiver's virtual method +0x78 with (amount, part, flag != 0), result into word 0). The
    receiver here is the player (Obj 0x2000000), the arguments (5, 0, 0) follow the effect Tinker Bell's script starts on him (1.149). The
    method of the player is FUN_00186238 (docs/magic.md): with the +0x7f8 bit 4 clear it adds the amount to part 0's HP and holds it between
    the lowest and the maximum (FUN_001a0100), posts the panel's heal event (FUN_0013de10) and runs the death method only at 0 HP. App:
    `SummonHost::own` adds the amount with `magic_rules::healed` to `SummonAi::player_hp`, `objects` writes Sora's HP back after the
    script ran (log `summon: P_EX330#summon0 healed the player (host 1.155): HP 17 -> 20`, Sora HP 20/20 in the screenshot) and the call returns the new HP. Approx: the
    revive branch (bit 4 set, FUN_0017cee8), the panel event and the face are not run; a negative amount is a Stub (no summon script
    asks for it [inferred: only this call site was reached]).
  - 0.40 (wrapper 0x16db10, no `names.rs` row before): copies the vector argument raw into the 16 bytes behind FUN_001d9d50(handle), the
    effect instance's second vector, the same place 0.41 (0x16db50) fills with the angles of a direction (FUN_0017be00). The script calls
    it every tick after 0.38 on the effect it started with 1.21. App: the vector is taken as those angles (pitch, yaw) and handed to the
    friend host's 0.41 as the matching direction, which sets the element's rotation [inferred; the roll is dropped; what the instance
    reads the vector for was not traced].
  - Tests: kh2-bdx `peter_pan_script_heals_the_player_and_sets_the_effect_vector` (both calls decode, 1.155 gets (object, 5, 0, 0),
    0.40 a vector, no `Unknown`, no script error), `peter_pan_script_asks_whether_tinker_bell_is_ready` (SUM6); the interpreter oracle
    lists `P_EX350` and `P_EX330` and ends `0 failures`.
- Gaps: whether the 1.155 call sits behind the 2.75 test (the log shows 2.75 asked just before it in the same tick [inferred
  order]; the script's branch structure was not read further); Tinker Bell's own object (`P_EX340`) was not looked for in this run; the
  two summons' friend-AI classes (follow, attack choice) are still not ported: they stand where they were put.

### Status (WP-SUM8: the classes of Chicken Little, Peter Pan and Tinker Bell, the face panel, Genie's "Drive!")

**The classes** (`summon.rs` module `scripted`; FUN_0010b780 -> FUN_0011a450 / FUN_0011a048 / FUN_0011a230). Class 0x12 (Chicken Little,
NeoStatus 204), 0x10 (Peter Pan, 202) and 0x11 (Tinker Bell, 203) keep the members' update FUN_00111070 (Chicken Little's own update
FUN_0011a2e8 runs it first, then keeps the aim point AI +0x40: the target's position, else 200 units ahead, read by 10.5; +0xb7c |= 0x1000
by FUN_00113190). Chicken Little and Peter Pan have one record each (0x3549f0, 0x354948: flags 3, handler FUN_0011a590 / FUN_0011a118,
mode string `"battle"` at +0x20); Tinker Bell none. The handler's phase 0 is FUN_00186e70 (`Runtime::set_mode_priority`): it starts the
script's `battle` mode, and that is all the class decides. `battle` (R2be0 in P_EX350) tests `1.244` (FUN_001903d8), 10.36, 10.37 and no
event, picks `follow_player`, `ball` (the shot), `whistle`, `surprise` or `use_potion` by its own tests (the enemy walk 2.4, distances to
the player, the player's HP fraction 1.230 below 0.25, the player's lock-on 10.33) and queues `battle` again. FUN_00111070 runs the record
only while the script is idle (FUN_0010fcd0 returns at once while object +0x390, the running mode record, is set: `current_mode()` is
`Some`); out of battle it runs the follow (FUN_001102c8, whose class speed rule is for the classes 0xc, 0xf and 0x10 only), and the script's
`follow_player` mode calls host 10.20 (FUN_001102c8 and the vector copies) every frame. The steer FUN_00110468 follows. In the app:
`scripted::decide` / `follow_speed` / `aim_point` / `join_ready`, and the motion table of FUN_002f6320 per class (`scripted::Data::loco`: the
default {1, 2, -1, -1, -1} for 0x12 and 0x11, Peter Pan's {0xa5, 0xa1, 0xa2, 0xa3, 0xa4}) for the walk and run motions while motion 0 or
one of the table's plays.

**Hosts** (docs/bdx.md, rows added): 10.5, 10.8, 10.17, 10.18, 10.20, 10.22, 10.23, 10.25, 10.26, 10.27, 10.28, 10.29, 10.30, 10.31,
10.33, 10.34, 10.36, 10.37, 10.44, 10.51, 10.59, 7.19, 7.24, 7.25, 6.23, 2.4, 1.244, 0.37 (`kh2-bdx` `math.rs`). The summons' object
walk (2.4: the alive hittable enemies, handles `friend_host::enemy_handle`) answers the enemies' position, reaction and boss flags; the
object's own target record is `SummonAi::scripted.target` (the script names it with 1.160, clears it with 6.16; the runtime's dispatch hands
1.160 the context's object as a third word); 1.131 clones a record into the object's pool (`Obj::new_record`); attack objects (2.10 ..),
2.17 (hits the named enemy: `combat::Hit` with the atkp row), 1.83 bone_to_world and effects (1.87, 1.21, 1.22, 1.149 on the object's own
`.apdx`, registered with the effect library at the spawn) run as for the members (`friend_ai::step_attacks`). The script's own motion
requests (1.11, 1.12, 1.13 queue) are played on the body for every summon but Stitch. The bodies carry a `Fighter` (player team, not
hittable): the summons' motion triggers hit the enemies as any object's (Peter Pan's atkp 691 and 1135 in the harness).

**Run** (harness of SUM5, `KH2_SUMMON=1|4`, tt04, 30 s): Chicken Little goes `appear_skip` -> `battle` -> `surprise` / `follow_player` /
`ball` / `whistle` / `use_potion`; Peter Pan `follow_player` / `attack_empty` / `attack_small` / `attack_blow`; no `P_EX350` or `P_EX330`
line among the `unimplemented host call` / `script error` warnings except Chicken Little's `1.0 create_object` / `1.148` in `surprise`
(Gaps). Peter Pan heals the player through 1.155 while he follows. Chicken Little's `ball` shot made an attack object (atkp 1441) that met
the terrain between his muzzle and the Dusk (the wall cast in `friend_ai::step_attacks`), so no enemy was hit by it in tt04; the `whistle`
threads (2.20, one per enemy in range) run clean (`chicken_little_whistle_threads_run`).

**The summon face panel** (`hud_party.rs` `SummonFace`, `summon_panel`; `summon.rs` `face_feed`). FUN_00156d08 (the gauge mode into 3) starts
the task FUN_00156ca8 that waits for a summon object in party slot 0 (+0x588 bit 0x400000); FUN_00156fe8 then calls FUN_00156e30(panel, 0x9a,
0x9b): a node of the field file's group 154 (plays once, then 155 looping) whose parent is Sora's HP circle (the panel's +0x1980; the groups'
own offsets are (-71, -17)), and a child node with the summon's own `face` picture (`obj/<model>.apdx`, BAR types 0x18 / 0x19 fetched by
FUN_0016b998; group 3 of its SEQD). When the mode leaves 3 (FUN_00156c78) FUN_00157020 plays group 156 (0x9c, 17 frames). The party panels
give way while the summon holds the slot. Screenshot `local/sum8/face2.png` (Chicken Little, tt04, 14 s) [looked at]: his face in a circle
left of Sora's portrait, Donald's and Goofy's circles gone. Gaps: the HUD layer order of the nodes (Z 7000 / 13000) is the scene's add
order; FUN_00156d08's other objects (group 0x82 at +0xccc and the two text objects at +0x2c40 / +0x2dd0) are not built.

**Genie's "Drive!"**: the choice of a form command (535..538, receiver 1) reaches his script as event 11 with (player, command, record)
through FUN_00177c20 -> FUN_0016b420; `SummonRequest::Form(cmd)` (harness `KH2_GENIE_FORM=<cmd> KH2_GENIE_FORM_AT=<s>`) delivers it while Genie
is out (`summon: event 11 (Drive!: form command 536): handler ran`, then motion 241 and `form command 536 (FUN_00119c18) ... byte 7 = 2, limit
command 544` in the harness). Test `genie_drive_event_plays_the_form_motion_and_sets_the_form`. **The menu entry (WP-FX12b)**: `items.rs` follows FUN_001bc6e0. With the summon state 2 and Genie's NeoStatus 201 (`FUN_001c6918() == 2`, `FUN_001c6940() == 0xc9`; `SummonState::genie_out`) the second deck's third entry is command 605 (`Execute` 605, `SubMenu` 3 = the forms list, the disc row) in place of Party, whatever the story flags say, and FUN_001bcf10 refills list 3 (the manager's +0x180) with the commands of the table at 0x37c036 (read at run time: 535..538 for forms 1..4) for each of forms 1..4 whose item count is above 0 (FUN_001c6a20; the fifth form item is not in the loop). The refill runs after the first deck was built (FUN_001bc2b8's order: FUN_001bca70, FUN_001bc578, FUN_001bc6e0), so the deck's own Drive entry sees the same list while Genie is out. Choosing a form (`Execute` = its id, no sub-menu) issues `SummonRequest::Form(cmd)` (`items::summon_request`); `summon.rs` takes it only while `genie_out` and raises event 11. Status of the entry: the generic `FUN_001ba748` rule (empty list: unavailable); the entries are ready [inferred: FUN_001ba2c0 has no case for 535..538, the port's default]. Tests `second_deck_shows_drive_with_genie_and_issues_the_form`, `genie_drive_rows_and_table_come_from_the_disc`. Not looked at in a screenshot or played: Genie needs the summon harness, and the menu key path (H for the deck swap) was only run in the unit test. Gap: the entry's target record (`FUN_0018f490(+0xa20, Genie, 0)`, whose pointer goes with each entry as its parameter) is not modelled; the event's record is made by `summon.rs` as before.

**Not done / gaps**: (1) Chicken Little's `surprise` creates objentry 2098 (`P_EX360`, a scripted bouncing object, type 9) with host 1.0 and
places it with 1.148: the app creates no objects of its own (missing, logged once); Peter Pan's `appear` creates Tinker Bell the same way
(1.0 with objentry 1268) and sets her mode through 1.8 on her context, which the runtime applies to the calling script (cross-object mode
switches are not routed: 1.8 on another object's context needs `Call::RemoteSetMode`), so Tinker Bell does not appear and her class and
script (`reraise`, 1.277 = FUN_00178df0, the player's revive with full HP) are untested beyond the constants
(`scripted_classes_from_the_executable_and_their_decision`). (2) The FPS limit of Chicken Little (hosts 10.3 / 10.4 = FUN_0011ae60 /
FUN_0011b008: player flags, camera, `0x35d268`, the screen box target search 10.51 = FUN_0011a6d8, aim 10.59) is stage 5 and not ported (WP-SM2 below ports the hosts, not the limit task).
(3) 10.29's item command is not carried out (motion 9 only; WP-SM2 below carries it out); 10.22 / 10.30 read the blow mover only; 10.44 (mover flag 0x10) reads false;
7.19 stands the object the player attacked last for FUN_00119d60's list (+0x10c bit 0x20000 not modelled); 7.24 / 7.25 set +0xb7c bit 0x400
with no reader found [GAP]; 1.18 / 1.102 / 1.157 / 1.220 (the arrival event's link and its effect from the object's own BAR entry `"00"` of
type 0x16) are not answered; 2.57 / 6.51 answer 0. (4) Summon limits (stage 5) are untouched. (5) The summons' damage strength is still
the open gap above.

