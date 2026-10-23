# Hit-stop (hit lag): what the game's code does and does not do

Work package WP-1G. Question: when Sora's attack connects, the attacker and the victim are
remembered to freeze or slow down for a moment. Is there a rule in the original game, and what is
it? Short answer: **no hit-stop rule was found.** The hit path of the main executable and of all
four code overlays has no route to any mechanism that freezes or slows an actor. The mechanisms
that do exist (per-actor stop slots, a global time scale, a per-actor motion rate) are driven only
by scripts and by the command cut-in, never by an attack landing. Details and the exact list of
what was checked follow, so that nobody has to repeat it.

Status words: **verified** = read directly in the code or data, **inferred** = follows from it but
a name or meaning is a guess, **unknown** = not found. Addresses are in `SLUS_210.05` (USA)
unless they start with `0x1f` (overlay code, loaded at 0x1f18000; the four overlays of `OVL.IMG`
all load at that base). Nothing here is copied code. The user's report that the original freezes on
hit is treated as a claim; the code does not support it, but absence in static analysis is not
proof (see "What is not ruled out").

## How an actor's time advances (verified)

All actor time goes through one routine, the per-actor update `FUN_001b74e0`, which is called from
the world's actor loop (call sites 0x1b78f8 and 0x1b7928, in the function at 0x1b7730). It loads
the global frame delta `DAT_00356f40` once and uses it for everything below. In order:

1. If actor flags `+0x108` bit 0x80000 or bit 0x10000000 is set the actor is skipped and marked.
2. Virtual method +8 (pre-update), unless actor word `+0x7f8` bit 0x80 is set.
3. `FUN_001d5a28(actor+0x518)`: counts the actor's **stop slots** down by the delta (below).
4. The "frozen" test `FUN_0019b088`, or the global `DAT_0035d268 & 0x20`. If either holds the
   actor does not advance: only the motion pose is refreshed (`FUN_0017e6c0`, no time step) and
   no movement is applied. Frozen means any of:
   * bit 0 of the stop word at `actor+0x518` (`FUN_001d5b20`),
   * `FUN_0019af28`: actor word `+0x588` has neither bit 4 nor bit 0x40 and the global state
     `DAT_01d5e368` (`FUN_001c6918`) equals 1,
   * actor flag `+0x108` bit 0x400 (`FUN_0019b100`),
   * `FUN_002c56c0`: a movie/sound-library condition on `DAT_0036f6a4`.
5. Otherwise: virtual methods +0xc and +0x10, then, unless flag `+0x10c` bit 0x400000 is set,
   the **motion step** `FUN_0017e3b8(delta, actor+0x140)`, then +0x14/+0x18, the motion-trigger
   scan `FUN_0017e6c0`, and the movement integrator `FUN_00181e00`.

The motion step advances motion time by `delta * rate`, where the rate is the float at motion
player offset 0xf4 (actor `+0x234`). The motion player constructor `FUN_0017d768` stores 1.0
(`DAT_0037a584`) there; the actor constructor at 0x1b8a38 also stores 0 to `+0x234`, and which of
the two runs last was not established (inferred: the player's 1.0 wins, otherwise no motion
would ever advance). Movement in `FUN_00181e00` is also scaled by `DAT_00356f40` and
`DAT_00356f48`. The delta is built each frame in `FUN_0014cdd0` (and the older `FUN_0014cbf8`):
the number of vsync frames elapsed (capped at 6.0), or a fixed 2.0 when `DAT_00356f50` is 0,
multiplied by `DAT_00356f3c`. So there is exactly one global time scale, `DAT_00356f3c`, and
exactly one per-actor rate.

## The three freeze/slow mechanisms and who drives them (verified)

**Stop slots (per actor, timed).** Four slots at `actor+0x518`: a u16 flag word (OR of the flags
of the active entries), then per slot an id and a float time remaining (id at `+4+8i`, time at
`+8+8i`). `FUN_001d5958(state, id, time in f12)` adds (or refreshes) a slot, `FUN_001d59a8` removes
an id, `FUN_001d59f0` clears all, `FUN_001d5a28` decrements the times by the delta and drops
expired slots, `FUN_001d5ab0` recomputes the flag word from the `stop` table of `00battle.bin`
(row = `Id u16, Flags u16`, found by `FUN_001d5920`). A time of 0 or less is never counted down
(the slot stays until removed). Wrappers: `FUN_0016bea0(actor, id, time)` (refuses when actor word
`+0x7f8` bit 4 is set), `FUN_0016bed8(actor, id)`, and the all-actor forms `FUN_0016bef0(id, class
mask)` and `FUN_00184b40(id)` (add with time 0), `FUN_0016bf78(id)` (remove from all). `FUN_00186580`
sets `+0x7f8` bit 4 (immune to stops) and clears all slots; it is called for the party class on
KO (`FUN_00175f10`) and from `FUN_001865a8`/`FUN_00186568`.

The `stop` table (USA, 15 rows): ids 1, 4, 8 to 13 and 17 have flags 1; ids 5, 6, 14, 16 have
flags 3; ids 2 and 15 have flags 7. (Ids 3 and 7 do not exist.) Flag meanings, from the readers:
bit 0 freezes the actor (above); bit 1 makes the reaction dispatcher `FUN_00184e80` return without
reacting (`FUN_001d5b30` at 0x184eac), so a stopped actor with this bit ignores the reaction part of
a hit; bit 2 is read by `FUN_00186638` (an "untouchable or busy" test used by `FUN_00175ad8`).
There is no duration in the table; every caller supplies its own. A new motion request on a stopped
actor clears all its stop slots (`FUN_0017dd38`, via `FUN_001d59f0`), so a stop does not survive
the next motion change.

Callers of the stop functions that exist anywhere (main executable and all four overlays,
searched for `jal`/`j` to 0x16bea0, 0x16bed8, 0x16bef0, 0x16bf78, 0x184b40, 0x1d5958, 0x1d59a8,
0x1d59f0, 0x186568, 0x186580, 0x1865a8, and by data reference):
* the command cut-in `FUN_001c6168` (a command id is passed; ids 0x59 and 0x132 are special-cased)
  sets `DAT_01d5e368 = 2` and calls `FUN_001c6578`, which adds stop id 2 (flags 7) to every actor
  except classes 0, 1, 5 and 19 (mask 0xfff7ffdc); `FUN_001c65b8` removes it. Inferred: this
  freezes enemies while a limit, summon, drive or reaction command plays.
* `FUN_001b35e8` (called from `FUN_001b27b0` and `FUN_001b28a0`) adds and removes stop id 1 on every
  actor under an event-like object's own flags (bits 0 and 1 of its word `+0x40`).
* The scripts' host functions: group 6 index 6 (add, args actor, id, time), 6.7 (remove), 6.46
  (add, two actors), 6.50 (add to all actors of a class mask), 7.1 (remove an id from all),
  10.11 (add, same wrapper shape as 6.6). Across the whole
  disc only 6.6 and 6.46 are ever called: 6.6 by three mission scripts (`msn/us/TT04_MS301.bar`,
  `TT05_MS403.bar`, `TT05_MS404.bar`) and 6.46 by two boss scripts (`obj/B_EX120.mdlx`,
  `B_EX120_HB.mdlx`). Nothing else.
* Within the hit path (see the call-graph result below) the only stop function reachable is
  `FUN_00186568`, and only through the KO route.

**Global time scale.** `FUN_0014cbc0(float)` stores `DAT_00356f3c`; `FUN_0014cbd0` resets it to
1.0. Callers of the setter: only the script host function `FUN_0016d7d0` (group 0 index 27); the
resetters are the area load/unload routines (0x1048e0, 0x196170, 0x196348). No BDX script on the
disc calls host 0.27, and no overlay touches `DAT_00356f3c`.

**Per-actor motion rate (`actor+0x234`).** The only write after construction is the script host
function at 0x173260 (group 1 index 107). It is called by 55 scripts, all enemy and boss AI
(`B_*`, `N_*`, `F_WI380*`, `M_BB010_*`, `P_BB000_BTL`); Sora (`P_EX100`) has no script at all. This
is an attack-speed knob for bosses, not a reaction to being hit.

Also checked and not related: the actor flag `+0x108` bit 0x400 and `+0x10c` bit 0x400000 are
only set by the script host function `set_flag` (bit number from the script); no native code in
the main executable or overlays sets them (searched for the read-modify-write on both words). The
floats at `+0x82c` and `+0x8d0` (tested at the end of `FUN_001b74e0`) are initialised to 1.0 and
read by drawing code; they are scale, not time. Actor `+0xf0/+0xf4/+0xf8` (set by `FUN_00178108`,
`FUN_001a1044`, `FUN_00188030` case 0x18, counted down by `FUN_001c1660`) are a timed locomotion
state with its own motion (ids 0xcb and 0xcc), not a freeze.

## What was ruled out (verified unless stated)

Static call graph over the main executable (every `jal`/`j` edge, functions delimited by the
decompiler's function starts). Starting from each of these and following all edges, no stop add,
time-scale or rate function is reachable: `FUN_0018cf50` (hit dispatch), `FUN_0018ca78` (wall
hit), `FUN_0018c6d0` (hit record builder), `FUN_0018d090` (hit effects), `FUN_0018d430` and
`FUN_0018d808` (attack clash), `FUN_00185a68` (damage), `FUN_00184e80` (reaction dispatcher),
`FUN_001a2cc0` (attack volume sweep), `FUN_00178748` and `FUN_001767a8` (party attacker callback,
which only gives drive and sends script event 0xf), `FUN_001754a0` and `FUN_00175668` (party
clash and wall methods). `FUN_001b21f0` and `FUN_001b2378` (victim receive) reach
`FUN_00186568` only through the over-wide edge into the KO path, which is a graph artefact: the
decompiled body of `FUN_001cd1c0` does not call it. Virtual calls are not edges in this graph; the
party class vtable (0x35c9a8) methods that run each tick were read directly: +8 (`FUN_00177110`),
+0xc (`FUN_00177788`), +0x10, +0x14, +0x18 (`FUN_001779e8`), +0x20 (`FUN_001772b0`), none has a
hit-lag counter (their timers are guard, drive and gauge timers).

Per-actor update, motion step, motion trigger dispatch. The frame trigger dispatcher (0x188498,
types 0 to 31) and the range trigger handler (`FUN_00188030`, types 0 to 0x35) were read in full.
None of the frame triggers stops anything: types are effect start (1), footstep (2), sound (8),
voices, state requests through `FUN_0016b698`, script events 0x10, 0x11, 0x1b and similar,
fade/tint, and 31 which calls `FUN_001d3088` (vibration, inferred from OpenKH). Range triggers
only set bits in the motion player's flag words at `+0x20`, `+0x24`, `+0x28` (hit volume windows,
combo windows, gravity, turning, invincibility) or call small helpers. Range trigger 30 (OpenKH
guesses "freeze?") sets motion flag 0x1000000, which the game reads as part of its "airborne" test
(`FUN_0019aa28`, flags 0x20 and 0x1000000); it does not freeze anything.

The camera. `FUN_00140a80` (modes 0 to 10) and the helpers it calls read camera data and the
player's position only; no hit input. See `docs/systems/player-camera/contract.md`.

The attack volume. `FUN_00197498` and the volume constructor path (`FUN_0019e350`,
`FUN_001a2640`) pass the trigger's fourth parameter as a hit-group id, not a duration. The
`atkp` fields and their readers were already listed in `docs/systems/combat-effects/hit-reaction.md`; nothing there has
a duration that is applied to the actors. The only hit-time callbacks are the volume's script
event 3 (`FUN_0018dbd8`, started with `FUN_0016c550`; used for clashes and hits) and the party
event 0xf; no script that handles them calls any stop, time-scale or rate function (see above).

The `sstm` record (92 words, `docs/systems/combat-effects/player-data.md`): the integer-valued fields at 0x08c, 0x090,
0x0a4, 0x11c, 0x130, 0x134, 0x148, 0x14c and 0x164 were traced to their readers (drive and stat
growth, `FUN_001a04b0`, `FUN_001c0f40`, `FUN_001d4180`); none is a frame count for a freeze.

Effects. The 245 particle program names in the executable (`ppp*`) include no time, stop or
actor-touching program. Names containing "Hit" are particle-versus-ground checks. (Names only;
handler bodies were not all read.)

Overlays (enemy and battle code, four files). Searched with a scan of every `jal` and `j` target
and every load/store with the relevant immediates. No overlay calls `FUN_0016bea0`, `FUN_001d5958`,
`FUN_001d59f0`, `FUN_0016bef0`, `FUN_0016bf78`, `FUN_00184b40`, `FUN_00186568`, `FUN_00186580`,
`FUN_0014cbc0`, `FUN_0014cbd0` or the rate setter at 0x173260. No overlay references
`DAT_00356f3c`. The stores to offset 0x234, 0x518 and 0xf4 in overlay code belong to unrelated
objects or are stack and overlay-global stores (read for overlay 0 at 0x1f739bc and 0x1f70798 and
overlay 3 at 0x1f68658, 0x1f68820 and 0x1f806a0; the remaining few, such as the 0x518 store at
0x1f1f840, were not read individually). Overlays can still
override virtual methods, but every call from them into main-executable freeze functions would
show up as a `jal`, and there are none.

Script data. Every BDX script on the disc (1,244 that parse; `docs/systems/script-host/contract.md`) was scanned for the
host functions above: 0.27 (time scale) 0 scripts, 6.6 three, 6.46 two, 6.7, 6.50, 7.1, 10.11 none,
1.107 (motion rate) 55 boss/enemy scripts and no player script.

## Conclusion for the combat code

1. There is no hit-stop in the generic attack path of the original executable or its overlays as
   far as static analysis can show. Do not implement a duration for it and do not present one as
   the original's behaviour. If the user wants a hit-lag feel anyway, it is a **deviation**: write it
   down as such where it lives.
2. The feel of weight in the original is therefore expected to come from the data that was
   already found: the victim's flinch or blow motion and push (`docs/systems/combat-effects/hit-reaction.md`), the hit
   spark and hit sound, the attacker's own motion data, and the camera. None of these was
   measured visually here (no emulator run was done).
3. If someone does implement the real stop mechanism (for the command cut-in, or for scripted
   events), use the original shape: per-actor slot list with an id and a time remaining counted in
   the frame-delta unit, flag bit 0 freezing the actor's motion step and movement while its pose is
   still refreshed, bit 1 suppressing the reaction part of incoming hits, and clearing the slots on
   any new motion request. Order within the actor's tick: slot timers count down first, then the
   frozen test, then (only if not frozen) the motion step and movement. A slot time is in the
   same unit as the delta, which is 1.0 per 60 Hz vsync and normally 2.0 per logic tick (`FUN_0014cdd0`
   with `DAT_00356f50` equal 0; inferred reading).

## What is not ruled out

* Host function groups 3 and 9 are registered at run time (`FUN_0016ca10`) and have no static
  signature, so BDX scripts that call them were not analysed for calls into the stop functions;
  but any such registered function that stops an actor would still have to be a native function
  calling one of the stop wrappers, and no native `jal` other than those listed exists.
* Function pointers called indirectly with computed addresses would not appear as `jal`. Both the
  party vtable and the hit path were read for virtual calls, but the enemy-style classes' vtables
  (outside Sora's party class) were only checked through their wall and clash methods in
  `docs/systems/combat-effects/hit-reaction.md`.
* The ARD/mission scripts other than the three that use 6.6 were scanned with the BDX analyser
  only; other script formats (if any drive actors) were not.
* The scan for scripts that call `set_flag` with bit 54 (which would skip an actor's motion step
  through `+0x10c` bit 0x400000) produced no result and should not be relied on.
* No dynamic confirmation. A run of the hit path in `kh2-ee` was judged not feasible (it needs a
  populated actor world) and, for a negative result, would only show absence on one input.
* The user's memory of a freeze may come from the motion data of the attack and reaction motions
  (held frames, slow-in/slow-out), which is the game's own data and would play correctly once the
  motions are played at the game's own rate. Comparing a recording of the original with the port
  would settle it and is the cheapest next step.
