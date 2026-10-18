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
- `Time` is the duration passed to the limit task (`FUN_001b2c10`). Unit [V, WP-LM1]: the task's time gauge (task `+0xa8`) starts
  at `Time`, and every tick of `FUN_001b35e8` (called each frame by `FUN_001b27b0`) sets it to its value minus the frame step
  `DAT_00356f40` (1.0 in the executable) times the float at the player's status block `+0x1cc + 0x5c` (`FUN_001768a8`;
  what writes that factor was not traced [GAP]), clamped at 0 by `FUN_001cfc28`. So `Time` counts 60 Hz ticks when the factor
  is 1, as the port counts. The time only runs while none of the player and the participants has `+0x168` bit 0
  (`FUN_001b3a08`; the meaning of the bit was not traced [GAP], the port does not stop the count).

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
list (`FUN_001d21e0`, `FUN_001d2320`), Auto Limit (since WP-AL1 the offer is `FUN_001d25e0`, below), the battle flag
that gates the prompts (`FUN_001d2460`), and every companion: the app has none, so no limit with a partner can run
as gameplay. The only row Sora alone satisfies among the normal limits is the partnerless Trinity variant, and
the summon limits once the summon gauge exists.

## The limit task (WP-LM1) [V unless marked]

### The original's flow

1. **Start** (`FUN_001cf030` case 0x51 -> `FUN_001b28a0(command id, player)`). The id is the halfword at `+6` of the command in
   the player's command slot (`+0xb44`). `FUN_001b6f48` finds the command row, `FUN_001d2040` the first usable `limt` row of the id
   (the port's `row_for_command`).
   - **No row** (the id is a sub-command): `FUN_0016c620(thread at DAT_01d5d6b0 + 0x34, 0x17, [id], 1, 0)`: **event 23** with the
     command id as the only argument, to the running limit's thread. Nothing else happens. (With no limit task the original reads
     `0x34` of a null pointer; not followed [GAP].)
   - **A row**: allocate the task (`FUN_00104fa8(0xdc)`, kept in `DAT_01d5d6b0`, which is what `FUN_001b3c00` tests), run
     `FUN_001b2c10` (below), pay (a row whose `Summon` byte is 200 to 204, `FUN_001d1fe0`: `FUN_001788d8(-Cost)` on the summon
     timer; otherwise `FUN_00186368(player, -FUN_001b3f00(cmd))`, MP), call `FUN_00175db0` when the second byte of the row is not 1
     (not read [GAP]), call `FUN_001c75d0(0, member)` for the two party slots (`FUN_0017cd08(0..1)`) that are present, not flagged
     `+0x588 & 0x400000` and have `+0xb4c` zero (the party member's limit-state hook: `FUN_001c7600` sets `+0x7f8` bit 0x10
     and queues a handle in the list at `0x1d5e380`, `DAT_0035ff30` entries; not ported [GAP]), then `FUN_001b35e8(task, 1)`
     (the first tick).
2. **`FUN_001b2c10`** (task creation): stores the row at `+0`, the player at `+0xc`, sets the player's `+0x10c |= 0x10000` (the
   in-limit bit host 10.19 reads), calls `FUN_0016b698(player, 0)` unless the command is 282, 344 or 484 (`FUN_001d1f98`), and for
   each of the row's three bytes above 1 stores the party member or summon (`FUN_0017d400`) at `+0x10`, `+0x14`, sets its
   `+0xb4c` to 1 and `+0x10c |= 0x10000`, with the same `FUN_0016b698` call. Then:
   - `FUN_001b2e08` loads the BAR `limit/<region>/<FileName>` (`FUN_001d1ff8`, format `limit/%s`, `FUN_0016fd18`) and looks up
     its entries by kind: **0x14** (stored at `+8`), **0x12** (`FUN_001db4d0` on `+0x18`: the limit's sound set), **3** (the limit
     script: `FUN_0016c9b8` makes a thread, **`FUN_0016c500(thread, 0x16, [task], 1)` sends event 22** with the task as argument;
     the thread is kept at `+0x34`), **0x22** (a mover object, `+0x44`). It reads the two gauge sprite sets (kinds 0x18 and 0x19
     with the tags `comb` and `gage` at `0x37ba38` / `0x37ba40`): `comb` is the **hit counter** (task `+0x68`, range 0..999, its
     marker raised to the saved best by `FUN_001d00c0`, shown by `FUN_001cff20` except for commands 484 and 620), `gage` the
     **time gauge** (task `+0xa8`, range `Time`). For command 484 it also starts the `comd` command-set display (`FUN_001b3a70`).
   - `FUN_001b3bc0(task)` is "no type 0x14 entry". **With** the entry the task sets bit 0 of the word `0x35feb8`
     (`FUN_001c6bd0`; `FUN_001d2460` refuses new limit prompts while it is set, drive.rs holds Drive on it), and the limit brings
     its own object set. **Without** it `FUN_001b3118` creates a thread on the first participant's script
     (`*(participant + 0x484)`, entry of event 0x16 from `FUN_0016c4c0`) and **sends it event 22** with the task as argument;
     that thread replaces the one at `+0x34`, so it is what event 23 reaches later. `FUN_001afe78(0)` then allocates a
     screen-effect slot (`+0x38`, flag 2 set) that the end releases.
3. **Per frame** (`FUN_001b27b0` -> `FUN_001b35e8(task, 0)`): ends the task when `FUN_001b3970` holds (below); otherwise it
   follows the actor-stop flags (`DAT_0035d268 & 0x20` skips the tick), lets the sequencer `FUN_0016c680(thread, 0) == 1` end the
   task through `FUN_001b3910` (flag 4), and decreases the time gauge as above. `FUN_001b3970` ends the task when the BAR's script
   state (`FUN_0016f038(bar +0x24) & 8`) is not set and the hit counter and time gauge widgets have both finished
   (`FUN_001cfb30`, `FUN_001cfd90`: the HUD animation, not just `Time` reaching 0), no party member is in the limit-state
   (`FUN_001c7c30`) and `+0x24`, `+0x3c` are zero. So the task outlives the gauge by the length of its out animation [GAP: not measured].
4. **End** (`FUN_001b37f8` then `FUN_001b32d8`): ends the script thread, clears `+0x10c` bit 0x10000 on the player and
   participants and their `+0xb4c`, releases the sound set and the screen-effect slot, `FUN_001d25b0(row id, marker)` stores the
   hit-count record (the port's `LimitHits::finish`), `DAT_01d5d6b0 = 0`, clears bit 0 of `0x35feb8` when it was set, and
   destroys the objects of kind 5000 the limit spawned (type 0x14 entry present).

The party-limit **offer** (not the Limit menu): the player controller (`FUN_0019c670`) calls `FUN_0019d430` only when no limit
task exists (`FUN_001b3c00`), the player's `+0x588` has bit 0x80 and not 0x2000000, `+0x160` bit 0x4000 is clear and no native
candidate was taken. `FUN_0019d430` asks `FUN_001d25e0(player, the controller's preferred command id at *controller +0x26)`:
for every `limt` row whose `Character` is not 1, usable (`FUN_001d1ce0`) and set up (`FUN_001d1ea0`: the limit's file loaded, the
party members present and not busy), the partner object (`FUN_0017d400(Character)`) must carry Auto Limit (his own ability block
`FUN_001768a8(member) + 0x10`, bit 1 = ability bit 129, item 417) and stand within `500.0` (`0x37c414`, read from the executable) of
the player, horizontally; its command must have a status below 4 (`FUN_001ba2c0`). The earlier reading "alive (`+0x10 & 2`)" was
wrong: that word is the ability block, not a life flag. A row whose command is the preferred id wins at once; otherwise one of the
candidates is chosen at random: see "Auto Limit (WP-AL1)" for the formula. The winner becomes the reaction prompt's candidate
(`FUN_0019c3c8`) like a native part, and is accepted through the same Execute 81 path.

### The app's flow, and what changed here

`limit.rs` follows steps 1 and 2 for the pieces that exist: `row_for_command` (`FUN_001d2040`), `available` (`FUN_001b3e38`), the
cost (MP or the summon timer through summon.rs), `LimitTask` (`FUN_001b3c00`), `LimitHits` (step 2's counter and step 4's record),
the 60 Hz duration count (step 3's `Time` unit). WP-LM1 adds the **script events**: at start the plugin reads the limit BAR's
kinds (`LimitBar`), and sends event 22 as a `LimitScriptEvent` to the receivers `start_events` lists (the type 3 script and, for a
BAR without the type 0x14 entry, the first participant: the summon); a command without a limit row that is an Execute 81
sub-command (`sub_commands`: Stitch's 525 to 527, Never Land's 469 ...) becomes event 23 with the id for the receiver
`event_receiver` names, while a limit runs. The old gap in docs/summons.md (who sends events 22 and 23) is answered by these
functions. Disc facts, pinned by `real_limit_bars_and_sub_commands`: the 20 partner limit files (Auron to Riku, Trinity in all its variants) have both the type 3 script and the type 0x14 entry, so their event 22 goes to their own script and event 23 would too; the 7 summon limit files (Peter Pan, Stitch, Chicken Little, the four Genie forms) have neither, so the only receiver is the summon's script, for event 22 at the start and event 23 for the Execute 81 commands without a `limt` row. 98 such sub-commands exist on the disc (the partner limits' follow-ups too, for example 75 and 76 after Beast's 74) (the test pins 469, 525, 526, 527).

Harness: `KH2_LIMIT=<command>` as before; `KH2_LIMIT_SUB=<command id>` sends that sub-command 120 ticks after the limit starts;
the log lines are `limit: script event 22 (Task) to ... (FUN_001b2e08 / FUN_001b3118)` and
`limit: sub-command N: script event 23 to ... (FUN_001b28a0 first branch)`.

### What is still missing (gaps with their functions)

- **The type 3 limit script** runs since WP-LT1 (next section). What stays open: the host calls listed there as gaps, and the
  receivers: `LimitScript` events go to `limit_script.rs`, `Participant(n)` events to the summon (summon.rs, WP-SM5:
  `summon::objects` reads `MessageReader<limit::LimitScriptEvent>` and delivers them to the summon object whose neo id is `n`,
  `limit_deliveries`, with `LimitArg::Task` mapped to `LIMIT_ATTACK` [inferred] and `LimitArg::Command(c)` to the integer `c`).
  `KH2_SUMMON_EVENT` stays as a harness aid.
- The participants' limit state (`FUN_001c75d0` / `FUN_001c7600`), the object set of the type 0x14 entry (kind 5000 objects), the
  camera hand-over (`FUN_0016b698(player, 0)` is the actor hook, its camera effect was not followed), the two HUD gauges
  (`comb`, `gage`), the sound set (kind 0x12), the screen effect slot (`FUN_001afe78`) and the task's end condition
  (`FUN_001b3970`) are not ported.
- The party-limit offer and its random choice (`FUN_001d25e0`): ported by WP-AL1 (next section), with the gaps listed there.

## Limit script (WP-LT1) [V unless marked]

`limit_script.rs` runs the limit BAR's type 3 entry (`limit/us/<file>`) as a BDX thread. 20 partner limit files have one (Auron to
Riku, Trinity's three files); the summon limit files have none.

### The original

- `FUN_001b2e08` (limit task creation): `FUN_001684c0(bar, 3, 0)` finds the entry, `FUN_0016c9b8` makes a thread on it, task `+0x34`
  (word index `0xd` of the task) holds it, `FUN_0016c500(thread, 0x16, [task], 1)` points it at event 22 with the task as the one
  argument. The same function fills the time gauge (`+0xa8`, current value `+0xd4`) with the command's `Time`
  (`FUN_001cf528`, `FUN_001cfc28(Time, gauge)`), and for command `0x1e4` (Trinity) calls `FUN_001b3a70(task, 3)` (the `comb` / `gage`
  HUD animation: sets bit 8 of task `+0x40`, adds 1 to `+0x3c`).
- The task's frame `FUN_001b35e8` runs the thread with `FUN_0016c680(thread, 0)`; when it returns 1 `FUN_001b3910` sets bit 4 of
  `+0x40`, and the next frame's `FUN_001b3970` (the end test: gauges at 0, no animation, thread done) ends the task. Each frame, while
  `FUN_001b3a08` holds (the player and the participants are not down, `+0x168` bit 0 clear), the gauge value falls by `DAT_00356f40 x`
  the player status's `+0x5c` (`FUN_001cfc28(value + fVar5 * status[+0x5c], task + 0xa8)`).
- Event 23 (a sub-command) goes to the same thread through `FUN_0016c620(task + 0x34, 0x17, [id], 1, 0)` (`limit::sub_command_event`).

### The script, as the port runs it (Trinity, event 22: routines at words 0x22f6 to 0x2341)

Setup (routine 0x2342): reads the task's owner (host 2.48) and participants (2.49 index 0 and 1: Donald, Goofy), binds them to the
context (1.210 by NeoStatus 2 and 3), records the target records (2.23, 1.131), starts the child thread (2.54, routine `0x241c`) and the
screen effect (7.28). The main loop then yields while host 2.60 (the gauge value, task `+0xd4`) is above 0 and the context's
flag word `+0x2c` is set. At 0 the tail runs: 2.89 (releases the HUD animation, `FUN_001b3b60`), then a wait on 2.63 (the animation
count, task `+0x3c`) and a yield, then the cleanup (6.47, 7.30, target release 1.132) and the thread returns. The other 13 files
have the same shape: the first loop on 2.60 and a tail on 2.63.

### What the app does

- `LimitScriptPlugin` (in `CombatPlugins`, after `LimitPlugin`): on `LimitScriptEvent { event: 22, to: LimitScript }` it reads the
  type 3 entry of the file `LimitStarted` named, builds a `kh2_bdx::Runtime` with the executable's host table and calls
  `start_thread(0, 22, [TASK])` (`TASK` = `0x7a000000`, summon.rs's `LIMIT_ATTACK` value); event 23 is `restart_thread(23, [command])`
  [inferred: that the thread takes it as the cast task takes event 21, `restart_thread`]. The thread runs one `update_script` slice per
  60 Hz tick (`FixedUpdate`, after `Sim::Sora`).
- The countdown stays the end condition. The gauge value (2.60) is the countdown's ticks left, so the script's own wait ends at 0 as
  in the original; when `LimitTask` goes `None` the thread keeps ticking for at most 30 more ticks (its tail) and is dropped.
- A fault (a VM error, a step limit or a panic) is logged once (`limit script: fault ...`), the thread is dropped, the limit goes on.
- Logs (`RUST_LOG=info`): `limit script: start <file> ...`, `limit script: first call host g.i <name> at tick n: <call>` (once per
  host function), `limit script: ended after N ticks (the limit is over); host calls: g.i xN ...`, `limit script: fault ...`.
- Hosts ported: 2.48 (task `+0xc`, the player: `FUN_001b2c10` stores its fourth argument there), 2.60 (the countdown, [inferred] in
  ticks: the unit the app's `LimitTask` already counts `Time` in), 2.63 and 2.89 (`TaskState`: one animation for command `0x1e4`,
  released once), 1.94 (true for the player only), 1.132 (no pool), 6.47 (removes stop id 17 from every object: the app holds none),
  0.0 to 0.2, 0.8, 0.24 to 0.26 (debug prints). 7.28 / 7.30 start and end a screen effect (`FUN_002f3bd8` makes the object
  `DAT_00371518` from `FUN_00104fa8(0xa00)` and `FUN_0016e208(0x5c3)`; `FUN_002f3c18` ends it): answered with nothing, not drawn.
  Everything else answers 0.

### Harness results (`KH2_LIMIT=<command>`, all rows usable: `KH2_LIMIT_PARTY=2,3,5,...,13`, every ability bit)

| command (file) | result |
|---|---|
| 484 Trinity | runs 180 ticks, tail 2 ticks, returns: `1.94 1.114 1.131 2.23 2.48 2.49 2.54 2.60 2.63 2.89 6.47 7.28 7.30` |
| 82 Auron, 74 Beast, 139 Mulan, 255 Goofy, 338 Goofy 2, 344 Donald 2, 351 Aladdin, 357 Sparrow | run the whole countdown and return; they add some of 1.0 1.11 1.13 1.15 1.18 1.23 1.34 1.130 1.183 1.220 1.234 2.47 2.50 2.55 2.59 7.3 7.11 |
| 282 Donald, 376 Simba, 405 Jack, 415 Tron, 569 Riku, 620 Trinity (no group) | fault in the first tick (`BadAddress`): the script reads a participant object (2.49 / 1.114 answer 0: the app has no partner objects) |

### Gaps

- **2.54**, the partner objects and the small task hosts were gaps of WP-LT1; WP-LT2 (next section) ported the first, stood in for
  the partners and answered some of the hosts. What stays open is listed there.
- The task's end (`FUN_001b3970`: gauges, the HUD animations, `FUN_001c7c30`) stays the countdown.

## Limit script, part 2 (WP-LT2) [V unless marked]

### Host 2.54, the child thread

`0x18ea28` -> `FUN_001b3c10(routine, &arg, 1)` -> `FUN_0016c8d8(*(limit task + 0x34), 0, routine, args, 1)`: a child of the limit
script's own thread, the call host 1.82 makes through `FUN_00172df0` (which also stores the cleanup routine at the new thread's `+0x24`;
2.54 passes tag 0 and no cleanup). `kh2-bdx` `runtime.rs` answers it as `Request::Spawn { root: main_handle, tag: 0, cleanup: 0,
args: [second argument] }`; the name row is in `names.rs` (`limit_spawn_thread`, no result). Test: `runtime::spawn_tests::host_2_54_spawns_a_child_that_runs`
(the child calls host 0.14 once, the parent keeps yielding). Harness: Trinity (command 484) now reads the gauge from two threads,
`2.60x361` for a 180 tick limit (the parent about 181, the child 180); the child makes no host call besides 2.60.

### The participants

`FUN_001b2c10(.., task, row, player)` walks the three participant bytes of the limit row (row `+1` to `+3`, `LimitRow::members`:
`Character`, `Summon`, `Group`) and for each byte above 1 stores the party member object `FUN_0017d400(byte)` in the next of task
`+0x10`, `+0x14`, `+0x18` (sets byte `+0xb4c` of the object to 1, bit `0x10000` of its `+0x10c`). So the participants are the real party
member objects, found by character id: Donald and Goofy for Trinity, the limit's own character (Auron 5, Simba 11, ...) for the others.
Host 2.49 reads those slots, 1.114 / 1.210 find an object by entity id / NeoStatus.

The app has Sora only, so `limit_script.rs` hands out stand-in handles (`partner(id)` = `0x7a100000 + id`, no memory behind them):
2.49 answers the i-th participant of `participants_of(members)`; 1.210 by NeoStatus finds a participant by its character id
[inferred: the scripts bind 2 and 3 for Donald and Goofy], 1.114 by entity id finds the player (0x236) only. `LimitStarted` carries the
row's `members` for this.

Hosts answered for the stand-ins and the player (positions come from Sora, `Sora::motor`, game space): 1.147 position (the player's;
a participant stands at the player [inferred]; any other handle at the origin), 1.83 bone to world (position plus the vector, no turn
[inferred]), 1.98 ground position (the position [inferred]), 1.201 facing vector (`(sin yaw, 0, -cos yaw)` as the enemy host
answers it, Sora's yaw), 2.55 (task `+0x38`: a stand-in object, non-null as in the original), 7.3 (`FUN_002efe30` resets the 30 slot
pool at `DAT_01e356a0`: nothing to do, the app has none), 7.11 (`FUN_002effd8`: the pool's free count is 0x1e: true), 7.21
(`FUN_002f3a08` copies five words to `DAT_00371400`; no reader in the decompiled code: nothing to do).

Answering 2.55 non-null changed which branch some scripts take (they now test for the object and go on to 1.83 / 1.147 / 1.201 /
1.98), which is the branch the original always takes.

### Harness results after WP-LT2 (same set-up as above)

| command (file) | result |
|---|---|
| 484 Trinity | runs 180 ticks (two threads), returns |
| 82 Auron, 74 Beast, 139 Mulan, 255 Goofy, 338 Goofy 2, 344 Donald 2, 351 Aladdin, 357 Sparrow, 282 Donald, 405 Jack | run to the end and return (282 and 405 used to fault) |
| 376 Simba, 415 Tron | (WP-LS2: run to the end) fault in the first tick: `BadAddress(0x2000020)`, a direct read of the player object (`0x2000000`) at offset `+0x20` through a pointer, which no host serves |
| 569 Riku, 620 Trinity (no group) | (WP-LS2: both run to the end, see part 3) fault in the first tick: `BadAddress(0)` after host 2.56 (`ptr_b40_4_b`, a pointer into the participant's sub-object, answered 0) |

A faulting script is logged once and dropped; the limit goes on (unchanged).

### Gaps

- Simba / Tron read the player object's `+0x20` directly (words `0xc4b` / `0x316`); the object has no memory in the limit host, and
  `+0x20` of a player object has no documented meaning (for the thread objects it is a tag).
- Riku / Trinity-zz take a pointer from 2.56 and read through it (words `0x481` / `0x172`).
- 2.47 (`FUN_001b3438` -> `FUN_0017f0c0`: sets up a motion controller of the participant), 2.50 (`FUN_001b3468`: an event start, see "Limit camera" below; the old reading was: starts a PAX or
  object effect) and 2.59 (`FUN_001b3c48` -> `FUN_001db868`: looks a row up in a table by halfword, then `FUN_001db5f0`) are visuals or
  motions of the participants; the decompiled signatures of these wrappers do not match their call sites, and the app has nothing to
  draw: left logged, answered 0.
- 1.0 / 1.11 / 1.13 / 1.15 (the participant's motions), 1.18, 1.130 / 1.183 / 1.220 / 1.234 and 1.23 / 1.34: the enemy and summon runtimes
  implement them against their own object state (`enemy_host.rs` reads `self.obj` and `self.env`), so they are not shared; the
  limit host has neither (1.23 is the world number byte `0x33e830`, 1.34 the event word `0x35e3a4`). Answered 0, logged.
- The Trinity child's own host calls are the same as the parent's (2.60), so nothing further is logged.

## Limit script, part 3 (WP-LS2) [V unless marked]

All 15 limit scripts run to the end now (harness: `KH2_LIMIT=<command>`, party 2,3,5-13, every ability bit, tt00, `KH2_NO_ENEMIES=1`):
Simba 376 (330 ticks), Tron 415 (302), Riku 569 (303), Trinity-zz 620 (182) and the eleven of WP-LT2 (none faults).

### The four faults

- **Simba / Tron** (words `0xc4b` / `0x316`, routine `R0c3e` / its twin): the script copies 16 bytes from the object `+0x20` of the context's
  object (`load [ctx]+4`, `+ 32`, `copy.ind 16`). That is the object's velocity vector [inferred from the enemy scripts' use of that offset]: host 1.58 (`0x172988`) stores the dash direction at
  object `+0x20` and the Quick Run start `FUN_00178108` its velocity there (docs/camera.md). The limit host now serves it: `ObjectMem`
  in `limit_script.rs` answers reads at the player object (`0x2000000`) and the participant stand-ins (`partner(id)` = `0x7a100000 + id *
  0x100`, `0x100` bytes each); `+0x20..+0x2f` is Sora's velocity as a game-space vector (`motor.vel`, set each tick), writes are stored and
  read back. A participant's stand-in has Sora's velocity [inferred: the party follows Sora].
- **Riku / Trinity-zz** (words `0x481` / `0x172`, routines `R0463` / `R0154`): host 2.56 (`0x18de58`; no decomp file, the address rests on `names.rs`; the pointer at object `+0xb40` plus 4,
  the word the target controller keeps its target in) answered 0 and the script read through it. It now answers `TARGET_SLOT`, a word that reads 0
  (no target) [inferred: the app's lock-on is not an object of this host]; the script prints "no lockon target" and goes on. The second
  fault was a store through the target record the script files with `2.23` -> `1.131`: `1.131` (`0x1736f8`) copies the 5 word record
  into a pool at `DAT_01c89bd0 + 0x14 * index` and answers the copy's address. Host 2.23 now answers `OWN_RECORD` (the player as the
  object, word 3 = -1, as `enemy_host::target_record`) and 1.131 files a copy in `RECORD_POOL` (256 records, never reused; 1.132 stays a
  no-op), so the script's later store into the record (the 6.57 result at record `+4`) lands. Hosts 1.160 and 6.57 are ported (WP-LS3, see "Hosts 1.160 / 6.57").

### Hosts 2.47, 2.50, 2.59

`TaskState::requests` keeps each call as a `Request` (`Motion`, `Effect`, `MotionRow` with the raw arguments). 2.47 (`FUN_001b3438` ->
`FUN_0017f0c0`, a participant's motion controller) and 2.59 (`FUN_001b3c48` -> `FUN_001db868`: the walk over the rows of 0x50 bytes of the
motion table at task `+0x18`, halfword `+2` = the id; 0 when nothing matches, and the app holds no such table) answer 0 [inferred]; 2.50
(`FUN_001b3468`, corrected in WP-LS2b: an event start, see "Limit camera") answers nothing and now also sends `LimitEvent`. Nothing is drawn or played:
the stand-ins have no model [GAP, candidate WP LS2b].

### Tests

`host_1_147_gives_the_player_position`, `hosts_1_83_1_98_and_1_201_follow_the_player_state`, `object_memory_serves_the_velocity_and_the_target_word`
(also 2.23, 1.131 and the record store) and `participant_requests_are_kept`, in `limit_script.rs`.

### Not done (goal D)

The limits' camera, effects, gauges and motions are not visible: they need models for the participants (the app has Sora only), the
motion tables of the BARs (2.47, 2.59), the effect starts (2.50, 7.28) and the HUD animations (`FUN_001b3970`'s end test). Candidate WP LS2b (done in part: next section).

## Limit camera (WP-LS2b) [V unless marked]

### Host 2.50 is an event start, not an effect (corrects WP-LT1 / LT2 / LS2 above)

`FUN_001b3468` (`0x18e9f8`) looks the entry up in the limit BAR: `FUN_001b3bc0(task)` is `task +8 == 0` (the "LIMI" party set pointer,
false for every file that has one), then `FUN_00168510(bar, 0x16, name, 0)` (the first entry of kind 0x16 whose four byte name equals the
script's string; the data pointer is the entry's `+8`). With no party set it takes `FUN_001971c0` instead (the player model's BAR at object `+0x7b0`).
It then makes an event object: `FUN_00195898(obj, 0xb, data)` (kind 0xb, the `evtp` row), `FUN_00195a78(obj, player)` (stores the player
at event `+0x44`, which `FUN_00195ae0` hands to `FUN_002232c8` as `DAT_0036bd50`; the scan `--addr 0x36bd50` finds only the reset and that
store, so nothing reads it) and `FUN_00195ae0(obj)`, the call every area event goes through (docs/events.md section 6). Before that it
runs `FUN_00183eb0` (a stepped collision move of an object; not read further) and `FUN_0016a890` / `FUN_0016a948` (stores the player's
turn word `+0x66c`). The only caller of `FUN_00195898` in the limit code is this function; a scan for `jal 0x1b3468` finds none (it is
reached through the host table).

The limit BARs hold two events each (kind 0x16, names "00" and "10"; Aladdin also "01"), e.g. Trinity `re_p_ex100-00` (opcode 0 name),
Auron `re_p_he000-00`. Their commands are `0x0` (name), `0xf`, `0x10`, `0x8` (end frame), `0x13` (camera keys), `0x6` (cut table), `0x46`
(map pull-in) or `0x31` (mode 0): a camera track and nothing else (no actors, no messages, no effects). `intro::Track::parse` reads them.
Examples (opcode 8 end frame, event frames; an event frame is half a game tick): Auron "00" 115, "10" 190; Trinity "00" 333; Beast "00"
110; Mulan "00" 94; Aladdin "00" 80, "01" 67; Sparrow "00" 125 (three cuts); Simba "00" 90; Jack "00" 54; Tron "00" 151.

The script gives the name as a pointer to a four byte literal in its code (host argument 1; Auron `268442262` = code base `0x10000000` +
`0x396`). `limit_script::name_at` reads it from the program image (`Runtime::new` maps the words as bytes at `shared.layout.code`).
Harness (`KH2_LIMIT=<command>`, party 2,3,5-13, abilities 62..70, tt00, `RUST_LOG=info`), 2.50 starts at tick 1 of the limit in every file
that calls it: Auron `"00"`, Beast `"00"`, Mulan `"00"`, Aladdin `"00"` then `"01"` (two calls), Sparrow `"00"`, Simba `"00"`, Jack `"00"`,
Tron `"00"`; Riku does not call it (the `_wi` and Trinity files were refused by the harness's world, not run). Log lines `limit script: host 2.50 starts event "00" of auron.bar`.

### What the app does (`limit_camera.rs`, `LimitCameraPlugin` in `CombatPlugins`)

`LimitEvent { file, name }` (written by `limit_script`) starts the track: each 60 Hz tick (`Sim::EffectTick`) the event frame is
`ticks * 0.5` (`FUN_00222728`), `Track::at` picks the cut and evaluates the keys (`FUN_0022b0f8`, ported in WP-I1), the result goes to
`CamEvent::track` (with the pull-in flag of opcode 0x46) and at the end frame the track stops, which hands the view to the field camera
(`Rig::handover`, as the intro). A new event replaces a running one (`FUN_002232c8` ends it). Logs `limit camera: ...`. Tests:
`the_limit_files_hold_camera_events` (disc: end frames 115 / 190 / 333, no event in Donald's file), `the_event_runs_to_its_end_frame_and_hands_the_camera_back`,
`a_held_frame_does_not_advance`, `the_keys_are_in_the_players_frame`, `host_2_50_names_an_event_from_the_program_image` (script).

**Off by default, `KH2_LIMIT_CAMERA=1` turns it on** (`KH2_LIMIT_CAMERA_HOLD=<event frame>` holds one frame for screenshots). Two reasons.
1. **The space of the keys is a GAP, the app's choice is [inferred].** The original evaluates the keys in the event matrix `DAT_0036bce8`,
the event object's `+0x50`, which `FUN_00195898` fills with `FUN_0011b460` from the constant at `0x354a60` (`FUN_0011bd58` writes it:
the unit matrix scaled by `DAT_00378934`; no other writer: `mips_scan --addr 0x354a60` / `0x36bce8` find `FUN_0011b198`, `FUN_0011b460`,
`FUN_0011bd58` and `FUN_00223170` / `FUN_002232c8` / `FUN_0022b0f8`; store offsets 0x50, 0x54, 0x60, 0x70 in the event methods find
none), and the player-space branch of `FUN_0022b0f8` needs opcode 0x31 mode 1 (`DAT_0036bce0 == 1`), which no limit event has. Read
literally the camera sits near the area's origin, while every key set frames a figure at the origin (look-at heights 56 to 130 cm, positions
within a few metres: Trinity "00" goes from behind the shoulder, z = -125, to the front, z = +129). The app therefore puts the keys in the
player's frame when the event starts (origin at his feet, key +z along `Motor::forward`) [inferred]. How the original places the
frame (maybe the participants are moved to the origin by the limit's own calls; `FUN_00183eb0` was not read to the end) is open.
2. **The participants have no models.** The shots of Sparrow, Mulan and others frame the participants (the harness screenshots at
event frames 70 and 50 show empty sky and a dark wall); only the shots of Sora himself (Auron 0 / 25 / 60 / 100, Simba 45) show
something.

Harness PNGs looked at (not committed; `local/ls2b_*.png`, Auron 82, `KH2_LIMIT_CAMERA=1 KH2_LIMIT_CAMERA_HOLD=<f>`): frame 0 and 25, the lens
inside Sora's black and orange clothing (a very close shot); frame 60, Sora's hip and hand in close-up against the sky; frame 100, Sora from
behind at full length, centred, with the Twilight Town skyline behind him; after the end (231 ticks) the field camera behind Sora again.
Simba frame 45: Sora's neck and shoulder in close-up. The log line `camera: intro camera off: the field camera takes over from ...`
(`camera.rs`) closes every event.

### Gaps (addresses searched)

- **What the event kind 0xb holds back** (the player's control, the world stop `FUN_00184b40(1)` / `FUN_0016bf78(1)` that `FUN_001b35e8`
  undoes when the event is over, the HUD hide): not ported; the limit task's bits 1 and 2 of `+0x40` (`FUN_001b35e8`) go with it.
- **Host 7.28 / 7.30** (`FUN_002f3bd8` / `FUN_002f3c18`): an object of 0xa00 bytes built from `FUN_0016e208(0x5c3)` -> `FUN_001d1470`
  by `FUN_002f3aa0` -> `FUN_001c01c0(obj, row, 0, 1, 0, 0x80, 0x100)`, stored in `DAT_00371518` with the vtable `0x371528`. Objentry row
  1475 is "PRIZE" (type 22, sub type 5) [whether `FUN_001d1470` reads that table was not checked], which does not read as a screen effect; its class and
  draw were not traced. Not drawn.
- **The HUD animation** (`FUN_001b3a70`, `FUN_001b3b60`): only for command `0x1e4` at the task's creation (`FUN_001b2e08` line 87, the only
  caller). It looks the BAR's kind 0x18 / 0x19 entries named `gage` / `comb` (name at `DAT_0037ba58`, four bytes not read) up and starts
  group 3 on the command deck HUD object (player `+0xb44`, `FUN_001bbb18` -> `FUN_001d2848` / `FUN_001d2918` at that object's `+0x94c`,
  `FUN_001bb558(.., 0xd, 0)`). The command deck's module is not in this package's ownership and the placement is not traced: not drawn.
- **Hosts 2.47 / 2.59** (participant motions): unchanged, not drawn; there are no participant models, and the motion table at task `+0x18`
  is not in the app.
- **Hosts 1.160 / 6.57** (WP-LS3): ported. `kh2-bdx` gained `Host::call_with_mem(&Call, &mut dyn ScriptMem)`, which the runtime calls for
  every host call that reaches the host (the default forwards to `Host::call`, so no other host changes); `ScriptMem` reads and writes
  the script's image, globals, frames and heap. Both functions are in the function table (`GROUP_TABLE` 0x35bce8; the names table has no
  entry, so they arrive as `Call::Unknown` with raw arguments):
  - 1.160 = 0x173770, 2 arguments, no result: `FUN_0018f490(*args[0], *(args[1] + 4), 0)`, i.e. record = `args[0]`, context = `args[1]`.
    `FUN_0018f490` first runs `FUN_0018ed40` on the record (word 0 = 0, words 1 and 2 = 0, word 3 = -1), then stores word 1 = third
    argument (0) and word 0 = the second (the context's word at `+4`, where 1.114 / 1.210 leave their answer). The port writes the
    four words `[object, 0, 0, -1]` (the record's fifth word is not touched, as in the original).
  - 6.57 = 0x2eec10, 1 argument, one result; no decomp file, read from the instructions (`lw t7,0(a0); lw t6,4(t7); jr ra; sw t6,0(a0)`):
    the answer is the word at `+4` of the block the argument points at (the same context word). 0 when the address is in neither the
    script's nor the limit's object memory [inferred].
  A record that is in the limit's object memory (the pool of 1.131) takes the write there instead of the script's memory.

## Limit visuals (WP-LS2c) [V unless marked]

`limit_visual.rs` (`LimitVisualPlugin`, in `CombatPlugins`) plays what hosts 2.47 and 2.59 ask of the participants. It corrects two
earlier readings: 2.59 is not a lookup of a motion row, and the motion data is the file's `LIMI` entry.

### Host 2.47: a motion of a participant

- **The data.** The limit BAR's kind 0x14 entry `LIMI` (`FUN_001b2e08` stores its data at task `+8`) is a BAR of 259 entries in the layout
  of a motion set's paired entries (`player_reaction::paired_entries`). The entries 252 to 258 are kind 20 sets (`A910` to `A915`); the
  count differs per file (Goofy 252 to 254, Donald 2 252 to 256, Auron 252 to 258, Trinity-zz 252 only). Each set is a BAR of sub-motions
  named `%03d0` of a model byte (`0010` Sora, `0030` Goofy, `0050` Auron; `0011` and `0012` belong to the keyblade objects), each an
  animation BAR (type 9 motion, type 16 triggers) made for that character's skeleton. Test `the_limit_files_hold_their_motion_sets`.
- **The call.** `FUN_0018e988` (`0x18e988`) -> `FUN_001b3438` -> `FUN_0017f0c0` -> `FUN_0017ef98` -> `FUN_0017ee40`. The scripts call it with
  (the task, a pointer to a context block, the motion id 252 to 258, the blend as a float). The participant is the word at `+4` of the
  block (`FUN_0018e988` reads `*(arg1 + 4)`; the scripts' 2.48 / 2.49 / 1.210 leave their answer there). `FUN_0017ef98` formats `%03d0`
  (string at `0x37a598`) with the object's model byte (`FUN_001699b8`: `*(*(obj + 8) + 7)`, the objentry `WeaponJoint`); `FUN_0017ee40` looks
  the name up with `FUN_00168510(set, -1, name, k)` for k = 0 to 3 (the first non-empty entry) and starts it on the controller at
  `obj + 0x140` (`FUN_0017dd38`; the blend lands at `+0xf0`). When `+0x588` has bit 4, `FUN_0017ef98` repeats this for the two objects at
  `+0xaf8` / `+0xafc` with the name's last digit raised by one. [inferred] That the set is the entry whose index is the motion id
  (`FUN_001a7da8` is `FUN_001684c0(bar, -1, index)`): the ids 252 to 258 and the entry counts of the files agree (Goofy's script uses 252
  to 254 and its `LIMI` has entries 252 to 254), and the same lookup serves host 1.29 for the player's paired motions.
- **The app.** `limit_script` reads the participant word through `call_with_mem` and sends `LimitMotion { who, id, blend }`;
  `limit_visual::play_motions` finds the body (Sora, or the `PartyMember` whose NeoStatus is the stand-in's character id), looks the
  sub-motion up (`find`) and plays it with `Animator::play_external`. Sora enters `State::Reaction` (no input, the state the paired
  motions use) and keeps the last frame until the limit task ends. A party member's body gets the `LimitHold` marker, which
  `party::party_tick` checks before it picks its own idle / walk motion. When the limit task is gone the marker is removed and Sora's
  control returns (Ground or Air).
- **Harness** (`KH2_LIMIT=255 KH2_LIMIT_PARTY=2,3 KH2_LIMIT_ABILITIES=57..70`, area tt04 so Donald and Goofy stand there, `RUST_LOG=info`):
  `limit visual: goofy.bar holds 3 motion sets (entries [252, 253, 254])`, `motion set 252 sub-motion 0010 on handle 0x2000000`,
  `sub-motion 0030 on handle 0x7a100300`, `the limit is over, Sora's control returns`. PNG at `KH2_SHOT_AT=4.6` looked at: Sora crouched
  in a close stance facing the camera, Goofy leaning forward with an arm out, both mid-motion (the HUD line reads `Reaction 0010 f38`).
  PNGs at `KH2_SHOT_AT=6.5` and `8` looked at: both frozen on the last frame (Sora with raised arms, Goofy in a jump pose; the line reads
  `f65`). Control returns when the limit ends.
- **Not ported.** A participant that is not in the field party: the files of Auron, Simba, Mulan, Beast, Aladdin, Sparrow, Jack, Tron
  and Riku name a character the app does not spawn, so the call is logged (`no body in the field`) and skipped (Donald 2, Goofy 2 and the
  Trinity files use Donald and Goofy, who are there). The keyblade sub-motions (`0011`, `0012`), the root motion, the placement of the
  participants (the scripts move them with hosts the limit host does not serve) and the controller's queue (`+0xf0`, `+0xfc`). Each
  script asks for one motion per 2.47 call; after the last one the app holds the final frame until the limit ends, where the original
  plays on from the script's later calls (which the limit host answers only in part).

### Host 2.59: a PAX element on a participant (corrects "a lookup of a motion row")

`0x18eb00` -> `FUN_001b3c48` -> `FUN_001db868(task + 0x18, id, flag, 1, 30000, object)`: the function the clash spark starts a system effect
with (`FUN_001db868(&DAT_01c954d8, id, 1, 1, 10000, 0)`). The table at task `+0x18` is filled by `FUN_001b2e08` from the limit BAR's kind 0x12
entry (`FUN_001db4d0`: rows of `0x50` bytes, the halfword at `+2` the element id, the count at the header's `+8`), which is the file's own PAX
(`limi`, type 18). `FUN_001db5f0` starts the element: its effect (`FUN_001da6f0` -> `FUN_001d9bf0` walks the DPX list by number), the position,
rotation and scale vectors of the row, a start wait (plus `DAT_0037c778` when bit 0 of the row's flags is set), and for a bound element
it hangs the effect on the owner's controller (`FUN_0017fbe0(owner + 0x140, effect)`). The scripts call it with (the task, the element id,
a context block, 1); the owner is the word at `+4` of the block (the third argument here, the second for 2.47).

The app (`limit_visual::start_effects`) loads the file's PAX with `fx::load` when the limit starts (`limit/us/<file>`, the kind 18 entry, the
common textures as `fx::setup` does) and starts the element with `fx::start_placed(.., Bind::Owner(body), Placement::default())`, as the object
effects of the other owners start. `kh2 pax limit/us/donald2.bar` lists six elements; element 4 is bound to bone 38 with flags 0x5, and it
is the one Donald 2's script asks for. Harness (`KH2_LIMIT=344`, tt04): `limit visual: donald2.bar PAX holds 6 elements`, `effect element 4
of donald2.bar on handle 0x7a100200: started`. The PNG at `KH2_SHOT_AT=5.5` looked at shows small yellow sparkles around Donald (bottom of
the frame) that the shot at 4.0 does not have. The answer of 2.59 is 0 (the original's is the effect handle; no script is known to use
it). [inferred] That the app's bound-effect placement equals the original's for this element. Host 2.50 stays the event start of "Limit camera".

The requests are now `Request::Motion { args, who }` and `Request::PaxElement { args, who }` (`MotionRow` is gone). Test
`hosts_2_47_and_2_59_name_their_participant_from_the_context`.

### What is still open

- **Host 7.28 / 7.30** (`FUN_002f3bd8`): `FUN_00104fa8(0xa00)` and `FUN_0016e208(0x5c3)`; the latter falls through to `FUN_001d1470(0x5c3)`, the
  objentry row id 1475, which reads `PRIZE` (type 22, sub type 5, flags 1, no animation file). `FUN_002f3aa0` -> `FUN_001c01c0(obj, row, 0, 1, 0,
  0x80, 0x100)` builds an object of 0xa00 bytes with the vtable at `0x371528` (`DAT_00371518`). The vtable's methods and `FUN_001c01c0` were not
  read, so what it draws is unknown (a screen effect is only the old guess): not drawn, answered with nothing. Searches: `mips_scan --addr
  0x371528`, `--addr 0x371518`, `--jal 0x2f3bd8` (reached through the host table only).
- **The HUD gauge animation** (`FUN_001b3a70`, only Trinity's command 0x1e4): the name at `0x37ba58` reads `comd`, not `comb` / `gage` (the Trinity
  BARs hold `comd` kind 24 and 25 entries besides `comb` / `gage`), and the call goes through the command deck object (`FUN_001bbb18` on the
  player's `+0xb44`, then `FUN_001d2848` / `FUN_001d2918` at that object's `+0x94c`), which is `hud_menu.rs` / `menu_*.rs` territory, outside this
  package. Not drawn. `TaskState::animations` stays the stand-in the script's wait on 2.63 needs.
- **The task's end** (`FUN_001b3970`): not over while the thread's state word has bit 8 (`FUN_0016f038(thread, 0) & 8`), nor while either gauge
  object (task `+0x68` and `+0xa8`) holds a running animation (`FUN_001cfb30` / `FUN_001cfd90`: the word at the object's `+0x30` through
  `FUN_0014f658` / `FUN_0014f7b0`), nor while a party object has the state `FUN_0019a440` tests (`FUN_001c7c30`, objects 0 and 1), nor while
  the animation count `+0x3c` or the word `+0x24` is non-zero; `FUN_001b3910` sets bit 4 of `+0x40` when the thread returned. The gauge
  animations that would hold it open are not in the app, so the end stays the countdown plus the thread's tail.
- **`KH2_LIMIT_CAMERA` default.** Not flipped. Participants render only for Sora, Donald and Goofy; the files with a camera event
  (Auron, Beast, Mulan, Aladdin, Sparrow, Simba, Jack, Tron) name characters the app does not spawn, Goofy's and Donald's files have no event, and
  the Trinity files were refused by the world in tt00 / tt04 (`WrongWorld` for `trinity_wi`, `AbilityMissing(57)` for `trinity_zz` until the ability
  list holds 57, then no event). No PNG supports a default of on, and the frame the keys are placed in is still [inferred].

## Auto Limit (WP-AL1) [V unless marked]

The ability (item 417, bit 129 of the partner's own block) lets the partner's limit be offered to Sora as a reaction command.

### The original's rule

- **Caller.** The player controller `FUN_0019c670` (each frame) calls `FUN_0019d430(controller, player)` last, when: no native
  candidate was taken (`FUN_001ba6c8`: the controller's candidate record `+4` has command 0), no limit task exists (`FUN_001b3c00`),
  the player has `+0x588` bit 0x80 and not 0x2000000, `+0x160` bit 0x4000 is clear, `FUN_001baee0` holds (a mode word at
  `*(*(player + 0xb44) + 0x974)` that is 0 or 6), and the two other offer sources `FUN_0019d0b8` (mode 6 with a member whose
  `+0x7f8` bit 4 is set, offers through `FUN_0019d1a8` with the table at `0x37b3a8`) and `FUN_0019d048` (the status block's
  `FUN_001a0be0`, table `0x37b3a0`) answered nothing. So Auto Limit is the last resort: any other offer wins.
- **`FUN_0019d430`** asks `FUN_001d25e0(player, wanted)`, `wanted` being the halfword at `*controller + 0x26`; a non-null answer
  (a command row) is stored with `FUN_0019c3c8` as the candidate, with the same `DistRange` test as any candidate (the target
  record is the player himself, distance 0).
- **`FUN_001d25e0`.** It answers null unless `FUN_001d2460` holds: the player exists with a status block, no limit task, not
  (`DAT_01d5e368 != 2` and word `0x35feb8` non-zero), `FUN_00140fe0` false, **the battle flag** (`FUN_001903d8`) set, and the status
  block's `+0x1c0` float not above 0 (`FUN_001a05d0`). Then it walks the `limt` table (`DAT_01d6a948`, `0x40` bytes a row, in table
  order). A row qualifies when: its `Character` byte is not 1; `FUN_001d1ce0` (the port's `usable`) and `FUN_001d1ea0` hold; the
  member object of `Character` has bit 129; the member stands within 500.0 of the player (the position difference with y zeroed,
  `<=`); and `FUN_001ba2c0(command row, player)` is below 4 (Execute 81 is `FUN_001b3e38`: 0, or 2 for too little MP; then the
  command's Flags and `DisableForm` tests, `FUN_001b70b8` / `FUN_001b7290`, make it 5). The first qualifying row whose command is
  `wanted` is returned at once. Otherwise, with `n` qualifying rows (duplicates are kept), `n = 0` answers null and else the
  generator steps once (`DAT_003716a0 = DAT_003716a0 * 0x10dcd`) and the answer is the row at index
  `word / ((2^32 + n - 1) / n)`: `FUN_00325b38` is an unsigned 64-bit divide, called with `(n - 1) + 2^32` and `n` for the step
  (0x1d27bc), then with the word and the step (0x1d27cc); the word is zero-extended by `dsll32` / `dsrl32` at 0x1d27a0 and 0x1d27a4,
  so the index is below `n` for every word (the earlier worry of a sign-extended word is void).
- **Accepting** is the Execute 81 path of any limit command: `FUN_0019c670` pushes the candidate as input action `0xcb`, the player
  class handler `FUN_001cf030` case `0x51` runs `FUN_001b28a0`. No use counter and no event 0xb: the limit task starts as in "The limit
  task (WP-LM1)".

### What the app does

- `limit.rs`: `auto_limit_candidates` is the row walk (`Partner`, `party_for`, range `AUTO_LIMIT_RANGE`), `auto_limit_pick` the choice,
  `LimitRng` the generator (the app keeps its own copy of `DAT_003716a0`; `KH2_AUTO_LIMIT_SEED=<n>` fixes it), `StartLimit` the
  message an accepted offer sends; `try_limit` takes it, runs the usual rules with the party the offer saw, spends the MP and starts the
  limit exactly like `KH2_LIMIT`.
- `reaction_cmd.rs`: `auto_limit_offer` (after `offer`, before `accept`) fills slot 1 when the slot and the townsperson/save point
  candidates are empty, a battle is on (`sora::Battle` or `KH2_DRIVE_BATTLE`), no limit runs and Sora stands. The partners are the
  living `PartyMember`s (`neo` is the party id), their abilities the equipped items of the saved record (`Progress::member`). The
  command's Flags and `DisableForm` come from `reaction_cmd::flags_block` (the tail of `status`, now shared). A status 2 candidate
  (too little MP) is offered with `Status::NoMp` and refused on accept. The HUD's reaction capsule shows it like any other offer
  (`hud_menu::mirror_reaction`).
- **The wanted id** is the command shown last tick [V, WP-AL1b]. `*controller` is the persistent command object; `FUN_0019c0f0`
  (each frame) builds a temporary controller (`FUN_0019c380`), runs `FUN_0019c670` on it (which ends in `FUN_0019d430`) and, when the
  temporary one found a candidate (`FUN_001ba6c8`), copies the record into the persistent object with `FUN_0019bf58` (`sw` at
  `+0x24`, whose upper half is the command id at `+0x26`). So `+0x26` is last frame's shown command, and a still-qualifying shown limit
  keeps winning without a draw. `FUN_0019c0f0` also forces the shown status byte (`+0x29`) to 5 while the player's form field
  (`+0xb50`) is 5 (Anti), or `*(*(player + 0xc) + 8) & 0x8000`, or `player +0x168 & 0x80`: the port does the Anti case (`Status::Blocked`).
- Harness: `KH2_AUTO_LIMIT=2,3` treats Donald and Goofy as holding Auto Limit and every limit ability of their rows;
  `KH2_REACTIONLOG=1` prints `reaction: Auto Limit offers command ...`, `reaction: accepted limit command ...`, then `limit: ... started`;
  accept key R (`KH2_REACTION_PRESS=<s>,..`); `KH2_DRIVE_BATTLE=1` holds the battle flag without an enemy.
- Tests: `auto_limit_needs_the_partner_within_500_horizontally`, `auto_limit_offer_gating`, `auto_limit_pick_rule_with_a_fixed_seed`,
  `auto_limit_pick_matches_the_games_divide` (kh2-ee runs `FUN_00325b38` as the call site does; skipped without a disc) in `limit.rs`.

### Gaps (addresses searched)

- **Gates ported (WP-AL1b), `limit::StartGates`, used by the offer and again when an accepted offer starts (`try_limit`):**
  Sora KO or the Viewer (`FUN_0019ae90`: player `+0x7f8` bit 4); a summon's arrival or leave (`FUN_001d2460`: refused when the word
  `0x35feb8` is non-zero and `DAT_01d5e368` is not 2, so an active summon, state 2, does not refuse; `FUN_001c6168` sets bit 0 of
  `0x35feb8` and state 2 at the start, `FUN_001c6578` sets bit 3 of `0x35fec0`, `FUN_001c65b8` clears it; the app's `SummonState::sequence`
  and `state`); the menu open or closing and an event playing, both through the global command lock (`DAT_0035d268` bit 2, which
  `FUN_001ad378` sets at an event's start: `FUN_001b70b8` makes every command without Flags bit 5 status 5; `reaction_cmd::flags_block`
  with `commands_locked`) [inferred: that the pause menu sets the same bit]. `FUN_001b3e38`'s `0x35fec0` / limit-task test is
  `Party::busy` (`Refusal::Busy`). Tests `start_gates_refuse_down_menu_event_and_summon_sequence`, `a_busy_party_is_not_available`.
  The coordinator's "summon out" gate is refused only for the arrival and the leave: with the summon active the original lets the offer
  through (the party members are not on the field then, so no candidate exists anyway).
- `FUN_001d2460`'s other words: `FUN_00140fe0` is `DAT_00355884 != 0 && DAT_00355880 == 7` (a mode pair; writers `FUN_00140cc8`,
  no caller found that names 7 [GAP]); the MP-charge float `+0x1c0` (`FUN_001a05d0`) is the status block's MP timer the app does not
  keep for Sora's offer. `FUN_0019c670`'s `DAT_0035d268 & 0x4000`, player `+0x160` bit 0x4000, `+0x588` bit 0x2000000, `FUN_0019ae90`'s
  other bits (`+0x7f8` bit 0x400 clear, `+0x108` bit 0x40, `+0x160` bit 0x8000000, `FUN_0019b088`, `*(*(player + 0xc) + 8)` bit 0x2000), the
  mode word of `FUN_001baee0`, the sources `FUN_0019d0b8` / `FUN_0019d048` (they can only make the original offer something else), and
  the revive thread `FUN_001ad9a0` (sets `0x35feb8` / `0x35fec0`; `SummonState::revive` is private) are not modelled [GAP].
- `FUN_001d1ea0`: its first part asks whether the limit's BAR is a loaded resource (`FUN_0016fd98(FUN_001d1ff8(row)) == 2`, then the
  row's `+0x24` resource state `FUN_001a7d48`); the loader that asks for it was not found, the app takes it as loaded [approx]. Its second
  part needs both field companions `FUN_0017d030` (alive object, `+0x160` bit 0x4000 clear, no object whose `+0x570` is the member) and every
  participant that is a party member (`+0x588` bit 0x4000) `FUN_0017d0a8` (that plus `+0x7f8` bit 4 clear, not KO): the KO part is the
  port's `present` list; the `+0x160` / `+0x570` conditions are not modelled [GAP]. A row with `Character` 0 is skipped (the original would pass a null member
  to `FUN_001768a8`) [inferred]; the disc has none.
- The limit itself still has no task body (see "What is still missing"), so an accepted offer starts what `KH2_LIMIT` starts: MP spent,
  BAR read, `LimitStarted`, script events, the count-down.

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

## Harness press timing (RX1)

The Soldier's command 34 offer lasts about 0.13 s (8 ticks: the range trigger 20 covers those frames and the command manager clears slot 1 every tick, FUN_001bb368). A `KH2_REACTION_PRESS` list with one press per second almost never lands inside it; the `reaction-34` scenario needs presses every 0.1 s or denser (`100..400 / 10`). With that, the log has `reaction: accepted command 34` and `event 11` (the press is consumed on the next fixed tick whether or not an offer exists).
