# Limits and reaction commands (work package 2C)

What the game does for reaction commands and limits, what `crates/kh2-app/src/reaction_cmd.rs` and
`limit.rs` implement, and what is open. It builds on `docs/player-data.md`. **[V]** verified (read in the
game's code or checked against the disc), **[I]** inferred, **[GAP]** not found or not ported. Function names
are addresses in `SLUS_210.05`. No game values are stored in the repository; the rows below are read from the
disc by the tests and the app, and `kh2 table limits_ex` / `kh2 table cmd` print them.

## Reaction commands

### The offer [V]

- A range trigger of id 20 in a motion calls `FUN_00188ad0(object, params[0] = command id, params[1] = position
  index or 0)` (`FUN_00188030` case 0x14). The trigger covers a frame range of the motion; while the object plays
  a frame inside it the offer is made again every tick.
- `FUN_00188ad0` needs the player to be in the object list and not in the two states `FUN_0019a440` tests
  (object flags 0x80000 and 0x10000000 at +0x108; their names were not found, the port reads "downed or dead").
  It builds a target record for the offering object (the object itself, or the object at +0x23c when the party
  flag 0x80 and object flag 0x4000 are set) and writes the command into slot 1 of the player's command manager
  with priority 100 (`FUN_001bb3e0`; an existing entry with a higher priority is kept). The call through the
  offering object other than the player is wrapped by `FUN_001bb3a8`, which refuses while player flag 0x2000000
  is set. It also sets player flag 0x20000 and stores the command id at player +0x50c.
- The manager clears all its slots every tick (`FUN_001bb368`), so the prompt exists only while the trigger
  covers the frame.
- The slot status (`FUN_001ba670` -> `FUN_001ba2c0`): a reaction command has Execute equal to its own id, which the
  Execute switch does not list, so the status is 0 (available) unless the command flags (`FUN_001b70b8`:
  Flags bit 1 with `FUN_0019aa28`, bit 16 needs the battle flag, bit 5 ignores the global lock) or the
  `DisableForm` mask of the player's form (`FUN_001b7290`) refuse it (status 5). Then `FUN_0019c3c8` marks it too
  far when `DistRange` (cmd +0x28, game units) is non-zero and the horizontal distance from the target to the
  player is larger.
- The controller (`FUN_0019c670`) pushes slot 1 as input action 0xcb every tick it is available (`FUN_0019c520`
  refuses when `FUN_001b71b0` says the player's object type or form cannot take reaction commands: form field 0xb,
  or an object type test against Flags bits 0x400, 0x800, 0x1000, 0x2000 or 0x8000).

### Accepting [V up to the hand-over]

Triangle (the port: R, or `KH2_REACTION_PRESS`) runs the command through `FUN_00177b18`, which sends Execute
0x1e, 0x1f, 0x20, 0x37 and 0x7f to their own handlers and everything else to `FUN_00177c20`:

1. increment the saved use counter at slot `ReactionCount` (capped at 999, only if non-zero and the journal is
   active);
2. `CmdAction` 1: stop the player if he is moving; 2: clear the player's velocity and call the command target's
   handler; 0 and anything else: nothing on the player;
3. hand the command to the offering object: `FUN_0016b420(target, player, command id, record)` posts script
   event **0xb** with three arguments to the target's BDX thread (`FUN_0016c620`).

So the **command's effect is whatever the target's script does with event 0xb** (BDX event ids are decimal in
`kh2 bdx`: 0xb is "event 11") and with the motion it then plays on both objects. The port writes
`ReactionAccepted { cmd, pos, target }` and logs it; `enemy_ai.rs` does not deliver event 11 (its gap list).

**Soldier's script has an event 11 handler** (`kh2 bdx M_EX010`, entry at 0x1e) [V from the disassembly]: it pops
three values, compares the middle one (the command id, the second argument) with **34**, and only then calls a
routine (0x39) that fills a 16-byte vector from two host/library calls (80, 90 and a position) and stores it at
offset 0x40 of a record [what the vector is was not worked out; a position for the player is a guess [I]]. Command 34 is a reaction
command row (Mark 2, `ReactionCount` 1, message 33949). Any other command id falls through and does nothing.
**Shadow's script has no event 11.** Nothing offers command 34 to Soldier through a motion trigger (scan below),
so the offer is not made by a motion; it is a lead for whoever finds the other callers. The rest of command 34's
effect (the player's and Soldier's motions) is not in this handler [GAP].

### Where reaction commands come from on this disc [V by scan]

A scan of all 1,157 `.mset` files on the disc (`cargo test -p kh2-app real_disc_scan -- --nocapture`, needs
`local/kh2idx.txt` for the file names) finds range trigger 20 in exactly **two** motions:

| motion set | slot / tag | frames | command | position index | playable in the app |
|---|---|---|---|---|---|
| `obj/P_EX200.mset` (objentry 91, party class) | 772 / A122 | 60 to the motion end | 457 | 2 | no: the object is a player-class body, not an enemy; the app has no such object |
| `obj/P_EX220.mset` (objentry 792) | 772 / A122 | 60 to the motion end | 457 | 2 | no, same reason |

Command 457 (Execute 457, message 48047, Mark 6, Require 455, Priority 255, Flags 347, CmdAction 0, no
`DistRange`) is offered by the **player's own** motion here (the `iVar2 == param_1` branch of `FUN_00188ad0`
calls `FUN_001bb3e0` directly).

**Shadow (`M_EX020`) and Soldier (`M_EX010`), the only enemies the app spawns, offer no reaction command**
(unit test `app_enemies_offer_nothing`). No enemy motion set on the disc offers one through trigger 20.

The disc has 124 reaction-type command rows (`CmdAction` 1 or 2, Execute equal to Id; Mark 1 to 8, grouped by the
capsule art in `docs/hud.md`; list them with `kh2 table cmd 700`). They are therefore offered by something other
than a motion trigger. The offer function has these other callers in the executable (found by scanning for the
jumps to `FUN_001bb3a8` and `FUN_001bb3e0`) [V that the calls exist, GAP for what each is]:

| call site | enclosing code |
|---|---|
| 0x1734f0, 0x173618 | inside the BDX host function range (0x1733xx to 0x1736xx, host group 1); the unnamed host functions 1.113, 1.117, 1.118 (13, 5 and 3 uses on the disc) are the candidates: a script can offer a command from its own code |
| 0x188bb0, 0x188b60 | `FUN_00188ad0` itself |
| 0x1bb504 | the command manager (`FUN_001bb4b0`) |
| 0x1cd710, 0x1ce398 (`FUN_001ce240`), 0x1cf01c (`FUN_001cef10`) | player code: the Auto-form and Final-form-ready prompts |
| 0x2db858, 0x2db8f4, 0x2db948, 0x2e0434, 0x2e44e4, 0x2f1b28 | code outside the decompiled set (0x2db670, 0x2e0348, 0x2e44a8, 0x2f1988): map and mission objects |

The next worker should name host functions 1.113, 1.117 and 1.118 (read the exe at the call sites 0x1734f0 and
0x173618) and list which BDX scripts call them; that gives the enemies and bosses that offer commands, and the
event 0xb handler in the same scripts is the command's effect.

### What the port does

`reaction_cmd.rs` (`ReactionPlugin`, one line in `main.rs`):

- every tick, for each living enemy, reads the active range triggers of the playing motion (`Animator::triggers`
  at `gframe`) and takes the trigger-20 offers; the last offer wins (equal priority replaces). The slot is cleared
  and rebuilt each tick like the game's.
- the status follows the rules above (`status()`: command flags, form mask, `DistRange`); an Execute value that the
  player's availability switch handles (1, 2, 4, 5, 0x37, 0x48, 0x51, 0x67, 0x68, 0x7f, 0x13c, 0x14a, 0x1c4) is
  reported as `NotPorted` instead of guessed.
- the key (R) or `KH2_REACTION_PRESS=<s>,...` accepts an available offer: use counter, `CmdAction` 1 or 2 clears
  Sora's velocity, `ReactionAccepted` is written.
- resources: `ReactionOffer(Option<Active { cmd, pos, from, status, message }>)` (the HUD capsule input),
  `ReactionUses`.
- harness: `KH2_REACTIONLOG=1` logs offers, withdrawals, accepts and refusals; `KH2_REACTION_FAKE=<command id>` makes
  every living enemy offer that command (nothing on the disc's spawnable enemies offers one, so this is the only
  way to exercise the path).

Sample run (`KH2_REACTIONLOG=1 KH2_REACTION_FAKE=457 KH2_REACTION_PRESS=9 cargo run -p kh2-app -- tt00`):

```
reaction: offered command 457 (message 48047) by 1052v0, status Available
reaction: accepted command 457 (Execute 457, CmdAction 0, counter slot 0 = 0) from 1052v0; delivered as script event 0xb to the offering object (target side not ported)
```

Not done: the player-side motion paired with a command (it comes from the target's script), the Triangle button
prompt art (needs `CommandMenu`, below), `FUN_001b71b0`'s object-type test, the party flag redirect of the target
record, the flags `FUN_0019aa28` and `FUN_0019a440` read.

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

## Hook points needed from the orchestrator

1. `hud.rs`: make `hud_menu` visible and mirror `ReactionOffer` into `CommandMenu.reaction` (above).
2. `enemy_ai.rs`: deliver `ReactionAccepted` as script event 0xb (arguments: the player handle, the command id, the
   target record) to the BDX thread of `target` as event 11 (Soldier's script handles command id 34 only, see
   above); useless until something offers command 34.
3. `sora.rs` (or the drive plugin): pass the player's form id into `reaction_cmd::Ctx.form` (now 0) and the battle
   flag; the DisableForm mask of the reaction rows hides some commands in forms.
4. A shared `LimitParty` source once companions exist (it is a resource today).
