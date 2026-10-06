# Limits and reaction commands (work packages 2C and 2C2)

What the game does for reaction commands and limits, what `crates/kh2-app/src/reaction_cmd.rs` and
`limit.rs` implement, and what is open. It builds on `docs/player-data.md`. **[V]** verified (read in the
game's code or checked against the disc), **[I]** inferred, **[GAP]** not found or not ported. Function names
are addresses in `SLUS_210.05`. No game values are stored in the repository; the rows below are read from the
disc by the tests and the app, and `kh2 table limits_ex` / `kh2 table cmd` print them.

## Reaction commands

### Where an offer comes from [V]

There are five sources. Everything that reaches the player's Triangle prompt goes through the command
manager or the controller candidate (`FUN_0019c520`); the status and the accept path below are the same for all.

1. **Reaction parts in the object's file (native, the main source).** The player's controller
   (`FUN_0019c670`) calls `FUN_0019cad0`, which calls `FUN_0019cb70` for every object in the object list
   (or only for the lock-on target when player flag 0x20000000 is set). `FUN_0019cb70` walks the object's
   collision records (pointer at object +0x8d8, the BAR entry of type 23 in `obj/*.mdlx`: a 0x40-byte header
   and 0x14-byte records, walked by `FUN_00188e70` with the type mask 0x4020) and offers the command of
   every **type 5 record** (id in the halfword at +2; the id 0x83 reads the spawn record instead) that is
   enabled and whose volume the player's own type 1 volume touches. Layout, shape test and enable masks are
   in `reaction_parts.rs`. The Soldier has five such records, all with id 34 (a cross of ellipsoids around
   him: radii 700 and 300, offsets 0 and 300 / 450 to each side), and its default enable mask holds only
   part 0, so the parts of index 6 are enabled by **range trigger 4 (parameter 6) of one motion, the kick
   (slot 203)**: command 34 is on offer while the Soldier kicks and the player is inside the cross. Trigger 5
   switches a part off again; scripts do the same with host 1.52 and 1.53 (the script masks win over the
   trigger masks, `FUN_0019a6a0`). Records of type 14 take the id from the object's word at +0xaa4 instead;
   **no object file on the disc has a type 14 record** (WP-6E: `reactmap` prints `TYPE14` lines, there are
   none among the named `*.mdlx` files), so that branch has no data to run on. A part with id 0x83
   takes the halfword at +0x2c of the object's spawn record (+0x81c, stored by `FUN_0016b870`): the
   0x40-byte entities of the area's spawn lists (BAR type 12 of `ard/*.ard`). `reactmap` reads them
   (`SPAWN` lines; 3,449 of 3,456 lists walk exactly): 230 objects have a 0x83 part, nearly all `_RTN`
   townspeople and shops, and their entities carry 31 (talk) in most places and 30, 154 to 158, 191 to
   240, 249 to 252, 293, 555 to 559, 596, 604, 622, 633, 639, 642, 656 elsewhere; **none of the 124
   reaction rows comes from a spawn record**. `FUN_0019cf58` gates every native candidate
   (`reaction_cmd::native_gate`, ported, test `native_gate_follows_fun_0019cf58`): with the battle flag
   set the row needs Flags bit 4; a row with Flags bit 6 also needs the player's window: his +0x160 flag
   0x200000 (range trigger 26 of his own motion covering the frame, `FUN_00188030` case 0x1a, which also
   writes the trigger's parameter to +0x50c) and +0x50c equal to the row's `Require`. Trigger 25 (case
   0x19, flag 0x100000) writes +0x50c too, but the gate does not read 0x100000. The port reads trigger
   26 from Sora's playing motion (`reaction_cmd::window_at`), which is where the windows are: the scan
   `real_disc_window_scan` finds trigger 26 almost only in the **paired sub-motions** (BAR type 20) the
   enemies play on the player, with the id of the command that started them as parameter (M_EX920
   entry 252 opens 286, M_EX760 254 opens 300, B_MU100 253 opens 270, N_TR010_BTL 252 opens 435, ...):
   a bit 6 row is a follow-up that can be offered only during the paired motion of its `Require`.
2. **Scripts** offer through host 1.165 (`offer_command`: slot, command id, object, show flag), 1.237 (four
   commands at once, slots 3 to 6) and 1.325 (`send_reaction`: delivers event 11 straight to another object,
   used by the mission scripts of the skate and letter games). Scripts on the disc call them with a constant
   id for 22 distinct ids (`kh2_bdx::reaction::offered`). 1.113, 1.117 and 1.118 are not offers (FUN_00173310
   reads a stored vector, FUN_001905a8 sets a screen mode).
3. **A range trigger 20 in a motion** (`FUN_00188030` case 0x14 -> `FUN_00188ad0`). Among the slots of
   the motion sets only `P_EX200` / `P_EX220` use it (command 457, below); the **paired sub-motions**
   hold 35 more entries (WP-6E scan `real_disc_window_scan`): 36 and 480 (M_EX050), 28 (M_EX770), 224
   (F_MU070), 285 (M_EX660), 297 to 299 (B_EX100), 334 (F_TR060), 374 (F_MU000/010/020), 38, 39, 41
   (B_HE020), 393 (B_CA010), 433 (N_PO010_BTL), 462 (B_CA020), 504 (M_EX880_DANCER), 524 (B_LK110), 562 to
   564 (B_EX170), 641 (B_AL020), 661 / 662 (B_AL100), 665 (B_EX170_LAST), 151 (F_BB050). These play on
   the player, so `FUN_00188ad0` runs with the player as the object: the offer goes through
   `FUN_001bb3e0` (no 0x2000000 refusal) and its target record names the player himself, **unless** he is
   a party-class actor (+0x588 bit 0x80) with +0x160 bit 0x4000, when it names his +0x23c: the object
   whose paired motion he plays (`FUN_0017ee40` stores it at controller +0xfc). The setter of +0x160 bit
   0x4000 was not found (searched every `| 0x4000` store in the decompilation and the trigger cases) [GAP:
   so these offers are not ported; with the target unknown, accepting them would send event 11 to the
   wrong object].
4. **Player code** (`FUN_001cd678`: command 412 when the player enters; `FUN_001ce240`: 455 while the drive
   gauge is below its maximum, 456 when it is full; `FUN_001cef10`: the four commands of the table at 0x37c350)
   and **map objects** outside the decompiled set (`FUN_002db670`: 496, 497, 578, 579; `FUN_002e0348`: 461 and
   498; `FUN_002e44a8`: 565; `FUN_002f1988`: 523).
5. Records of the other type (14) and every object without a part use the object's word at +0xaa4 as the id
   (a copy of the hit-state word +0xa44, whose values 0, 1, 2 are command ids of the attack and magic menu;
   [I] that this is how the lock-on target's plain commands reach the same prompt).

### The prompt's rules [V]

`FUN_0019cb70` -> `FUN_0019c868` computes the candidate's key from the distance (centre of the part to the
player, scaled by 0.8 when the target record is the lock-on target [GAP: not applied]) and flags it unavailable (bit
0x40000000, status 5) when the row's `Range` is above -1 and the player does not face the object closely enough
(dot product of the direction to the object and the player's facing below `Range`; two rows have 0.5, the other
122 have -1 = off), when the cone of `Dir` / `DirRange` is not met (every reaction row has `DirRange` -1, which
never fails), or when Flags bit 1 holds with `FUN_0019aa28` for the player. `FUN_0019c520` keeps the candidate
with the higher row `Priority` and, for equal priority, the nearer one. `FUN_0019c3c8` stores it and marks it
too far when `DistRange` (three rows: 466, 516, 517) is below the horizontal distance. The slot status
(`FUN_001ba670` -> `FUN_001ba2c0`) is the Flags and `DisableForm` test already ported (`status()`).

For the commands offered through the manager (sources 2 to 4) `FUN_00188ad0` builds the target record of the
offering object and writes slot 1 with priority 100 (`FUN_001bb3e0`; a slot with a higher priority is kept). The
call through an object other than the player is wrapped by `FUN_001bb3a8`, which refuses while player flag
0x2000000 is set. It also sets player flag 0x20000 and stores the command id at player +0x50c. The manager
clears all its slots every tick (`FUN_001bb368`). The controller (`FUN_0019c670`) pushes slot 1 as input action
0xcb every tick it is available.

### Accepting [V]

Triangle (the port: R, or `KH2_REACTION_PRESS`) runs the command through the player object's class handler
(entry +0x98 of its class table). For **Sora** (class table 0x360410, built by `FUN_001ce718`) it is
`FUN_001cf030`: Execute 0x48, 4, 5, 0x49, 0x51 and 0x13c are its own (summons, forms; no reaction row has
them), everything else, 452 to 457 included, goes to `FUN_00177b18`. Only **Mickey's** class (table 0x360360,
`FUN_001cda78`) has `FUN_001ce518`, which takes Execute 0x1c4, 0x1c7, 0x1c8 and 0x1c9 itself (below).
`FUN_00177b18` sends Execute 0x1e, 0x1f, 0x20, 0x37 and 0x7f to their own handlers and the rest to
`FUN_00177c20`:

1. increment the saved use counter at slot `ReactionCount` (capped at 999, only if non-zero and the journal is
   active);
2. return if the target object is no longer in the object list (`FUN_001b80e8`);
3. set the player's bytes +0xb04 and +0xb05 to 1 (`FUN_00176a70`, the combo step counters) [not ported]; then
   `CmdAction` 1: stop the player's mover if it is moving; 2: clear his velocity (+0x20) and call the mover with a
   zero vector; the vector at +0xad0 is cleared in both cases;
4. deliver **script event 0xb (11)** to the target object's thread: `FUN_0016b420(target, controller, command id,
   record)` runs `FUN_0016c620` on the target's persistent thread with three arguments: the controller (a
   structure whose word at +4 is the player object: the script copies its first 16 bytes into a context it passes
   to the host calls that take the player), the command id, and the 16-byte target record {object, position
   index (the part index), 0, -1}. Whatever the target's script does with it is the command's effect.

The port does exactly this: `reaction_cmd::accept` writes `ReactionAccepted`, `enemy_ai::ai_update` runs the
target's handler (event 11) with (player, command id, record) and the script's host calls then reach the app.

### The target side: what Soldier's command 34 does [V from the script, executed by kh2-bdx]

The Soldier's event 11 handler (entry 0x1e, `kh2 bdx M_EX010`) compares the command id with 34 and calls the
routine at 0x39 (nothing else answers). That routine allocates an 80-byte block (host 0.47, the first 16 bytes
a context for the Soldier himself, +0x10 zero, +0x30 a target record freed with 1.131) and **starts a detached
thread** at word 0x5a through host 2.20 (`FUN_0018e898` -> `FUN_001a4738`: `FUN_0016c838` makes the thread,
`FUN_001a42b8` links it into the global list at 0x1c95518 that the task body `FUN_001a4428` runs once per frame
and empties when the thread has ended), with the block as its argument; the 16-byte controller head is copied
into the block at +0x40. The thread (word 0x5a, found by following the routine number by hand because it is a
parameter, `kh2-bdx` example `listing`):

1. the vector from the player to the Soldier, turned a quarter turn about Y (`vec_rotate_y` by 1.5708) and
   normalised: the direction D, tangential around the Soldier;
2. `set_flag(player, 3)`: **the player cannot be hit** (bit 3 of his flags, tested by `FUN_0019a4f0`);
3. `play_motion_with(soldier, player, 252, 0.0)` (host 1.29, `FUN_001715b8` -> `FUN_0017f160`): entry 252 of
   the *Soldier's* motion set is a BAR of sub-motions named `%03d0` of the player model's objentry byte +7
   (Sora's P_EX100: 1, so `0010`); the sub-motion is started on the player's controller, the player's +0x154
   becomes 252 and his +0x158 a serial taken from the counter at 0x35ccb0 (returned, kept in the thread);
4. every tick while the player's motion has not ended (`motion_done`, bit 1 of +0x160) and his +0x158 still equals
   the serial: `set_velocity(player, D, 30.0)` (host 1.89, the secondary velocity at +0x880, 30 units per tick),
   then yield;
5. `clear_flag(player, 3)`; if the serial is still the thread's own (nothing replaced the motion): play entry
   253 on the player (`play_motion_with`, the follow-up motion) and `queue_motion(player, 0, 0.0)` (host 1.13:
   idle behind it); release the target record and free the block, end.

The Soldier himself does nothing in this handler: command 34 is a **dodge around him**: the player slides on a
circle round the Soldier at 30 units per tick for the length of the paired motion, untouchable, and recovers
into the follow-up. The Soldier keeps kicking. `CmdAction` 1 (stop the player first), Mark 2, `ReactionCount` 1
(the saved counter of slot 1), message 33949, Priority 100.

Verified against the game's own interpreter (`kh2-ee`, `FUN_001d7cb0`): `tests/oracle.rs ::
soldier_reaction_command_path_matches_the_games` runs the handler with command 34 (81 interpreter steps,
`alloc`, `target`, `free_target`, `start_request` ...), with 457 and with 0 (11 steps each: the other ids do
nothing) and the routine at 0x5a (73 steps, 10 host calls), **0 differences** (program counter, operand depth,
frame pointer, top of stack and every host call with its arguments); the whole-disc sweep
(`KH2_BDX_ORACLE_ALL=1`: 1246 scripts, 19,558,975 steps, 1,520,214 host calls) still has 0 differences.

What the app does with it (`enemy_ai.rs`): event 11 is delivered; the host calls on the player turn into
`reaction_cmd::PlayerReactionCmd` messages (`PairedMotion`, `Unhittable`, `Velocity`, `QueueMotion`), the paired
motion's length is read from the Soldier's motion set so that `motion_done` comes true when it would have ended,
and the script runs to its end.

### The player's side (WP-5C) [V rules, timing partly inferred]

`player_reaction.rs` (`PlayerReactionPlugin`) consumes the messages at the start of Sora's next tick (the scripts
run after his controller, so one tick after the script asked):

- `PairedMotion`: host 1.29 (`FUN_0017f160`) clears the player's motion queue (`FUN_0017e118`) and starts the
  sub-motion on his own controller. The port reads the type 20 entries of `M_EX010.mset` / `M_EX020.mset` at start
  (`paired_entries`: motion type 9 and triggers type 16 of each sub-entry), finds `%03d0` of P_EX100's objentry byte
  +7 (`0010`) in entry `slot` of the sending enemy's set and plays it on Sora's body with
  `Animator::play_external` (a motion outside the rig's own set; `slot` reads `None` while it plays). Sora is in
  `sora::State::Reaction`: no input, gravity on, root motion of the sub-motion applied. Unit test
  `real_soldier_paired_motions`: entries 252 and 253 hold `0010`, 12 and 70 ticks, posable on Sora's skeleton.
- `motion_done` (host 1.14, bit 1 of +0x160, "the current motion has ended"): `PlayerReaction.paired_done` follows
  the real end of Sora's animator while a reaction runs (`enemy_ai` no longer counts the length; it only clears the
  bit when it asks for a new motion). An entry without the player's sub-motion is reported as ended at once
  [inferred: the lookup failure of `FUN_0017ee40` was not read].
- `Unhittable(bool)`: bit 3 of the player's flags; `FUN_0019a4f0` refuses hits while it is set. The port clears
  `Fighter::hittable` after Sora's tick while it is set.
- `Velocity { dir, speed }`: `FUN_00183418` stores the unit direction at +0x880 and the speed at +0x88c; the mover
  `FUN_00181e00` adds direction x speed x the frame step (`DAT_00356f40`, one per 60 Hz tick) to the tick's
  displacement and **clears the whole vector at its end**, so a request moves the player for one tick. The port sets
  Sora's velocity to it for that tick (moved with collision by his motor); a tick without a request has zero
  velocity. A request while Sora is not in `State::Reaction` (a hit took him over) is dropped and logged [app gap:
  the game would still add it].
- `QueueMotion { slot, blend }`: host 1.13 (`FUN_0017e170`) appends a motion id to the controller's 4-entry queue.
  When the paired motion ends the port plays Sora's own motion id `slot` (state fallback of the battle flag) with
  that blend and gives control back (Ground or Air). With nothing queued Sora holds the last frame for 2 ticks,
  waiting for the script's next request, then control returns [the app's: the controller's empty-queue case was not
  read].
- A hit or other motion request during the reaction ends it (the queue is dropped, as `FUN_0017e220` clears it);
  `paired_done` then follows that motion's end.

Log lines (all `player_reaction`): `player: paired motion started: entry 252 sub-motion 0010 of Soldier on Sora (12
ticks, blend 0), serial -1`, `player: unhittable set` / `cleared`, `player: velocity (x y z) 30.0 cm this tick`
(only with `KH2_REACTIONLOG=1`), `player: paired motion ended after N ticks: motion_done set`, `player: motion 0
queued behind the paired motion (blend 0)`, `player: queued motion 0 (A000) plays, control returns (Ground)`,
`player: paired motion ended with nothing queued: control returns (...)`, `player: paired motion interrupted: Sora is
...`, `player: velocity request dropped: ...`, and for clashes `player: clash: own atkp ...`.

### Commands 452 to 457: Mickey's class [V]

The four Execute values 0x1c4, 0x1c7, 0x1c8 and 0x1c9 belong to **Mickey's player class** (class table 0x360360,
constructor `FUN_001cda78`: player class 10, drive gauge maximum 3, message 0xbbc2 queued). Its handler
`FUN_001ce518` takes them before the generic path; **Sora's class handler `FUN_001cf030` does not**: for Sora
they are ordinary reaction commands (use counter, `CmdAction`, event 11 to the target). The port keys this on
`reaction_cmd::PlayerClass` (the app's player is Sora; `KH2_CHARGE_ON_SORA=1`, the drive.rs harness, routes
through Mickey's handler) and `mickey_command` (test `class_handlers`).

457 (Execute 457, message 48047, Mark 6, Require 455, Priority 255, Flags 347, CmdAction 0, no `DistRange`) is
offered by the **player-class** motion sets `obj/P_EX200.mset` (objentry 91) and `obj/P_EX220.mset` (objentry
792), slot 772 (A122), range trigger 20 from frame 60 to the end with position index 2, found by scanning all 1,157
motion sets (unit test `real_disc_scan`); the same slot holds a trigger 26 with 455. Neither object's script has an
event 11 handler, and Mickey's handler `FUN_001ce518` takes 457 before the generic path: **no event is sent**. It
sets the player's word at +0xb68 to 30.0 (the float at 0x37c344); 455 does the same with 120.0 (0x37c340) and starts the player's motion
0xc1. While +0xb68 is above zero `FUN_001ce240` (the player's tick) counts it down by the frame time, and each
time the second timer +0xb6c (re-armed with the float at table 0x360130 + 0xd4) runs out while the motion is near
its end (`FUN_0017f400`) it adds one unit to the **drive gauge** (`FUN_001a0290(gauge, 1)`: +1 to the fraction
byte at +0x1b0 with carry into the level at +0x1b1 up to the maximum +0x1b2); at zero it asks for motion 5
(idle). So 455 and 457 are Mickey's drive-gauge charge commands, an execute path of his class and not a script
call. For a Mickey-class player the port writes `PlayerReactionCmd::Charge { ticks, motion }` (consumed by
`drive.rs`).

- **456** (Execute 0x1c8, message 48048, Mark 4): `FUN_001ce240` (Mickey's tick) offers it in slot 1 with
  priority 100 once his gauge level (+0x1b1) reaches the maximum (+0x1b2; below it he offers 455 with himself as
  target); the first time it also runs `FUN_001ce3f8` (calls `FUN_0016a570` for 10 to 21, sets +0xb70) and queues
  the messages 0xbbc3 and 0xbbc4. Its target is the object in `DAT_00360400`, which Sora's constructor
  `FUN_001ce718` stores: **456 is aimed at Sora**. Accepting: motion 0xc0 requested, then `FUN_001ce680`: the
  message queue at +0xb74 cleared, the record at +0xb40 released (`FUN_0019b580`), +0x10c bit 0x200000 set
  (`MickeyCommand::Release`; what the flag does was not followed). Its availability is flags only.
- **452** (Execute 0x1c4, message 48020, Cost 18): **not a reaction command**: no code offers it to the
  prompt (the only `li 0x1c4` in the executable are the availability case and the handler); it is the second
  entry of Mickey's **command menu** table at 0x37c450 (`FUN_001d2970` menu kind 1: 454, 452, 1). Availability
  (`FUN_001ba2c0` case 0x1c4, ported in `status()`, test `execute_0x1c4_needs_mp`): 5 while the status block's
  float +0x1c0 is above 0 (`FUN_001a05d0`), else 2 when MP (+0x180) is below `Cost`. Accepting
  (`MickeyCommand::Cost`): +0xaf4 = `Cost`, motion 0xc2 requested, motion 4 queued.

No object of the app is of Mickey's class, so 452 and 456 are logged only.

### The 124 reaction rows [V by scan, classification below]

The disc has 124 rows with `CmdAction` 1 or 2 whose Execute equals their own id (`kh2 table cmd 700`; the unit
test `real_reaction_rows` pins their facts together with 455 and 457): every one can show in battle (Flags bit 4),
12 need the player's trigger window (Flags bit 6: 38, 113, 270, 271, 287, 380, 393, 427, 433, 436, 449, 647), two
have an offer cone (`Range` 0.5), three a distance limit (466: 1000, 516: 500, 517: 1000), 43 a saved use counter.
**All 124 are script calls**: Execute equals the id, so `FUN_00177b18` falls through to `FUN_00177c20`, which
does the player's `CmdAction` and hands the command to the target's script as event 11. None is an execute
path of the player. What differs is the target's script, which decides the motions, damage and effects.

`kh2_bdx::reaction::handled` reads, for every script on the disc, the command ids its event 11 handler compares
with (a constant in front of a subtraction, the Soldier's pattern); the example `reactmap` also lists the objects
whose collision table has type 5 parts and the ids scripts offer themselves. Over the 1,243 scripts on the disc,
177 have an event 11 handler and 229 distinct ids are handled (some are false positives of the pattern: ids that
are not commands). Cross-checked with the 124 rows:

| class | rows | meaning |
| --- | --- | --- |
| native part + script handler | 92 | an object with a type 5 part of that id **and** a script that answers it: the full path of command 34 |
| script handler, offerer not found | 22 | a script answers the id; the offer is made by something this scan cannot see (a part whose id is computed, a script offering with a computed id, map code) |
| native part, no handler found | 4 | a part offers it; no script was found that compares the id (the handler may use a table, or the id belongs to the player-side motion only) |
| offered by a script, no handler found | 2 | |
| script handler, offered by a script | 2 | |
| neither found | 2 | |

**What the app can run**: only the objects the app spawns, Shadow and Soldier. Shadow has no reaction part and no
event 11 handler. Soldier has command 34: **it is the only one of the 124 rows the app can exercise end to end**.
Every other row needs an enemy or boss that is not in the app; the table lists each id with the objects whose
script answers it (the enemy AI it needs), `CmdAction` and the counter slot. The generic path (offer, status,
accept, event 11) is the same for all of them and is ported; the missing part is the object (model, motion set,
script) and the player-side consumer of the script's calls. Ids are listed with their base object name; the
variants (`_RAW`, `_TR`, `_WI`, `_NM`, ...) answer the same.

### What the port does

`reaction_cmd.rs` (`ReactionPlugin`), `reaction_parts.rs` (collision parts), `clash.rs`, and in `enemy_ai.rs` the
delivery of event 11:

- every tick the offer is rebuilt like the game's slot: **native parts** of every living enemy whose table
  (`obj/M_EX010.mdlx`, `M_EX020.mdlx`, read at start-up) has enabled type 5 records touching Sora's body volume
  (`P_EX100`'s type 1 record, radii 35 and 55) compete by row priority and distance (`native_offers`); a range
  trigger 20 of the playing motion and the harness offer overwrite the slot like `FUN_00188ad0`. The status follows
  the rules above (`status()`: command flags, form mask, `DistRange`, the `Range` cone); an Execute value the
  player's availability switch handles is reported as `NotPorted`.
- the key (R) or `KH2_REACTION_PRESS=<s>,...` accepts an available offer (`accept`): use counter, `CmdAction`,
  `ReactionAccepted { cmd, pos (the part index), target }`; 455 and 457 go the player's own way (`Charge`).
- `enemy_ai::ai_update` turns `ReactionAccepted` into **event 11** with (player, command id, target record) on the
  target's BDX runtime; the host calls the script makes on the player come back as `PlayerReactionCmd`
  messages; the detached thread of host 2.20 runs in `Runtime::update_requests`; the script heap behind host
  0.47 is the runtime's own (`HEAP_BASE`, bump allocator).
- resources: `ReactionOffer(Option<Active { cmd, pos, from, status, message }>)` (the HUD capsule input),
  `ReactionUses`, `PlayerReaction { serial, paired_done, remaining }`.
- harness: `KH2_REACTIONLOG=1` logs offers, withdrawals, accepts and refusals; `KH2_REACTION_FAKE=<command id>`
  makes every living enemy offer that command (`KH2_REACTION_FAKE_ON=Soldier` limits it to one kind); with
  `KH2_BDXLOG=1` the script's calls are in the log (`event 11: ...`, `player side: ...`).

Acceptance run (`KH2_REACTIONLOG=1 KH2_BDXLOG=1`, Sora locked on and attacking every 0.45 s from 6 s, R pressed
every second from 12 s, `kh2-game tt00`; nobody watched the video, the evidence is the log). A Soldier arrives
with the second wave; its attacks meet Sora's, it kicks, and the prompt appears and is accepted:

```
clash: clash: Player attack 129 meets Enemy attack 405
reaction_cmd: reaction: offered command 34 (message 33949) by 994v1, status Available
reaction_cmd: reaction: accepted command 34 (Execute 34, CmdAction 1, counter slot 1 = 1) from 994v1; delivering script event 0xb (player, command id, target record) to the target's script
enemy_ai: bdx Soldier#5 t1880: event 11: reaction command 34 (position index 6, record 0x6000000); handler ran
enemy_ai: bdx Soldier#5 t1880: player side: Unhittable(true)
enemy_ai: bdx Soldier#5 t1880: player side: PairedMotion { from: 994v1, slot: 252, blend: 0.0, serial: -1, ticks: 12.0 }
enemy_ai: bdx Soldier#5 t1880: player side: Velocity { dir: Vec3(0.40124437, -0.0, -0.91597104), speed: 30.0 }
enemy_ai: bdx Soldier#5 t1881: player side: Velocity { dir: Vec3(0.40124437, -0.0, -0.91597104), speed: 30.0 }
enemy_ai: bdx Soldier#5 t1892: player side: Unhittable(false)
enemy_ai: bdx Soldier#5 t1892: player side: PairedMotion { from: 994v1, slot: 253, blend: 0.0, serial: -2, ticks: 70.0 }
enemy_ai: bdx Soldier#5 t1892: player side: QueueMotion { slot: 0, blend: 0.0 }
```

### Clash (attack against attack) [V rules, ported for the enemy side]

`clash.rs` implements the detection `FUN_0018d808` makes and the enemy-class reaction (`reaction_rules::enemy_clash`,
the handler at 0x1b2478 read from the disassembly). Live attack volumes of opposing teams (`combat::live_attacks`) are
paired when the 7 by 7 table at 0x37ac98 allows the two rows' `Type` (read from the executable into
`ExeConsts::clash_pairs`: type 0 clashes with 0, 2 and 3; type 1 with 3; the table is symmetric; 4 to 6 never) and the
capsules overlap. The enemy side then plays `ReflectedMotion` (140 to 143; the Soldier's claw rows 5 and 1313 and
the Shadow's rows 9 to 12 have it) with the default recoil push backwards along its facing (`enemy::receive_clashes`,
`EnemyClash`), and **both volumes are disabled** (`FUN_0018d6b0`) unless a side's own row says `RefactSelf` 2: the
window is marked as landed so `combat::detect_hits` skips it. The player's side is the party handler
`FUN_001754a0` (`reaction_rules::party_clash`): the `Clash` message carries both rows and the ground result;
`player_reaction::receive_clashes` applies it to Sora with his own airborne state (WP-5C, rules in
docs/hit-reaction.md: latch `connected`, spark only / recoil 0x29 or 0x2b with combo step refund / guard bounce 0x2a
with push 200, facing the other attack and 80 invincible ticks). The spark (`FUN_0018d560` ->
`FUN_001db868(&DAT_01c954d8, id, ...)`) is element 0xd (spark only, recoil) or 0xb (guard bounce of a party actor,
+0x588 bit 0x80; 0xc otherwise) of the **system file** `00effect.bar`, placed at the own volume's centre moved toward
the other's by the own radius. The spark is started through `fx::StartSystemEffect { id, at }` (handler
`fx::start_system_requests`: `start_placed` on `EffectLib::system` with the translation replaced and a fixed bind;
`fx::StartEffect` only reaches the owner's own effect file). `player_reaction::receive_clashes` sends it with the
`spark` id `party_clash` returns, at `own centre + unit(other centre - own centre) * own radius`; the volume centre is
the midpoint of the capsule [inferred: the hit code's own centre is private to `combat.rs`]. The game starts it only
when neither volume has flag 0x80, which `FUN_0018d560` itself sets. `FUN_0018d808` first calls `FUN_0018d6b0` for both
sides, which would start a spark from the *owner's* effect file (category 0x10 plus the row's `EffectOnHit` bit) and set
the flag, silencing the handler's; none of `W_EX010`, `P_EX100`, `M_EX010`, `M_EX020` has a 0x10 element
(`fx::tests::clash_spark_elements_by_file`), so for these objects the handler's system spark is the clash spark and the
flag is clear. The enemy-side handler does not call `FUN_0018d560` (only `FUN_001754a0` and `FUN_0018d6b0` do), so the
spark is made once per clash. Evidence (`KH2_FXLOG=1`, the long fight of `accept.ps1`'s reaction-34 scenario; clashes
happen by chance, 1 of 2 runs here had none): `player: clash: own atkp 127 ... recoil motion 41 ...; system effect 0xd
(started at Vec3(-3.607, 0.395, -1.024))`, then `fx: system effect 0xd at Vec3(-3.6067522, ...)` and `fx: fx 13 objs 9
batches 7 verts 1176` (effect 13 = 0xd running at that point). The element itself was looked at in a screenshot made
with `KH2_FXTEST=13` (a bright ring and sparks); a screenshot of the spark at a real clash was not taken (the clash
moment is not reproducible).

WP-6E ported the cone test of `FUN_0018d308` for the 70 rows with `ReflectRange` (`reaction_rules::reflect_cone`,
both ways in `clash::detect_clashes`; a pair outside the cone is logged with `KH2_HITLOG=1`), the owner flags of
`FUN_001754a0` (`ClashActor`: Sora's range triggers 3, 30 and 24) and blend 0 for the recoil and guard-bounce
motions; details in docs/hit-reaction.md. Not ported: `ReflectAction` (an offset from the atkp row to an action
record for the action system at +0x300, `FUN_0019e870` / `FUN_00186db0`; the record format was not decoded), the hit-registry tests of `FUN_001be108` (a pair that already hit each other
does not clash again), `ReflectRC`, the air push `FUN_00185978` (decoded up to the mover's method +8 and a run-time
table, docs/hit-reaction.md; the ground push stands in). WP-6G added the enemy handler's script part (the running
mode's slot 8 hook and event 0x13, on every clash, docs/enemy-ai.md); none of the app's three scripts handles it.

Guard-bounce spark (effect 0xb): no real clash in the app's fights has produced a guard bounce (the rows met had
`RefactSelf` 2 or both 0). `KH2_CLASH_FORCE_GUARD=<s>` (harness, `player_reaction::receive_clashes`) runs
`party_clash` once with a guarding own row against a reflecting row and starts the returned spark 20 units in
front of Sora at chest height (the place stands in for the volumes; the bounce itself is not applied). Run
`KH2_CLASH_FORCE_GUARD=5 KH2_FXLOG=1 kh2-game tt00`:

```
player: clash (forced by KH2_CLASH_FORCE_GUARD): own RefactSelf Guard against RefactOther Reflect, ClashActor { fun_0019aa28: false, flag_40: false }: GuardBounce { motion: 42, power: 200.0, star: 80.0 }; system effect 0xb (started at Vec3(1.152, 1.000, -0.150))
fx: system effect 0xb at Vec3(1.1520001, 1.0, -0.1501)
fx: fx 11 objs 9 batches 3 verts 52
fx: fx 11 objs 9 batches 5 verts 260
```

A screenshot 0.15 s later did not show the spark clearly; nobody has looked at it otherwise.

### Gaps of this section

- The player side of the script calls is ported (WP-5C) except: the one-tick lag behind the script, the velocity
  while a hit has taken Sora over, the keyblade during the paired motion (it follows Sora's slot, which reads none,
  so it holds its rest pose; whether the game moves the weapon then was not read), the empty-queue hold. Not ported:
  the Mickey objects that offer 455 to 457 (no object of the app is of Mickey's class), the trigger 20 offers of
  paired sub-motions (their target rule, source 3), part id 0x83 (spawn records are not read by the app; no
  reaction row comes from them), joints other than 0x4000 (bone matrices of `FUN_0016aa18`),
  cylinder shapes, the lock-on distance bonus.
- `FUN_0019ae90` (the object-side refusal: flags +0x7f8 bits 4 and 0x400, +0x108 bit 0x40, +0x160 bit 0x8000000,
  `FUN_0019b088`, the object-entry flag 0x2000) is read as "the enemy is alive"; `FUN_00176a70` (the player's combo
  counters) is not ported.
- The object's orientation matrix (+0x40) is read as a rotation about Y with local +z forward [I]; the Soldier's
  cross is symmetric under a half turn but not under a quarter turn.
- The serial counter of `FUN_0017ef98` (0x35ccb0) starts at an unknown value; the app starts it at 0 (scripts only
  compare serials).
- Summons: not touched. The summon gauge rule was not met on the way.

### The 124 rows: which object answers each

The "objects" column lists the objects whose collision table has a part with the id and whose script answers it
(first block); for the other blocks the objects that have one of the two. Rows of the second, fourth, fifth and
sixth block have no offerer or no handler this scan could see (computed ids, map code, tables): they are gaps, not
claims.

**native part + script handler** (92 rows)

| id | Mark | CmdAction | counter slot | objects |
| --- | --- | --- | --- | --- |
| 27 | 3 | 1 | 18 | M_EX770 |
| 28 | 8 | 1 | 35 | M_EX770 |
| 33 | 2 | 1 | 0 | F_MU070 |
| 34 | 2 | 1 | 1 | M_EX010 |
| 35 | 2 | 1 | 41 | M_EX050 |
| 37 | 3 | 1 | 3 | M_EX050 |
| 38 | 3 | 1 | 0 | B_HE020 |
| 53 | 6 | 1 | 0 | N_BB080 |
| 69 | 2 | 1 | 22 | M_EX990 |
| 70 | 2 | 1 | 36 | M_EX710 |
| 71 | 7 | 1 | 37 | M_EX710 |
| 77 | 3 | 1 | 0 | B_MU120 |
| 79 | 3 | 1 | 0 | B_MU120 |
| 105 | 2 | 2 | 4 | M_EX520 |
| 106 | 2 | 1 | 6 | M_EX690 |
| 143 | 2 | 1 | 0 | N_BB050 |
| 148 | 3 | 1 | 0 | B_BB110 |
| 152 | 3 | 1 | 46 | M_BB010_SWORD |
| 173 | 5 | 1 | 0 | F_BB050 |
| 236 | 5 | 1 | 0 | N_BB050 |
| 237 | 5 | 1 | 0 | F_BB100 |
| 238 | 5 | 1 | 0 | B_HE100, B_HE110 |
| 253 | 5 | 1 | 0 | N_DC010 |
| 269 | 2 | 2 | 9 | M_EX660 |
| 273 | 5 | 1 | 0 | F_CA050 |
| 274 | 5 | 1 | 0 | F_WI390, F_WI390_RTN |
| 277 | 5 | 1 | 0 | F_NM100 |
| 300 | 3 | 1 | 17 | M_EX760 |
| 301 | 1 | 1 | 23 | M_EX910 |
| 303 | 1 | 1 | 47 | M_EX900 |
| 307 | 1 | 1 | 0 | N_EX760, N_EX760_BTL_CLSM, N_EX760_BTL_HERCULES, N_EX760_BTL_MEGARA, N_EX760_BTL_WILLY |
| 308 | 2 | 2 | 15 | M_EX210 |
| 309 | 2 | 1 | 13 | M_EX600 |
| 310 | 2 | 2 | 11 | M_EX730 |
| 311 | 2 | 1 | 8 | M_EX550 |
| 312 | 1 | 1 | 26 | M_EX950 |
| 315 | 1 | 1 | 0 | N_HE010 |
| 323 | 1 | 1 | 27 | M_EX950 |
| 326 | 5 | 1 | 0 | F_BB060 |
| 327 | 1 | 1 | 0 | F_TT010, F_TT010_CA, F_TT010_HE, F_TT010_SORA |
| 328 | 2 | 2 | 14 | M_EX110 |
| 329 | 2 | 1 | 12 | M_EX740 |
| 331 | 2 | 1 | 16 | M_EX420 |
| 333 | 1 | 1 | 0 | F_TR060 |
| 335 | 5 | 1 | 0 | B_HE100 |
| 336 | 5 | 1 | 0 | F_WI400, F_WI410, F_WI420 |
| 337 | 7 | 1 | 0 | F_WI400, F_WI410, F_WI420 |
| 346 | 5 | 1 | 0 | B_HE100 |
| 355 | 1 | 1 | 0 | B_CA040, B_CA040_PUB |
| 361 | 1 | 1 | 0 | B_CA020 |
| 362 | 5 | 1 | 0 | F_MU000, F_MU010, F_MU020 |
| 364 | 1 | 1 | 0 | F_WI360, F_WI360_PETE |
| 366 | 5 | 1 | 0 | B_LK120 |
| 368 | 2 | 1 | 21 | M_EX670 |
| 370 | 2 | 1 | 19 | M_EX780 |
| 371 | 5 | 1 | 0 | F_NM170_CATCH |
| 380 | 6 | 1 | 31 | M_EX800, M_EX800_DC, M_EX800_MU |
| 381 | 1 | 1 | 0 | N_DC010 |
| 391 | 1 | 1 | 0 | N_DC010 |
| 392 | 4 | 1 | 0 | B_CA010 |
| 397 | 1 | 1 | 0 | M_EX880_DANCER |
| 398 | 1 | 1 | 0 | F_NM070 |
| 431 | 1 | 1 | 0 | F_CA010 |
| 437 | 1 | 1 | 0 | F_TT000 |
| 440 | 5 | 1 | 0 | F_TR150 |
| 444 | 3 | 1 | 0 | B_EX110 |
| 446 | 4 | 1 | 0 | B_CA050 |
| 448 | 2 | 1 | 0 | F_TT060 |
| 449 | 3 | 1 | 0 | F_TT060 |
| 458 | 1 | 1 | 0 | B_EX150 |
| 466 | 3 | 1 | 0 | F_TR020, F_TR170 |
| 481 | 2 | 1 | 30 | M_EX850 |
| 490 | 5 | 1 | 0 | B_EX110 |
| 491 | 5 | 1 | 0 | B_EX140 |
| 493 | 5 | 1 | 0 | B_EX220 |
| 494 | 3 | 1 | 0 | N_HB500, N_HB530 |
| 501 | 1 | 1 | 0 | N_WI010, N_WI010_BTL_VS |
| 502 | 3 | 1 | 0 | B_EX120 |
| 516 | 3 | 1 | 0 | F_TR030 |
| 517 | 4 | 1 | 0 | F_TR160 |
| 533 | 3 | 1 | 7 | M_EX060 |
| 581 | 4 | 1 | 0 | P_CA000, P_CA000_HUMAN, P_CA000_HUMAN_LOW, P_CA000_LOW |
| 603 | 4 | 1 | 0 | B_NM110 |
| 613 | 1 | 1 | 0 | F_TT170 |
| 615 | 1 | 1 | 0 | B_TR000 |
| 624 | 1 | 1 | 0 | B_EX210 |
| 625 | 5 | 1 | 0 | F_EH050 |
| 629 | 2 | 1 | 42 | M_EX590 |
| 630 | 3 | 1 | 43 | M_EX590 |
| 631 | 4 | 1 | 44 | M_EX590 |
| 632 | 5 | 1 | 45 | M_EX590 |
| 648 | 3 | 1 | 48 | M_BB010_AX |

**script handler, offerer not found** (22 rows)

| id | Mark | CmdAction | counter slot | objects |
| --- | --- | --- | --- | --- |
| 36 | 7 | 1 | 2 | M_EX050 |
| 113 | 1 | 1 | 39 | M_EX890 |
| 286 | 1 | 1 | 25 | M_EX920 |
| 287 | 1 | 1 | 25 | M_EX920 |
| 304 | 1 | 1 | 0 | M_EX900 |
| 314 | 2 | 1 | 10 | M_EX750 |
| 318 | 3 | 2 | 5 | M_EX620 |
| 345 | 2 | 1 | 0 | B_EX100 |
| 367 | 3 | 1 | 20 | M_EX670, M_EX720 |
| 393 | 7 | 1 | 0 | B_CA010 |
| 427 | 3 | 1 | 17 | M_EX760 |
| 432 | 2 | 1 | 0 | B_EX100 |
| 435 | 1 | 1 | 0 | N_TR010 |
| 436 | 1 | 1 | 0 | N_TR010 |
| 463 | 5 | 1 | 0 | N_HE010 |
| 480 | 3 | 1 | 3 | M_EX050 |
| 492 | 3 | 1 | 0 | B_EX140 |
| 524 | 1 | 1 | 0 | B_LK110 |
| 635 | 1 | 1 | 0 | B_EX170 |
| 647 | 1 | 1 | 0 | B_EX130 |
| 666 | 2 | 1 | 0 | B_EX100 |
| 667 | 2 | 1 | 0 | B_EX100 |

**script handler, offered by a script** (2 rows)

| id | Mark | CmdAction | counter slot | objects |
| --- | --- | --- | --- | --- |
| 507 | 5 | 1 | 0 | F_TT110 |
| 561 | 1 | 1 | 0 | B_EX170 |

**native part, no handler found** (4 rows)

| id | Mark | CmdAction | counter slot | objects |
| --- | --- | --- | --- | --- |
| 235 | 5 | 1 | 0 | F_BB100 |
| 278 | 3 | 1 | 0 | F_NM130 |
| 576 | 3 | 1 | 0 | F_NM130 |
| 577 | 3 | 1 | 0 | F_NM130 |

**offered by a script, no handler found** (2 rows)

| id | Mark | CmdAction | counter slot | objects |
| --- | --- | --- | --- | --- |
| 270 | 6 | 1 | 0 | B_MU100 |
| 271 | 7 | 1 | 0 | B_MU100 |

**neither found** (2 rows)

| id | Mark | CmdAction | counter slot | objects |
| --- | --- | --- | --- | --- |
| 433 | 1 | 1 | 0 | - |
| 602 | 4 | 1 | 0 | - |

### The 30 rows of the last five blocks (WP-6E)

Searched again with every offer source the code has (`reactmap`: parts, spawn records, scripts with constant and
with computed ids; `real_disc_window_scan`: trigger 20 and trigger 26 in every slot and paired sub-motion):

| rows | offerer found | handler | note |
| --- | --- | --- | --- |
| 36, 480 | trigger 20 in M_EX050's paired sub-motions (entries 253; 242 and 254) | M_EX050 | follow-ups offered while the player plays M_EX050's paired motion; target rule gap (source 3) |
| 393 | trigger 20 in B_CA010's paired 252 (frames 178 to 220) | B_CA010 | Flags bit 6 with `Require` 392 is not read on this path (`FUN_0019cf58` is only the native gate) |
| 524 | trigger 20 in B_LK110's paired 253 | B_LK110 | |
| 433 | trigger 20 in N_PO010_BTL's paired 252; window 433 in its paired 254 | not found (N_PO010_BTL's handler compares 0 and 5) | |
| 113 | host 1.165 with a computed id in M_EX890's script (pc 0x348); window 113 in M_EX890 paired 253 | M_EX890 | |
| 427 | 1.165 computed in M_EX760 (pc 0x6b3, also `_NM`, `_RAW`); window 300 (its `Require`) in M_EX760 paired 254 | M_EX760 | the follow-up of 300 |
| 647 | 1.165 computed in B_EX130 (pc 0xb33); window 647 in its paired 253 | B_EX130 | |
| 345, 432, 666, 667 | 1.165 computed in B_EX100 (pc 0x8c7) | B_EX100 | |
| 367 | 1.165 computed in M_EX670 (pc 0x658) | M_EX670, M_EX720 | |
| 270, 271 | 1.165 with constants in B_MU100; window 270 in its paired 253 | B_MU100 has an event 11 entry but compares no constant (a table or switch the pattern misses) | 271 (`Require` 270) is 270's follow-up |
| 287 | none; window 286 (its `Require`) in M_EX920 paired 252 | M_EX920 | M_EX920 has no type 5 part and offers nothing the scan sees |
| 286, 304, 314, 318, 435, 436, 463, 492, 635 | none | M_EX920, M_EX900, M_EX750, M_EX620, N_TR010_BTL (435, and 436 = its follow-up, window 435 in paired 252), N_HE010_BTL, B_EX140 (window 492 in paired 254), B_EX170_LAST (window 635 in paired 252 / 253) | the offer is made by something no scan reads: the action system at +0x300, an overlay, or a script call through a host other than 1.165 / 1.237 / 1.325 |
| 235, 278, 576, 577 | type 5 parts of F_BB100 and F_NM130 | none found | field objects whose event 11 handler (if any) does not compare constants |
| 602 | none | none | no reference on the disc found |

None of these objects is in the app; the generic path they need (offer, gate, status, accept, event 11) is
ported. What the app still lacks for them: the objects themselves, and for the paired-motion offers (source 3)
the target rule (+0x160 bit 0x4000).

### HUD hook

`CommandMenu.reaction` (hud_menu.rs, a private submodule of `hud.rs`) takes `message` (the command's `MessageId`,
bit 15 ignored), `alt` (Flags bit 3, 0x08) and `kind7`. `reaction_cmd.rs` cannot reach it because `mod hud_menu` is
private. **Hook for the orchestrator**: make `hud_menu` visible to the crate (`pub(crate) mod hud_menu` in `hud.rs`)
and copy `ReactionOffer.0` into it each frame: `menu.reaction = offer.0.filter(|a| a.status == Available).map(|a|
Reaction { message: a.message, alt: flags & 8 != 0, kind7: false })` plus `menu.visible = true` only when the
command menu itself is open (the capsule is drawn with the menu).

## Limits

### Data [V]

27 rows in `limt` (`kh2 table limits_ex`; the test `real_rows_need_companions` prints them). Columns: Id,
three participant bytes (Character, Summon, Group), file, spawn id, command, Limit item, world, plus from the
command row the MP cost byte and `Time`.

- Ids 0 to 12: single-partner limits (Auron, Beast, Mulan, Goofy x2, Donald x2, Aladdin, Sparrow, Simba, Jack,
  Tron, Riku). Participant bytes `[partner, 1, 0]`; the `Limit` item is a type 19 ability (bit 57 to 70) that the
  partner must have; the Goofy and Donald rows have a second row with world 13 (a different file, `*_wi.bar`)
  that wins only in that world (the Hundred Acre Wood ids in the notes are a guess; the world number is 13).
- Id 13: Trinity, three rows: `[1, 2, 3]` (world 13 variant and the world 0 row) and a row with participant bytes
  `[1, 0, 0]` (file `trinity_zz.bar`, command 620) that **needs no partner**. Which situation selects it was not
  found [GAP] (the name suggests the `zz` world of the title or a special area).
- Ids 14 to 20: summon limits (Summon byte 200 to 204, `Limit` is the charm item): Peter Pan, Stitch, Chicken
  Little, and the four Genie rows (one per form, `Time` 24 or 60).
- Every normal limit has Cost 255 and `Time` 120, 180, 300 or 600; summon limits cost 100 or 50 summon gauge.
- `Time` is the duration passed to the limit task (`FUN_001b2c10`); its unit was not traced (the port counts
  60 Hz ticks [I]).

### Rules ported [V from FUN_001d1ce0, FUN_001d2040, FUN_001b3e38, FUN_001b28a0]

`limit.rs` (`LimitPlugin`): `usable()` is `FUN_001d1ce0` (world, command lock, each non-zero participant byte must
be a present, living party member, member ids 2 and 3 refused in form field 6, the type 19 ability bit of the
`Character` member), `available()` is `FUN_001b3e38` (usable; normal limit needs MP, summon limit a non-zero summon
gauge), `row_for_command()` is `FUN_001d2040` (first usable row of the command).

**MP cost [I]**: all normal limits have Cost 255. Read literally (`FUN_001b3e38` compares the byte with the MP block
at status +0x180; `FUN_001b9ff0` replaces it only in a special battle) no MP value reaches 255, so the notes call it
"all MP" (player-data.md). The port treats it as "needs some MP, spends all". Whether the game's MP unit makes 255
reachable was not checked [GAP].

Activation in the port, with the party absent: `KH2_LIMIT=<command id>` tries that command once at 4 s, key L steps
through the commands in table order. The party is Sora alone, so every partner limit is refused:

```
limit: limit 0 command 82 (participant bytes [5, 1, 0], file auron.bar, cost 255, time 120) refused: MemberAbsent(5) (party [])
```

`KH2_LIMIT_PARTY=5 KH2_LIMIT_ABILITIES=63` pretends Auron and the ability are there; the limit then starts: MP is
spent, the limit's resource BAR `limit/us/<file>` is read and its entries listed, `LimitStarted { command, file,
time }` is written and the "duration" is counted down:

```
limit: limit 0 command 82 (...) started, MP now 0; resource BAR entries limi:3 LIMI:20 limi:18 00:22 10:22 wave:32 se40:31 comb:24 comb:25 gage:24 gage:25; the task body is not ported
limit: command 82 ended (duration over)
```

Gaps: the limit task body (the `limi` type 3 script, the PAX effects, the `LIMI` motion/model set, the
companion motions, the sound bank), the participants' limit state (`FUN_001c75d0`), the camera, the Limit menu
list (`FUN_001d21e0`, `FUN_001d2320`), Auto Limit (the consumer of the ability bit was not found), the battle flag
that gates the prompts (`FUN_001d2460`), and every companion: the app has none, so no limit with a partner can run
as gameplay. The only row Sora alone satisfies among the normal limits is the partnerless Trinity variant, and
the summon limits once the summon gauge exists.

## Summons

Not implemented ("only if cheap": it is not). `sumn` rows and the Execute 72 path (`FUN_001cf210`) are in
`docs/player-data.md`. Needs a summon gauge (gauge mode 3 of the drive code), the summon objects and their scripts,
none of which exist in the app.

## Hook points (state after WP-2C2)

1. `hud.rs` (WP-4B3): mirror `ReactionOffer` into `CommandMenu.reaction` (above). Done by 4B3.
2. `enemy_ai.rs`: **done here**: `ReactionAccepted` is delivered as event 11; Soldier's thread for command 34 runs.
3. `sora.rs` / the drive plugin (WP-2B2): the player's form id into `reaction_cmd::Ctx.form` (now 0) and the battle flag.
4. Consumer of `reaction_cmd::PlayerReactionCmd`: **done by WP-5C** (`player_reaction.rs`, see "The player's side"
   above). `Charge` is consumed by `drive.rs` (WP-6A) and written only for a Mickey-class player (WP-6E).
5. Consumer of `clash::Clash`: **done by WP-5C** (`player_reaction::receive_clashes`); the clash spark (system
   effect 0xb / 0xd of `00effect.bar`) goes through `fx::StartSystemEffect` (see the clash section). Not done: the
   spark of an enemy-owner row through `FUN_0018d6b0` (no object of the app has a category 0x10 element).
6. `drive.rs`: the `PlayerReactionCmd::Charge` of commands 455 and 457 (+1 drive gauge unit each time the timer runs
   out, see above).
7. A shared `LimitParty` source once companions exist (it is a resource today).
