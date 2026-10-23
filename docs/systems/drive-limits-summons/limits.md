# Limits

[Contract](contract.md) | [Index](../../README.md)

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

