# Dynamic tracing in PCSX2 (WP-DYN1)

What the static scan (`local/tools/mips_scan.py`) cannot see, the running original shows: a write watchpoint on the field names the writing
function at once. This note is the method, then one section per target with the address, the run that showed it and what the app does with it.
No game data is in this file: only structure, addresses and the small rule constants the code needs. Everything is [V] (read in a run, run
described) unless marked [inferred]. "Run" means the user's USA disc in the patched PCSX2 of `local/tools/pcsx2-mcp` (docs/systems/foundations/pcsx2.md).

## Method

1. Start the patched emulator (`local/tools/pcsx2-mcp/PCSX2-MCP-v1.0.0-win64/pcsx2-qt.exe`, not the installed one: they share the data folder) with
   the USA image and a savestate: `-batch -statefile "<...>.p2s" -- "<iso>"`. The debug server listens on 127.0.0.1:21512 (`local/tools/dbg.py`
   sends one JSON request per argument; quote carefully in PowerShell, or call `dbg.call` from Python). PINE (`pine.py`) loads and saves states
   (`pine.py load 3`) and reads memory. PINE slot n is the file `.0n.p2s`; slot 0 is `.00.p2s`, a file of its own (the numbered ones are the
   user's: never save over them).
2. Watch a field: `set_memcheck` with `type` `write`, `action` `break`, then `resume` and poll `status` until `paused`; read `pc` (the store
   itself) and `get_backtrace` (its callers); `resume` again. A watch on a range costs nothing while nothing writes. `action: log` only counts.
   Write watchpoints on the same field of many objects are set together (one `set_memcheck` per address).
3. Find the objects: an execution breakpoint on a function that takes the object in `a0` and runs for every live object (`FUN_0019aa28`,
   0x19aa28) and read `a0` at each stop for a few dozen stops: that lists the object pool of the scene (about 10 objects in a battle).
4. A function that nothing in the saved scene calls is called by hand: `pause`, save the 32 registers, set `a0..` and `sp` (below the current
   stack), set `ra` to the interrupted pc with an execution breakpoint there, `set_pc` to the function, `resume`; at the breakpoint restore the
   registers and the pc. The game goes on as if nothing happened except for the function's own effects. This ran `FUN_001b51e0` (experience),
   `FUN_001be6b8` (bonus rewards) and `FUN_00149df0` (leave for the world map, which loads the map scene in about 10 s).
5. A store `sw r, 0x20(s1)` into an object field `+0x160` means `s1` is the object `+0x140` (the motion-state block of the object): offsets
   seen in a store are relative to its base register. This is why `--store-off 0x144` found nothing readable.

The user's savestates (not in the repository) used here: 2 (world byte 2, area 5), 3 (2, 0x14), 5 (2, 0x19) and 8 (4, 8) are battles (the battle
flag word `0x1c8a7d8` reads 2); 1 (2, 7), 6 (4, 0x0a), 9 (4, 8) and 10 (the title) are calm; 4 and 7 were not looked at. All are early in the game (Roxas's
level 1 to 7, experience 0 to 695); none is beyond that. The savestate must be near what is wanted: the prologue has no world map, no flying enemy, no linking enemy.

## 1. Max HP and MP growth: the bonus rewards (ST3 gap closed)

Writer: `FUN_001be500` (0x1be500), run for every `bons` row of a reward id by `FUN_001be6b8(rewardId)` (0x1be6b8). Not by a level-up.
Run: savestate 3/4 (Roxas, battle scene). First `FUN_001b51e0(200)` (experience 0 to 200) raised the level 1 to 4 and changed no record `+4..+7`
(write watch on 0x33fec4..0x33fec8: no hit). Then `FUN_001be6b8(3)` by hand: HP 30/30 -> 35/35 for Sora (`bons` reward 3, character 1: +5),
Donald 18/18 -> 22/22 (+4), the absent Beast 40 -> 75 maximum and 7 -> 42 current (+35); the count byte `0x340e68` 3 -> 4; the flag word `0x340e6c`
gained bit 3. The writes to the records came from `FUN_001a0030` (0x1a0044..0x1a006c, the copy back of the status block into the record).
A second call changed nothing. Rewards 1, 2, 7 and 21 gave: Donald +10 HP and +10 MP, Goofy one item and one accessory slot (record
`+0x12` and `+0x11`), Donald an armor slot (`+0x10`), Sora's two bonus items as abilities (`give` with the item's id, the obtained log
record `[2, 8, id, 0, 1, 4 ..]` with source 4 and the counter at `0x341518` raised), and the drive gauge `0x340c90`: 100/3/3 -> 100/4/4
(maximum raised first, then `FUN_001a0290(status, 0)` carries the full fraction into the points).

The rule (`FUN_001be6b8`): when the flag bit `id` (array `0x340e6c`, `FUN_001be7e8`) is clear, add 1 to the count `0x340e68`, set the flag,
find the first `bons` row of that id (`FUN_001be4b8`, rows of 16 bytes from table `+8`, sorted) and for it and every following row with the
same id: take the character's status block (`FUN_0019fe48(character, -1)`; a character with no block gets one built from the record, one
with a block keeps the block's current HP), add the row's HP to maximum and current (current held to the maximum; `FUN_001a0140`) and the
row's MP likewise (`FUN_001a0210` -> `FUN_001a0180`, not while MP is charging), add the drive column to the gauge maximum `+0x1b2` (the
copy back writes it only for the block that has the party block linked, i.e. Sora's), add the slot columns to the record (`+0x12` item, `+0x11`
accessory, `+0x10` armor), give the two items (`FUN_001a18d8(item, member, 0)`, logged by `FUN_001d0828(item, member, 4)`), queue the window
messages 0x8ae2..0x8ae7 (`FUN_001b55c8`) and the level window (`FUN_001b5718`, the count shown as "Bonus LV"), release the block
(`FUN_0019ffb8` -> `FUN_001a0030`). `FUN_00180450` then runs (the tutorial prompt check).

Callers (`mips_scan --jal 0x1be6b8`): `FUN_001b25d0` after a defeat's experience, with the short at `+0x5a` of the defeated enemy's `enmp` row
(`BonusLevel`) when it is not 0 (it is 0 in all 177 rows of the USA disc: `kh2 table enmp`, so no defeat gives a bonus); `FUN_001c46a8` the same
for a second `enmp`-shaped row picked from a table (called through a pointer, not traced further); `FUN_001b95a0`, the end of a mission manager
(host 4.3 and 4.16, docs/systems/world-minigames/struggle.md "The end"), with the byte at `+0xd` of the mission header (`msn/us/*.bar`, `Header::unlock` in `struggle.rs`;
0 in every Struggle file). So the rewards come from the missions and scenes the app does not run. Port: `Progress::give_bonus`, `Progress::bonus_given`,
`Progress::bonus_level` and the defeat path of `progress::pay_defeats` (step 3 of `FUN_001b25d0`; harmless on this disc); the mission end is not wired
(`struggle.rs` only logs a non-zero header byte). Tests: `progress::tests::a_bonus_reward_raises_maxima_and_current_values_once` (numbers of this
run), `a_bonus_reward_raises_slots_and_the_drive_gauge`, `the_gauge_carry_matches_the_game`.
Gaps: the second caller; wiring a mission end that has a reward; the level window and messages; the live HP of Donald and Goofy while they stand (the record is raised, the
`PartyMember` keeps its old maximum until the next spawn); the drive gauge the app's `drive.rs` keeps (it does not read the record's
`+0xdd2`); [inferred] Sora's live HP rises by the row's HP like the block's does.

## 2. Host 1.77 (actor +0x144) and 1.59 (mover +8 bit 1, +0x160 bit 0x40)

Host 1.77 (`FUN_00172d60`) reads `*(*(*obj) + 4) + 0x144`, a word of the object's motion-state block (+0x140, section Method 5). Run: savestates
2, 3, 5 and 8 (battles; 7, 2, 12 and 8 objects found through `FUN_0019aa28`): `+0x144` was 0 in all of them, and a write watch on each
object's `+0x144` for 8 s of play saw no store. Nothing in these scenes sets it, so the app's constant 0 is what the original answers there.
The writer (if any) belongs to a state none of the savestates reaches (the script needs the enemy kinds M_EX750; the prologue has none).

Host 1.59 (`FUN_001729c0`): the object's `+0xc` is not a mover object but a descriptor of the mover state id (`FUN_0016b698(obj, id)` ->
`FUN_0016b648` -> `FUN_00179868(id)`; the constructor `FUN_00168d30` installs id 0). The descriptors are 16-byte entries from 0x1c88eb0
(`pointer, id, flags, 0`) written once by `FUN_00179880` from constants; the flag word `+8` was read live (all 32 entries). Bit 1 is set for the
ids 5, 8, 10, 11, 13, 15, 19, 20, 21, 24, 25, 27, 28 and 30 (bit 0, the "airborne" bit of host 1.57, for 1, 2, 4, 7, 9, 11, 12, 14, 17, 18,
20, 21, 26). Mover 5 is the dash of host 1.58; ids 0 and 1 (the constructor's and the jump's) have neither. In the 29 objects of the four
battles every mover was id 0 (flags 0x180). The writers of the bit are therefore not stores: it is a constant per mover id, and the answer
is true exactly while the object's mover id is in that list. The app keeps `enemy_host::MOVER_NO_GROUND` and answers it for id 5 (the dash,
tracked by `Obj::dash`) and id 0; the other mover ids are installed by hosts and classes the app does not model (gap). Bit `0x40` of
`+0x160` (the second half of the test): the stores that touch `+0x160` of an object in the battles are at 0x17e430 (an and-mask), 0x17e8b8
(`or 0x80`), 0x17e8e4 (`or 0x1000`, clear 0x2000), 0x17e978 (clear 0x80), 0x17f3f0 (`and 0x7183`) and 0x188078 (`or 0x4`): none sets 0x40;
the bit's setter remains unfound (a state with the flying kinds is needed), the app reads it clear.
Test: `enemy_host::tests::only_the_dash_mover_skips_the_ground`.

## 3. The sender of event 17: motion frame trigger 7

The sender is the motion's frame trigger of type 7. The frame-trigger dispatcher `0x188498` (called for every crossed record by
`FUN_00188cc0`, the walker of a motion's trigger table through `FUN_001889d0`) switches on the record's type byte through the table at
0x37aa78 (read live: 32 entries). Entry 7 is `0x188604`: it builds `{object, halfword +4, halfword +6}` on the stack and, when the object's
script thread (`+0x484`) exists, calls `FUN_0016c620(thread, 0x11, args, 3, 0)`: event 17 with (object, code, argument); then
`FUN_001b3cc8(args, 3)`. The earlier static pass found this call but not that it is fed by the motion data of the object: the records are
in the `.mset` (`kh2 mset obj/M_EX900.mset` prints `7@0[2, 0]` on 26 slots, `7@0[3, 0]` and `7@175[3, 0]` on 11 and one slot with code 1).
Other types of the same table, read the same way: 0 sets a mover state from the object's byte `+0x514` (a 10-way table), 1 `FUN_001889f0`,
2 `FUN_0016baa0(obj, +4)`, 3 mover 7, 4 `FUN_00192c50(obj, +4)`, 5 `FUN_00192c68`, 6 `FUN_001b8710`; the later cases were not read here, except 9..12 (they set the
bits the app already reads as `trigger_bits`) and 13 and 14 (docs/systems/audio/audio.md).
What the script does with it: M_EX900's entry for event 17 (`kh2 bdx M_EX900` at 0x002a) takes (object, code, argument): code 1 sets the mode
named by the argument (`crash_dead`), code 2 clears the script word `+0x18` and puts the weakness byte 6 back to 100, code 3 sets `+0x18` and the
byte to 0. So the invulnerable dive state ends at the next motion that carries a code-2 record: this was the "only event 17 clears +0x18" of the
earlier notes. Two kinds already in `KINDS` have an entry for event 17: M_EX690 (codes 1 to 4 start `ground_marker` drops of rows 115 and 116;
its motions carry codes 1, 2, 3, 5 and 300) and M_EX940 (code 1; its motions carry no type 7 record). Port: `enemy_ai::motion_events` and the
delivery in `ai_update` (`rt.event(17, [handle, code, argument])` for every type-7 record crossed in the tick, the same crossing test as the
9..12 bits). Tests: `enemy_ai::tests::m_ex900_motions_send_event_17_to_its_script` (disc), `motion_events_report_crossed_type_7_records`.
M_EX900 itself stays out of `KINDS`: the gate was the missing sender, but a 75 s soak of it was not run in this package.

## 4. Gaps this package could not close (and why)

- **+0x570 link of enemies** (host 1.161, `FUN_001beb90`): a write watch on `+0x570` of every object of the four battles for 8 s saw no
  store and every value was 0: the prologue enemies do not link. Needs a savestate with a linking enemy (the Dancer's force dance).
- **M_EX950's reaction-command senders**: the script offers the commands itself (hosts 1.165 and 1.237, event 11 with 312 and 323); no savestate has M_EX950.
- **World-map camera** (WM8): savestate 3, then `FUN_00149df0(2, 0xffffffff80000000)` by hand (Method 4) loads the world map in about 10 s (world byte
  0x33e830 = 0x0f; the ship over Twilight Town, "Land" and "Dash" prompts; saved as slot 0 for the run). On that map the camera object 0x355840
  stayed at mode 0 with no follow target (`+0x44` = 0), wanted and smoothed positions (0, -251.2, 605) and look-at (0, -170, 185) while the ship stood at (3992,
  -400, 1911); no store to `+0x60..+0x90`, `+0x40..+0x48` or the event camera block `0x5206f0`/`0x359038` in 6 s; changing the camera object's position by 800
  did not change the picture. [inferred] The map is not drawn through that camera object and its view comes from somewhere not found (a store of the ship's position
  copy at 0x354c90 is the ship's own model matrix, `FUN_0011c2a0` from 0x17ea70, not a camera). The view source stays a gap; the app's follow camera stays a stand-in.
- **Cursor contact record at `+0xb70`**: no store to `+0xb60..+0xbc0` of the cursor object (0x1af38b0, vtable 0x36fb00) in 40 s after the load except the
  constructor's zeroing of `+0xb60` and `+0xb64` (`FUN_00176e00`) and `FUN_001792cc`'s byte at `+0xb64`; no read of `+0xb70..+0xb90` in 8 s; the contact
  event handler `FUN_002ccb70` and `FUN_002cdf08` never ran (only the per-tick contact test `FUN_002cc9d0` did, 30 stops). The contact needs the ship to move
  into a symbol; without key injection the savestate stays still. The words at `+0xb70` in the loaded scene are `0x30000004, ptr, 0x1000101, tag`, repeating every 16 bytes:
  a list of 16-byte records, not a camera record. Needs a play session (or key injection) on the map.

## 5. Round 2 (WP-DYN2): the three new savestates, read offline

No emulator was run: the only PCSX2 on the machine was the user's own session (live game, PINE answered with the running game at another place), and RULES A10
and the shared data folder rule out starting the patched build next to it or touching the user's process. What was done instead: the savestates are zips whose
`eeMemory.bin` is the main RAM (same addresses as the decompilation), so the three states were read as memory images (Method: no breakpoints, no watchpoints,
so nothing here says "who writes"). Each result below is one read, not reproduced in a second run.

- **`KH2_World_Map`** (looked at: the screenshot is black): world byte 0x0f, camera type `0x35cd38` 4, visibility word `0x35cd10` 3. It is a state taken during the map's load:
  the prop list head `0x520a38` is 0, no object has the cursor's vtable 0x36fb00 (nor the player class's 0x35c9a8 or the marker's 0x36f920) anywhere in RAM, the program record
  0x33e6e0 (id 0x46) is still filled, and the map's files are named in memory (`map/us/wm00.map`, the `WM_*` models, `ovl_gumibattle.x` and `ovl_gumimenu.x` in the name table).
  So the cursor does not exist yet in this state: the +0xb70 record cannot be read from it, and a watch must start from this state and run on (about 10 s of load, as in
  round 1, then the cursor's constructor). The previous-place record 0x33e848 reads world 5, area 3, entrance 0x63 (where the user came from).
- **Camera object `0x355840`** in all three states: mode `0x355880` = 0, follow target `0x355884` = 0, wanted position (460.0, -202.1, -959.6) and look-at (285.3, -170.0, -660.5)
  (`+0x60..+0x6c` and `+0x80..`, the same twice: wanted and smoothed). Round 1 on the map loaded from the battle read (0, -251.2, 605) and (0, -170, 185). The values differ per
  run and are the same in the map state and both flight states: they are leftovers of the last field scene before the map, never rewritten by the map or the flight.
  This supports round 1 (the map's view does not come from the camera object) with a second observation; the view source is still not found. Next step for it: the VU1 data
  memory holds the transform used for drawing (`vu1Memory.bin` in the state) and a write watch on it from a running map would name the function that sets it.
- **Cursor contact record +0xb70**: not reachable (see above). Round 1's list-of-16-byte-records reading of the words at 0x1af38b0+0xb70 stands; in this state the address holds
  `0x30000004` at its very start, i.e. that heap area is the same list pattern with no cursor object on it, which makes the round-1 reading "a list of 16-byte records, not a camera
  record" more likely and the record's +0xb70 name for it doubtful [inferred]. The contact event path (`FUN_002ccb70`, `FUN_002cdf08`) stays unproven in a run.
- **Gummi flight**: docs/systems/worldmap-gummi/gummi.md (overlay at 0x1f18000, loader `FUN_00103e20`, program records, previous-place bytes).
- **+0x570 link and M_EX950**: not reachable: all three states have battle flag 0x1c8a7d8 = 0 and no enemy actor (a map and a flight); the M_EX names in RAM are only the executable's name
  table. They need the Dancer's force dance and the M_EX950 fight, i.e. a savestate in those fights.
- Needs a run (patched emulator with the three states, one at a time): the map camera/view writer, the cursor contact record and the program switch caller of 0x103e20.
- Input: `local/tools/dbg.py` and the patched build have no pad or input command (searched the script and the executable's strings). The user's PCSX2 controls are gamepad-bound, so keyboard injection is out. For a map or flight run, input would have to be written into the game's pad buffer in EE memory or the game's pad read called by hand (not tried: no emulator was run); the user's bindings and settings stay untouched.
