# Areas and events (WP-7G): implementation spec

Research only: nothing here is in the app yet, apart from the `kh2-ard` reader. This note is meant to let the
E1..E6 work packages (section 9) turn Twilight Town from a battle arena into a place without reading the game
code again.

Marks: **[V]** read in the game's code (address given) or printed from the user's disc by a test; **[I]**
inferred (fits the code, no line states it); **[GAP]** not found, with what was searched. Addresses are
functions (`FUN_xxxxxxxx`) or data in `SLUS_210.05`. No game data is stored here; the tests print it from the
user's disc.

Evidence that runs (all in `crates/kh2-ard/tests/disc.rs`, `cargo test -p kh2-ard --test disc -- --nocapture`):

- `twilight_town_area_script_opcodes`: every program of every `ard/ttNN.ard` (42 files), the opcode table with
  counts, and every trigger (opcode 0xc) decoded.
- `twilight_town_groups_exits_and_events`: every spawn group with its kind, entities, activators, walk paths and
  signals; per area the exits (destination area and entrance) and the BAR entries that are not scripts or
  spawn files (events, type 0x16; area BDX scripts, type 3).
- `twilight_town_events_split_into_commands`: every Twilight Town event splits into sized commands to its end;
  the event opcodes with counts; `07localset.bin`'s program-to-area table for Twilight Town.
- `twilight_town_story_flags_and_music`: the side effects of every Twilight Town story flag
  (`00progress.bin`), and the music each area-script program sets next to `03system.bin`'s `arif` pairs.
- Unit tests (`cargo test -p kh2-ard --lib`): `a_trigger_decodes_its_sub_commands`,
  `an_event_splits_into_sized_commands`, `story_flag_effects_follow_the_offset_table`.

Earlier notes used: `docs/enemies.md` (the spawn groups of tt04), `docs/camera.md` ("Event camera", "Area
camera types"), `docs/hud.md` ("Message windows (WP-6D)", area card), `docs/audio.md` ("Battle music"),
`docs/limits-reactions.md` (reaction commands, the 0x83 spawn part), `docs/bdx.md` (hosts 0.14, 0.15, 1.202,
1.203), `docs/progress.md` (the save block).

## 0. The area record

The game keeps the current place at `0x33e830` (six bytes) [V]:

| byte | address | meaning | writer / reader |
|---|---|---|---|
| 0 | `0x33e830` | world (0..18; Twilight Town is 2; the prefixes are the table at `0x349dc0`) | `FUN_00104bd0` |
| 1 | `0x33e831` | area (the `NN` of `ttNN`) | same |
| 2 | `0x33e832` | entrance | same; read by opcode 0x1c and the entity filter `FUN_001981a0` |
| 3, 4, 5 | `0x33e833..5` | the program ids run from the `map`, `btl` and `evt` scripts | `FUN_00104bd0`; read by `FUN_00181148` |

The file name is `ard/` + world prefix + the area as two digits (`FUN_00105010`, format `%s%02d`; `FUN_00180870`
formats `ard/%s.ard`) [V].

**Program ids per area** live in the save block: three bytes per area at `0x33e86f + world * 0xc0 + area * 3`
(map, btl, evt). `FUN_00104bd0` copies them into bytes 3..5 when an area is entered, then `FUN_00104b90` lets the
jump's own bytes 3..5 override them (0xff keeps a byte) [V]. Who writes them: the story flags (section 5) and
jumps of type 2 (section 2) [V].

## 1. The area-script interpreter

**Where it runs** [V]. `FUN_001689c0` (called by the area thread `FUN_001048e0`) loads the area file
(`FUN_00180870`: reads `ard/<area>.ard` into the buffer `DAT_0035cd44` unless the same world and area are
loaded) and later calls `FUN_00181148(0x33e833)`. That routine resets the script variables (list below), then
for each of the BAR tags `map`, `btl`, `evt` (strings at `0x37a850`, `0x37a858`, `0x37a860`) finds the type-13
entry (`FUN_00168510(file, 0xd, tag)`) and runs the program whose id is the matching byte of the area record
(`FUN_00180a68`: programs are `u16 id, u16 size`, size includes the header, id 0xffff ends the list).
`FUN_00180ad8` runs the commands of that one program in order (`u16 opcode, u16 count, count u32`) through
`FUN_00180b48`; a command that answers 0 stops the program. After the three scripts it applies defaults and the
map visibility (below).

**Opcodes** (`FUN_00180b48`, a jump table of 32 entries at `0x37a7ac`; `sltiu op, 0x20`, higher opcodes do
nothing). Names are the project's (`kh2_ard::opcode_name`). Arguments are the u32 words.

| op | name | effect | mark |
|---|---|---|---|
| 0 | `spawn` | for every argument (a 4-character spawn-group tag): load that type-12 entry's groups (`FUN_00181728(tag, 0)`) | V |
| 1 | `map_visibility` | the 64-bit word `DAT_0035cd10` = the two arguments; at the end `(word \| show bits) & ~hide bits` goes to `FUN_00149b40` via `FUN_001047a8` [I: which map parts are drawn] | V for the writes |
| 2 | `spawn_random` | load one argument chosen with the game's LCG (`DAT_003716a0 *= 0x10dcd`) | V |
| 3 | `spawn_chance_stop` | roll 0..99 (`FUN_00325b38(seed, 0x28f5c29)`); below argument 0: load group argument 1 and stop the program | V |
| 4 | `capacity` | `DAT_01c893b8` = argument as float (default 10000.0 at `0x37a868`); `FUN_00181458` / `FUN_00181470` report what is left and the fraction used [I: the area's object budget] | V for the writes |
| 5 | `heap_size` | `DAT_0035cd20` (default 0x280000) | V |
| 6, 7 | `cache_a`, `cache_b` | `DAT_0035cd28` / `DAT_0035cd2c` (defaults 3, 4; both 0 when the map program id is above 0x32); passed to `FUN_00194188` on the tables `0x35d278` / `0x35daa0`, four-slot caches keyed by area [I] | V for the writes |
| 8, 0xd | `no_op` | the jump table's default | V |
| 9 | `spawn_alt` | load the group with the second flag set (`FUN_00181728(tag, 1)`: the group record's word +4 = 2) | V |
| 0xa | `entry_by_index` | the area file's entry with this index (any type, `FUN_001684c0(file, -1, n)`), if not empty, goes to `FUN_001817f0` (a group record of the other kind, word +4 = 1) | V; meaning GAP |
| 0xb | `word_35cd18` | 64-bit mask `DAT_0035cd18`; `FUN_0019d4c0` / `FUN_0019d520` call `FUN_0015e9a8` / `FUN_0015e928` for each set bit (the same routines as hosts 1.37 / 1.36) | V; meaning GAP |
| 0xc | `trigger` | defines a trigger (below, `FUN_00180ff8`) | V |
| 0xe | `word_35cd30` | read by `FUN_0015bc10`: unless 99, `FUN_00149200(.., value)` -> `FUN_0015b6e0` | V; meaning GAP |
| 0xf | `world_record` | `FUN_001a7330(current world record, argument)` (the world records of docs/progress.md) | V |
| 0x10 | `music` | the music pair word `DAT_01c95638` (u16 field music, u16 battle music); `FUN_00137150` reads it at area load | V |
| 0x11 | `word_35cd34` | `DAT_0035cd34` (default -1), read by `FUN_00179140` (a test of the object's +0x5f4) | V; meaning GAP |
| 0x12 | `heap_base` | `DAT_0035cd24` (default 0x100000); `FUN_001048e0` calls `FUN_00105250(cd20 + cd24, cd24)` | V |
| 0x13 | `camera_type` | `DAT_0035cd38`, the area camera type of docs/camera.md (default 0) | V |
| 0x14, 0x17 | `flag_35d268_4`, `_10` | set bit 2 / bit 4 of the global flag word `DAT_0035d268` | V |
| 0x15 | `battle_manager` | `FUN_001b9338(argument 0, rest)`: creates the battle/minigame manager at `DAT_01d5dd20` (docs/enemies.md, Struggle) | V |
| 0x16 | `load_file` | formats the arguments as a string with the language code (`DAT_00349d40`) into `0x1c893d8` and loads it (`FUN_0016fac8(.., 0x38e, 0)`) | V; which files GAP |
| 0x18 | `effect_heap` | `DAT_0035cd3c`, `DAT_0035cd40` (defaults 0x80000, 0x20000), passed to `FUN_001dbe90` | V |
| 0x19 | `set_story_flags` | every argument not yet set is set through `FUN_0017ff60` (section 5) | V |
| 0x1a, 0x1b | `map_show_bits`, `map_hide_bits` | OR `1 << argument` into `DAT_01c893c0` / `DAT_01c893c8` (used with opcode 1) | V |
| 0x1c | `if_entrance` | when argument 0 equals the entrance byte, run the commands nested in the rest | V |
| 0x1d | `party_reset` | `FUN_001abfe8(argument)` per argument: cases end the drive form (`FUN_001c75c0`), reset Anti (case 0xb), call `FUN_001ac170` [I: party and form resets on area change] | V for the calls |
| 0x1e | `level` | `DAT_01c893d0` -> `FUN_001be828`: the value, or (when 0) the per-world byte at `0x1d5dd68`, at least 1, to `FUN_001beac8` [I: an area level] | V |
| 0x1f | `spawn_lottery_stop` | `FUN_001d7970`: a five-slot lottery at `0x1d6a9c0`; when this area's slot wins, load the group and stop | V |

Defaults set by `FUN_00181148` before the scripts run: the variables above, `DAT_0035cd10 = -1` (all map
parts), the show/hide bits 0, camera type 0 [V].

**Triggers (opcode 0xc)** [V]. The first argument word is the key `(event u16, arg u16)`; `FUN_001975e0` finds or
makes the 0x54-byte record in the table at `0x1c8a9c8` (16 records). The rest are sub-commands
(`kh2_ard::parse_trigger`):

| sub | size (halfwords) | record field | meaning |
|---|---|---|---|
| 0 | 4 | +0x14 = u16, +0x10 = u32 | play an event: +0x10 is the event's 4-character tag (an area-file entry of type 0x16), +0x14 its argument (the low 6 bits select a row of the event kind table `DAT_01d6a92c`, `FUN_001d1128`) |
| 1 | 8 | +0x34.. | the jump record (one unused halfword, then six: type, world, area, entrance, key, flags; `FUN_0019d8f0`) |
| 2 | 2 | +8 | story flag to set when the trigger fires |
| 3 | 2 | +0xc | argument of `FUN_001a52b0` when it fires [GAP: meaning] |
| 4 | 2 | +0x44 (jump record word 4) | a world-record value applied before the jump (`FUN_001a7330`) |
| 5 | 2 | +0x48 (word 5) | party reset kind (`FUN_001abfe8`) at the jump |
| 6 | 2 + 2n | +0x18, +0x1c | a list of n words handed to `FUN_001a6878` -> `FUN_001a6a78` [GAP] |
| 7 | 2 | +0x4c (word 6) | 1 or 2: open the menu `FUN_0024f300` before the jump (2 also `FUN_001d73d0`) [I: the save prompt] |
| 8 | 2 | +0x50 bit 0 (word 7) | flag [GAP] |

**Firing** [V]. `FUN_00197b70(event, arg)` is the game's "something happened" call. Some ids open menus
(`FUN_0024f0b8(4, id)` for 0x13, 0x66..0x75, 0x83..0x85, 0x8a) or go to `FUN_0023f108` (0x16, 0x46, 0x4f,
0x57..0x59, 0x62, 0x64, 0x65, 0x76, 0x81); all others are broadcast to every object (`FUN_0016b470`, object
method +0x34) and matched against the triggers (`FUN_00197670` -> `FUN_00197588`: same event and same arg, or a
record with arg -1). A matching trigger runs `FUN_001978b0` (set the flag, `FUN_001a52b0`, the list, then the
signals of the spawn groups: show or hide groups, below) and `FUN_001979f8`: with an event tag it plays the event
(`FUN_00196428`) and keeps the jump for its end, else it jumps at once (`FUN_0019d9d8`).

Senders found (`mips_scan --jal 0x197b70`): 0 area start (`FUN_0013efe8`, the area-start camera; when a trigger
takes it the event replaces the normal fade-in), 1 an exit volume touched (group id, `FUN_00198fa0`), 2 and 3 a
battle group emptied / all emptied (`FUN_00199518`), 0x12 or 99 an event ended (`FUN_00225028`), 0x1f a command
(`FUN_001778e8`), 0x21 leaving for the world map (`FUN_00149df0`), 0x4b (`FUN_001b02f0`), 0x4c the next event of a
chain (`FUN_001964d0`, after an event whose kind row has bit 0), 0x50 / 0x51 (`FUN_001bdb38`, `FUN_001bd1d8`),
0x9e (`FUN_0019b3f0`), the kind-3 and kind-4 groups (their header +0x1c, section 2), the battle manager's result
(`FUN_001b9140` -> `FUN_001b96e0`, the event in its +0x2dc, e.g. tt04's 7, 9, 0xa, 0xb, 0xf, 0x8b, 0x8d) and
`FUN_001aa1a0`, `FUN_001a8978`, `FUN_001b65e0` (script and menu paths) [V for the calls; I for the names].

**Opcodes used in Twilight Town** [V, printed by `twilight_town_area_script_opcodes`; 42 area files tt00..tt41]:

| op | name | count |
|---|---|---|
| 0x00 | spawn | 1706 |
| 0x01 | map_visibility | 308 |
| 0x04 | capacity | 72 |
| 0x05 | heap_size | 1 |
| 0x06 | cache_a | 3 |
| 0x09 | spawn_alt | 8 |
| 0x0a | entry_by_index | 15 |
| 0x0b | word_35cd18 | 15 |
| 0x0c | trigger | 373 |
| 0x0e | word_35cd30 | 52 |
| 0x0f | world_record | 187 |
| 0x10 | music | 108 |
| 0x11 | word_35cd34 | 42 |
| 0x14 | flag_35d268_4 | 16 |
| 0x15 | battle_manager | 61 |
| 0x16 | load_file | 33 |
| 0x19 | set_story_flags | 49 |
| 0x1d | party_reset | 39 |

Not used in Twilight Town: 2, 3, 7, 0x12, 0x13 (so every tt area has camera type 0), 0x17, 0x18, 0x1a, 0x1b, 0x1c,
0x1e, 0x1f. Every trigger decodes with the sub-command sizes above.

## 2. Area loading and transitions

**What triggers an area change** [V]. Every change goes through `FUN_00104e18(destination, fade, wait, flag,
reset)`. Callers: an exit volume (`FUN_00198fa0`), a trigger's jump (`FUN_0019d9d8`), hosts 1.30 and 1.31
(`FUN_00172118`, `FUN_00172170`), leaving to the world map (`FUN_00149df0`), the revive path (`FUN_001ad9a0`),
the area thread itself (`FUN_00103b38`) and some minigame/menu code (`FUN_00242908`, `FUN_002c4c58`,
`FUN_002d8b98`, `FUN_002df3d8`, `FUN_002e3c78`, `FUN_002e7f18`). The destination is six bytes (world, area,
entrance, map, btl, evt; 0xff keeps the current one).

- **Exit volumes.** A spawn group's activators (0x40 bytes: `u16` shape +0 (0 box, 1 cylinder with an
  elliptic x/z section, 2 always inside: `FUN_00334388` returns 1), `u16` option +2, position +4, half sizes
  +0x10, rotation +0x1c; the volume turns by minus the Y rotation, `FUN_001d8600`; tests `FUN_001d89a0`,
  `FUN_001d8a70`) become volumes when the group spawns (`FUN_00198750`). Every frame `FUN_001989d8` tests the
  player against them and hands a hit to `FUN_00198b30`, which acts by the group's kind (byte +0). For kind 1
  (`FUN_00198e10`) the destination is the header's +0x1c (area; 100 means world 0x12 area 1, 101 world 2 area
  0x28) and +0x1d (entrance). Option 0 is the walk-in volume: when the group is enabled, the exit is not locked
  (`FUN_00195358(world, area)`) and `FUN_001d88a0(volume, header +0x1e)` passes, it jumps (`FUN_00198fa0`: the
  destination goes to `0x33e840`, event 1 with the group id is fired, and unless a trigger takes it
  `FUN_00104e18` runs with fade 1 when the world changes, else 0). Options 1 and 2 are a larger volume: when the
  player faces it (dot product of his facing with the direction to its centre >= 0, `FUN_00199058`) it shows the
  destination (`FUN_0015ad80` with `FUN_001953e0(world, area)` while unlocked, `FUN_0015acc8` when locked) [I: the
  area-name prompt at the exit]. While the battle state (`FUN_001903c8`, `0x1c8a7d8`) is 2 an exit whose +0x1e is
  not 0 does nothing [V; I: exits close in battle].
- **Other group kinds** (`FUN_00198b30`) [V]: 2 respawning battle group (`FUN_001991a0`: entities of spawn kind 2
  come back after +0x56 frames, placed by +0x1d: 1 at a point near the entity, 2 within a radius +0x5a around
  +0x30), 3 fire event header+0x1c once (`FUN_00199930`), 4 fire header+0x1c for each object entering
  (`FUN_00199b18`), 6 a group whose entities of spawn kind 2 are created on demand (`FUN_00199e98`; only volumes
  with +0x2c = 1 are tested), 7 `FUN_001ccb10(player, header +0x1c, activator +0x28 bit 0)` [GAP], 8 show / hide map
  parts by activator +0x2e / +0x2f (`FUN_0015e9a8`, `FUN_0015e928`), 9 `FUN_0019a000` [GAP], 10 the player's +0xb4c
  = the group id. Kind 5 and group flag bit 0 (+1) start the group disabled (`FUN_00198430`). Twilight Town
  counts (the test prints the u16 +0): 1: 438, 2: 185, 0x102 (kind 2 with flag bit 0): 5, 3: 18, 4: 16, 6: 231,
  10: 37. Kind-1 groups are also the plain groups (the player's entrance groups `m_00`, the points `m_20`/`m_21`):
  only those with activators are exits.
- **Signals** (8 bytes, `FUN_00198430`): `u16 event, u16 arg, u8 action`: the group registers itself with the
  trigger `(event, arg)` (event 0x4a ignores the arg); when that trigger fires the group is enabled (action 0,
  `FUN_00199c70`) or disabled (1, `FUN_00199c48`). Scripts do the same with hosts 1.202 / 1.203 [V].
- **Jump records** (`FUN_0019d9d8`, from a trigger or `FUN_001d3a68`) [V]: type 1 goes to (world, area,
  entrance); type 2 to the area `07localset.bin` gives for the key (below) with all three programs = the key and
  the given entrance; type 3 back to `0x33e840` (where the last exit pointed); type 4 a world-map case decided by
  story flags 0x1071, 0x1072, 0x140f, 0x1822, 0x200b, 0x8cd, 0x3834, 0x1c1c (`FUN_00149df0`); 5 opens menu 0xe; 6
  `FUN_00149f38`. Word 4 (>= 0) sets the world record; word 6 opens the menu before the jump. The flags word
  becomes the fade kind (bit 0x4000 -> 0x80000000).
- **`07localset.bin`** [V, loaded by `FUN_00198058` into `DAT_01c8af88`; read by `FUN_00198080`]: a BAR, one type-2
  entry per world: `u16 count`, then `(u8 program, u8 area)` pairs. Twilight Town has 144: programs 0x33..0xd1
  (e.g. tt04 owns 0x4c..0x53, 0xb2, 0xb5..0xb8, 0xcc, 0xcd). So the "story programs" are world-wide numbers, and
  a type-2 jump to program 0x4d always lands in tt04 with map, btl and evt program 0x4d. Host 1.31 does the same
  jump from a script.
- **Jump types, status (WP-JT2)** [V unless marked]. The record is parsed when the trigger is built
  (`FUN_0019d8f0`), so type 2 is resolved there: `FUN_00198080(out, key = record word 4, world = record word 1)`
  (a negative world means the current one, `DAT_0033e830`) fills the six destination bytes with (world, the first
  pair's area, 0, key, key, key) and the record's entrance byte (word 3) replaces the 0; an unknown key leaves all
  six 0xff, which `FUN_00104bd0` reads as "keep the current world and area" (the entrance still changes).
  `FUN_00104b90` then lays the three program bytes over the story table's (a byte of 0xff keeps the table's). The
  app: `kh2_ard::localset` (world entry, pairs, `area_for`), `area_jump::resolve_jump` and `Dest.programs`,
  `AreaRecord.dest_programs` (applied by `area::run_area_script` at the next area start; `KH2_PROGRAMS` still
  wins). Type 1 is the record's own bytes, 0xff keeping the current ones (the same rule). Type 3 jumps to the six
  bytes at `0x33e840`: **written** by `FUN_00198fa0` on every exit walk-in (the exit's destination world, area,
  entrance and programs 0xff, before the trigger on event 1 is asked, so an exit that a trigger takes over also
  records it); initialised to all 0xff by `FUN_00105420` (a type 3 before any exit stays in the current area).
  App: `area_jump::LastExit`, recorded at both walk-in sites. Tests: `a_type_two_jump_goes_to_the_keys_area_with_all_three_programs`,
  `a_type_three_jump_returns_to_the_last_exits_target`, `the_disc_localset_gives_the_prologue_keys_tt04`
  (disc), `localset_gives_the_story_programs_their_areas` (disc: Twilight Town 144 pairs, 0x33..0xd1, keys 0xb5
  and 0x4d give area 4). Harness: `KH2_JUMP=<type>:<key>:<entrance>` (hex; a type-1 key is the area) at
  `KH2_JUMP_AT` seconds runs the record through the real fade and load path.
  A type-2 jump into the area that is already loaded makes the app run that area's scripts again so the new
  program bytes take effect (the game restarts the area thread on every jump) [inferred].
  **WP-JT3** (types 4 to 6 and the record's words 4 to 6; `area_jump::resolve_record`, `type4_dest`, `JumpExtras`).
  Type 4 (`FUN_0019d9d8` case 4 -> `FUN_00149df0`, fade kind always 1) [V]: flag 0x1071 set and 0x1072 clear and one of
  0x140f / 0x1822 / 0x200b set -> world 4, area 0, entrance 0, programs kept; else one of 0x8cd / 0x3834 / 0x1c1c ->
  `FUN_00198080(0x40, 0x12)` (world 0x12); in both the record's world byte is remembered in `DAT_01c8bd90`
  (`Type4Memory`). Otherwise the world map: world 0xf, area 0, entrance `FUN_00149db8(world)` (searches the 15
  `(entrance, world)` pairs at 0x3790a8, read from the executable at run time, 0 when absent), where the remembered byte
  replaces the record's world and is forgotten [inferred: that the caller passes the remembered byte as the world
  argument; the -1 case of `FUN_00149df0` reads the current world byte]. The app has only Twilight Town, so every type-4
  case ends in the log line "the world map is not available" (world 4 and 0x12 are not in the app either) and the player
  stays. Type 5: `FUN_0024f0b8(0xe, ..)` opens menu kind 0xe (record 0x380650, scene 9, init `FUN_00292f18`, task
  `FUN_00292fc8`, page `FUN_00293020`) and no jump follows; the app has no such menu, so `Outcome::OpenMenu` is logged
  (hook for a later `menu*.rs` kind 0xe) [gap: the page itself]. Type 6: `FUN_00149f38` starts task `FUN_00149eb0`, which
  freezes the screen (`FUN_0014dcb0`, `FUN_0014de00`), fades the screen fader to black (`FUN_0014a7c8(fader, 0xf, 0)`),
  waits for black and ends; no destination and no fade-in was found [gap], so `Outcome::FreezeFade` is logged only.
  Record words 4 to 6 come from the trigger sub-commands 4, 5, 7, 8 (`FUN_00180ff8`; defaults `FUN_00197780`): word 4
  (sub 4, default -1, `FUN_0019d8d0`) a party arrangement index (`FUN_001a7330(FUN_001a71c0(world), index)`, ported in WP-PT1: `party::jump_party`, docs/party.md section 21); word 5 (sub 5, default 0) the party reset code (the 5th argument of `FUN_00104e18`; when the record has no destination world and the word is non-zero `FUN_001abfe8(word 5)` is called (ported); with a destination the reset runs in `party::jump_reset`, WP-PT1);
  word 6 (sub 7, default 0) "menu before": 1 or 2 opens menu kind 5 first (`FUN_0024f300`) and sets the jump's wait (the 3rd argument of
  `FUN_00104e18`, read as ticks [inferred]) to 2, logged only (the menu is missing); any other code leaves both at 0
  (`LAB_0019db28`); sub 8 is not read by `FUN_0019d9d8`. Tests:
  **WP-JT4** (types 5 and 6, "menu before"): type 5 opens menu kind 0xe, the battle report (`menu_ending.rs`, docs/menus.md section 25:
  `ending.2ld`, title 0xcc24, eight statistic rows; `area_jump::JumpMenus.ending`), and no jump follows. The record's "menu before" code
  (word 6) is ported only as its wait: the jump task's `JumpTask::Wait` holds `JumpExtras::delay_ticks` (2) ticks before the fade-out
  (also after a trigger's event, where the fade-out is skipped); the camp it opens first (`FUN_0024f300`) is always page 0x15, the party
  switch notice, ported in WP-PT1 (menus.md section 25, docs/party.md section 21). Type 6 (`FUN_00149eb0`): `JumpTask::Black` fades to black over 30 ticks
  (`FUN_0014a7c8(fader, 0xf, 0)`, the black is full when the fader's state `+0xfc` is 4) and holds. Searched for a fade-in: the callers of
  `FUN_00149f38` (one: `FUN_0019d9d8`) and of `FUN_00149eb0` (only its task start), the fader kind 0xf callers (`FUN_0013f1a0` area-start
  fade-in, `FUN_0013f2e8`, `FUN_002233d8` event end, out: `FUN_00103fc0`, `FUN_0013f148`, `FUN_0013f290`, `FUN_002580b8`, `FUN_00259e88`): the task
  ends after the black, and the next fade-in is whichever area start or event end runs later. The app lifts the black when a jump starts or an
  event plays [inferred]; the screen freeze is not ported, by decision (WP-AJ1): `FUN_0014dcb0` sets the flag `DAT_003584e0` (cleared by
  `FUN_0014dcc0`; its only reference is in the capture routine `FUN_0014dbe8`, 0x14dc88) and `FUN_0014de00` calls `FUN_0014dbe8`, which
  copies the GS frame into the 0x9c000-byte buffer at 0x484480 (`FUN_0014de40` clears it; `FUN_0014ddc0` / `FUN_0014db58` send it back to
  the GS). The same pair opens every kind 1 fade-out (`FUN_0013eea8`) and the keyhole wipe, where `FUN_0014ddc0` redraws the copy
  (callers `FUN_0013eea8`, `FUN_0013f368`, `FUN_0014a270`, `FUN_0015cf58`, `FUN_001612b0`, `FUN_00161570`); `FUN_00149eb0` has no such
  call, so no read of the copy was found on the type 6 path [gap: the yield `FUN_00102438` was not read]. The copy is a PS2 VRAM
  detail with no counterpart in this app's pipeline (the fade is a node over the live scene, as for every other fade-out), so the
  current fade (simulation running under the black) is kept. Tests `the_type_six_black_lifts_when_an_event_plays` (the black advances,
  holds at `FADE_TICKS` and is released when an event plays), `the_wait_is_clear_and_the_type_six_black_holds`, `the_rows_use_the_tables_of_the_executable`.
  `type_four_decides_by_story_flags`, `type_four_to_the_world_map_is_not_available`,
  `types_five_and_six_open_a_menu_or_freeze_without_a_jump`, `record_words_four_to_six_parse_with_defaults`,
  `a_type_four_record_always_fades_with_kind_one`. Harness `KH2_JUMP=4:2:0`, `5:0:0`, `6:0:0` (quoted lines: "jump: type 4
  key 0x2 -> world 15 area 0 entrance 0 ...", "jump: the world map is not available (the app has only Twilight Town);
  staying", "jump record type 5: opens menu kind 0xe ... no menu, no jump", "jump record type 6: FUN_00149f38 freezes the
  screen and fades to black, no jump"). Scan done: `mips_scan.py --jal 0x19d8f0` has one caller, `FUN_00180ff8` (0x1810a0); `--jal 0x1d3a68` has one caller,
  `FUN_001051f0` (0x105220), which is not a record creator; so `FUN_00180ff8` is the only creator of words 4 to 6. **Gaps**: the other
  two writers of `0x33e840`, `FUN_00172250` (current world, area, entrance + 1) and `FUN_001722c0` (the record saved
  in `0x33e848` by `FUN_00177248`, world 0x11 remapped to area 0x32 / 0x37), have no direct call site in the
  executable (`mips_scan.py --jal` finds none for them; `FUN_00177248`'s only caller is `FUN_00177788`), so a
  type 3 after those paths is not modelled.

**The jump itself** [V]. `FUN_00104e18` starts the task `FUN_00104d58` (destination kept at `0x33e838`), which
waits its delay, starts the fade-out kind (`FUN_0013ef68`: kinds 0..3 run `FUN_0015d058`, `FUN_0013f148`,
`FUN_0013f290`, `FUN_0013f3b0`), waits until the fade and `FUN_002f4c40` are done, runs the party reset
(`FUN_001abfe8`), unloads the area (`FUN_00104ce0`), on a world change calls `FUN_001cb208` and clears the
previous world `0x33e848`, then `FUN_00104c68` applies the destination (`FUN_00105188` -> `FUN_00104bd0`; it also
stores world, area, entrance at `0x33e86c..e`) and restarts the area thread `FUN_001048e0`. On the next area start
`FUN_0013efe8` runs the matching fade-in unless trigger 0 plays an event.

**Fades, gate and lock (WP-E1b notes)** [V unless marked]:

- `FUN_00104e18(destination, fade, wait, flag, reset)` takes the fade kind from its caller;
  `FUN_00198fa0` passes `current world != destination world`, so an exit fades with kind 1 on a
  world change and kind 0 inside a world. With `wait` 0 the fade-out starts at once, else the task
  waits `wait` ticks first (`FUN_00104d58`). A trigger's jump record passes its flags word
  (`FUN_0019d9d8`); bit 0x4000 becomes bit 31, which makes `FUN_0013ef68` only record the kind.
- Kind 1 is the screen fader at `0x3564d0`: out `FUN_0013f148` -> `FUN_0014a7c8(fader, 0xf, 0)`
  (state +0xfc = 3), in `FUN_0013f1a0` -> `FUN_0014a7f0(fader, 0xf, 0)` (+0xf8 = 1, +0xfc = 1).
  `FUN_0014a710(fader, n)` sets the duration `2 * n` ticks (+0x100, so 30) and the step
  `DAT_00379130 / duration` (+0x104; `DAT_00379130` = 128.0, the full alpha); byte +0xf4 picks
  black (0, `FUN_0014af90(0, 0, 0, 0)`, mode 0x44) or white (0xff, 0x48). The app: a black
  full-screen UI node whose alpha runs 0 -> 1 in 30 ticks and back (`area_jump::FADE_TICKS`).
- Kind 0 is the 2D layout in slot 2 of the table at `0x355118` / `0x35511c` (`FUN_0013a980(2)`,
  `FUN_0013a998(2)`): out `FUN_0015d058` plays its animation 1 (`FUN_002368a8(0x3591e0, 1)`), in
  `FUN_0015d1a8` plays animation 0 and a 2-tick fader fade-in (`FUN_0014a7f0(fader, 1, 0)`). The
  reference video 8.mp4 shows this as the keyhole wipe, then the area card. **The layout (WP-E1c)**:
  `FUN_0013a7e8` reads `field2d/<lang>/<world>0field.2dd` (format string at 0x378d78; the
  world's prefix from `0x349dc0`) to `0x3c32c0` when the world changes; `FUN_0013a8a8` fills the
  table at 0x355110 with the BAR's count of type-0x19 entries (+4) and per slot `i` the `i`-th
  entry of type 0x18 (image, 0x355118 + 8i) and of type 0x19 (sequence, 0x35511c + 8i).
  `FUN_00236650` sets a sequence player on slot 2 (`0x3591e0`, image object `0x359040` from
  `FUN_002332e0`); its group records are 0x24 bytes with the play count at +4
  (`FUN_002369e0`) and its animation records 0x90 bytes (`FUN_002379a8`): the `SEQD` format the
  HUD already draws (`kh2_image::seqd`). The out task `FUN_0015cf58` steps and draws it
  (`FUN_002378a0`, `FUN_002379a8`) until the group has ended (play count not 0 and the player
  out of its states 1 and 2, `FUN_002369c0`), then fades the screen fader to black in 2 ticks
  (`FUN_0014a7c8(fader, 1, 0)`) and marks the fade done (`FUN_0013ef38`). The in task
  `FUN_0015d0e8` plays group 0 and ends when the group has ended and the fader is idle (+0xfc 0).
  The test `area_jump::tests::the_keyhole_layout_is_slot_two_of_the_world_field_file` decodes
  slot 2 of `tt0field.2dd` and prints both groups. **Drawn (WP-E1d)**: `area_jump::Keyhole`
  adds one hidden root node at (0, 0) on the HUD file that holds slot 2's pair (the HUD loads
  the world's field file first; `pair_index` gives its `SeqFile::from_bar` index, checked by image
  size and group count). Out: group 1, then once it has ended a 2-tick fader to black
  (`wipe_out_step`, about 74 ticks in all); in: group 0 with a 2-tick fader from black, done when
  the group has ended (about 72 ticks), then the node is hidden. If the pair is not among the HUD's
  files, kind 0 keeps the 30-tick black fade (logged). The node is drawn in HUD node order, so HUD
  parts drawn later (the command menu, the area card) show on top of it, and on a window wider
  than 4:3 the strips beyond the layout's extent (-54..566) stay visible [inferred: the original
  is 4:3; whether the game hides the HUD during the wipe was not read].
- Kinds 2 and 3 (`FUN_0013f290` / `FUN_0013f3b0`, in `FUN_0013f2e8` / `FUN_0013f3f0`): not read.
- Gate (header +0x1e, `FUN_001d88a0(volume, gate)`): 0 passes. 1 and 2 need `FUN_001d8908()`: the
  player is in the controlled-object list at `0x1d5d7a0` (`FUN_001b80e8`), his actor flags +0x108
  have neither 0x80000 nor 0x10000000 (`FUN_0019a440`) [I: an action or event in progress], and
  `FUN_001b7e00`: method +0x40 answers non-zero and +0x160 has bit 4, 8 or 0x10 [I: walking,
  running or dashing]; then `FUN_001d8958(volume)`: the direction `(sin a, 0, cos a)` for
  `a = DAT_0037c714 - volume rotation y` (`FUN_0017be98`; `DAT_0037c714` = -PI/2) must have a dot
  product of at least `DAT_0037b358` (= 0.7) with the player's facing (`FUN_0016a890`) and with his
  movement direction +0x10, while his speed +0x1c is above 0 (`FUN_0019b110`). Gates 3 and up
  fail. Every tt04 exit has gate 2, so walking out needs the player to move along the exit's
  direction. The app ports this as `area_jump::gate_passes` (moving = horizontal velocity above
  0; the busy flags are not ported) and reads the two constants from the executable.
- Lock (WP-E1c): `FUN_00198e10` asks `FUN_00195358(world, area)` about the exit's destination
  before it acts on a volume; non-zero stops the walk-in and makes the prompt the locked one
  (`FUN_0015acc8`). `FUN_00195208` finds (world, area) in the table `DAT_01c8a988`: `u16 count`,
  then `u8 world, u8 area` per entry (`lhu` / `lbu` at 0x195210..0x195230); -1 when absent, and
  an absent area is never locked. The table is the `dsa` entry of `00progress.bin` (the first
  named lookup of `FUN_0017fd10`, tag string at 0x37a5e8, handed to `FUN_001951f8`). Entry `i`
  owns two halfwords of the save block at `0x33f918 + 4i`: the state (read with `lhu`; any
  non-zero value locks, 0xffff included: `sltiu (v + 1), 2` / `movz` at 0x195398..0x1953a0) and
  the message the prompt shows (`FUN_001953e0`). With state 0 the game also asks `FUN_001d32c0`
  -> `FUN_001d32e0`, which locks only in world 0x10 (story flags in the table at 0x37c528).
  Writers: the story-flag side effects op 2 (`FUN_00195260`: state and message), 3
  (`FUN_00195288`: 0), 4 (`FUN_001952a0`: 0xffff and the world point bit of the area,
  `FUN_00194f68` / `FUN_00194fd8`) and 5 (`FUN_00195300`: 0 and the world point bit); the first
  halfword of each op is the table index (`FUN_0017ff60`). Ported: `kh2_ard::progress::lock_index`
  / `exit_locked` and `area_jump::ExitLocks` (the table from the disc, the halfwords from
  `progress::Story`, which the flag side effects ops 2..5 write since WP-E5 [inferred: a new game
  starts with them 0; its initialiser was not read]; the world-0x10 branch is not ported). The test `the_exit_lock_table_and_the_flags_that_write_it`
  prints the table and every Twilight Town flag that writes it.
- `FUN_001989d8` clears the group's flag bits 3 and 4 every frame (0xffffffe7) and skips a group
  whose bit 0 (fired, set by `FUN_00198fa0`) is set; bit 4 is the prompt shown this frame
  (`FUN_00198e10`), so the prompt is re-issued every frame while the player faces the volume. In
  battle state 2 (`FUN_001903c8`) a group whose gate is not 0 does nothing: not ported (no battle
  state resource in the app's area code).
- Oracle: `crates/kh2-ard/tests/oracle.rs` `exit_volumes_match_the_game` runs `FUN_001d8600` +
  `FUN_001d89a0` / `FUN_001d8a70` on every Twilight Town exit activator plus turned synthetic ones
  (the identity matrix at `0x354a60` seeded, as `FUN_0011bd58` does at start-up): 183 records,
  92781 points, 0 mismatches against `Activator::contains`. The rotation sign of
  `Activator::local` is therefore verified, no longer inferred.

**Player placement and facing** [V]. There is no separate entrance table: an entity whose spawn kind (+0x1c) is 1
is created only when its +0x1d equals the entrance byte (`FUN_001981a0`). The player (0x236) and the two friends
(FRIEND_1/2) stand in the `map` program's groups once per entrance (tt02 `m_00` has two such groups, `e_50` six
entities [I: two entrances, one of them 0x32 = 50, the entrance tt02's triggers jump to]). The object faces `DAT_0037b270 - rotation y` (`FUN_00198220`; docs/enemies.md uses
the same rule: app yaw `PI - r`).

**What is kept** [V for the calls; I for the rest]: the save block (party, HP, items) is not touched by the area
change; the summon is cleared and reloaded (`FUN_00168ba8`, docs/summons.md); the drive form may end
(party reset kinds); the music is restarted only if the pair changes (`FUN_001371d8`). The world's message file
is bound when an area loads (`FUN_00168ba8` -> `FUN_00190e70`, docs/hud.md).

**Twilight Town links** [V, `twilight_town_groups_exits_and_events` prints them per area as `exits:`]. Each exit
group has a walk-in volume (option 0) and a prompt volume (option 1 or 2). Examples: tt02 (the Usual Spot by the
community's names) -> tt03 entrance 0; tt03 -> tt02, tt04, tt06, tt37 (entrance 1), tt05, tt04; tt04 (the
Sandlot) -> tt03, tt06, tt07 (entrance 1 each); tt06 -> tt03, tt04, tt07, tt08, tt05, tt04; tt08 -> tt06, tt09,
tt37; tt40 -> tt19 and area 100. The full list is in the test output.

## 3. NPCs

- **Objects** [V]. `FUN_0016e8c0` creates an object by its `00objentry` type: type 15 (the townspeople `*_RTN`,
  e.g. `N_EX500_RTN`, `N_EX510_RTN`, `N_EX520_RTN` in tt02) -> 0xb40 bytes, `FUN_00189858` (object class 5, vtable
  `0x35ced8`; it holds two path followers `FUN_0018b0d8` at +0xa40 and +0xaa0); type 7 save and world points
  (section 4); type 8 (`F_EX000`) and others the default class `FUN_001a51e8`. Twilight Town spawns 50+
  different type-15 models; the most common are `N_EX510_RTN` (19 entities), `N_EX500_RTN` (16),
  `N_EX590_RTN`, `N_EX580_RTN`, `N_EX520_RTN` (15 each).
- **Scripts** [V]. The townsperson models carry their own BDX (`kh2 bdx N_EX500_RTN`: program `rtn_default`,
  2961 words, entry events 0 and 1 only). They use group 1 hosts and the group 5 library (5.0, 5.2..5.7, 5.11,
  5.16, 5.17, 5.26, 5.27, 5.33, 5.34). The group 5 wrappers read the object's path at +0x508 (`FUN_002ea230`,
  `FUN_002ea080` -> `FUN_002d2328`, `FUN_002d25b0`) and the class-5 helpers (`FUN_0018a8b0`, `FUN_0018af08`,
  `FUN_0018af10`, `FUN_00189e70`).
- **Walk paths** [V for the layout, I for the meaning]: 8-byte header (`u16` id, `u16` point count, `u16` kind 1
  or 2, `u16` 0), then 12 bytes per point (three f32: a position in the game's space). Twilight Town groups
  hold them with type-15 entities (e.g. tt02 `a72_`, ids 0xf3..0x2ff). Which entity follows which path: the
  script asks for it by id through host 1.177 (below, WP-E2); `rtn_default` asks for -1, the group's first path.

**The class-5 runtime and the walk (WP-E2)** [V unless marked]. Read for the port in `crates/kh2-app/src/npc.rs`:

- *Where the townspeople come from.* Any loaded group's type-15 entity (`FUN_00181728` makes every entity of a
  group). In tt02 the `map` program 0 loads none: the kind-6 groups `a72_`, `b72_`, `c72_` (one `N_EX5x0_RTN`
  each, 7 activators, one walk path, 9 return parameters) come from `evt` programs 0x10 and 0x12, `b50_` from
  0xc (`KH2_PROGRAMS=0,0,10` in the app, printed by its area log).
- *Construction* (`FUN_00189858`): two 0x60-byte "attention followers" at +0xa40 and +0xaa0 (`FUN_0018b0d8`
  -> `FUN_0018b0f0` zeroes kind +0, mode +4, target +0xc, +0x20, bytes +0x30 and +0x31), the floats 360, 720, 180,
  60 (`0x37ab48..`) into +0xb10.., flags +0x588 |= 8 and +0x600 bits. Init (vtable slot 2, `FUN_001899d0`): bits 0..6
  of the spawn record's word +0x24 go into the bit array at +0xb04 (host 5.17 tests, 5.27 sets a bit); with bit
  0 set the follower is put in kind 2, mode 0, and its motion (+8) comes from the group's return parameters when
  the group's kind is 6 (`FUN_00199ca0`): the last entry with byte 0 = 0 and byte 3 = 4 names it by byte 1
  through `FUN_0018b1e0` (0, 1, 3 -> 0; 2 -> 0x11f; 4..13 -> value + 0x117).
- *The path* (+0x508). Host 1.177 (`0x1719d0` -> `FUN_00183c30(obj, id, 0)`) looks the id up in the object's
  own group (`FUN_00199cc0`: the path header's u16; a negative id takes the first path) and builds a 0xa0-byte
  record (`FUN_002d3b40`, ctor `FUN_002d2100`): the points as 0x20-byte entries (position, tangent) at +0x80,
  +0x10 = header byte +4 bit 0 (the path loops), +0x20 = 0.5 unless bit 1, the tangents of a cubic spline
  (`FUN_002d32f8`), the place +0x18 (segment index plus fraction) set to the point nearest the object
  (`FUN_002d50c0` -> `FUN_002d4dc8`), direction +0x84 forward, +0x85 "turn around at the ends" (clear only for
  objects with flag 0x588 bit 3 whose bit 2 is set), +0x86 lock. Host 1.178 (`FUN_00183e18`) and 5.3
  (`0x2ea080`) call `FUN_002d25b0(speed, blend, out, path, position, facing)`: the speed is clipped to 50
  (`0x3837b8`), `FUN_002d2a98` takes the point at the place and, while it is within `speed` of the object
  (horizontal distance), moves the place by `speed / segment length * 0.5` (at most 20 times; at an end of a
  non-looping path `FUN_002d2328` flips +0x84 when +0x85 is set, else the walk stops and the facing is answered);
  the answer is the unit vector from the object to that point. 5.2 (`0x2ea230` -> `FUN_002d2328`) flips the
  direction unless locked. 5.16 (`0x2ea768` -> `FUN_002d3148`) picks the direction toward another object. The
  movement itself is the root motion of the walk motion, as for enemies (the script never sets a movement vector).
- *The curve* (WP-E2b, ported in `PathRun`). `FUN_002d3b40` copies the points (position w 1.0, `DAT_003837f8`;
  with bit 3 of the class-5 array every height is the first point's), +0x10 = header byte +4 bit 0 (loop), +0x20 =
  0.5 (`DAT_003837fc`) unless bit 1 (tension 0: tangents of zero, the chords walked with an ease), the lock +0x86 =
  array bit 2 and +0x85 = its opposite; with array bit 1 clear `FUN_002d5398` puts the object at a random place
  (`FUN_001063b8` of the segment count) walking a random way (`FUN_00106308(2)`), else `FUN_002d50c0` the nearest
  place. Tangents (`FUN_002d32f8`): fewer than three points: the chord, loop flag cleared; inner points (all
  points of a loop) (next - previous) * tension, w = the index; the ends of an open path: the quadratic Bezier
  through the end and its two neighbours, passing the middle at the share f = |P1-P0| / (|P1-P0| + |P2-P1|)
  (control M = (P1 - P0 g^2 - P2 f^2) / 2fg, g = 1 - f), its end derivative scaled so that its derivative at
  the middle has the inner rule's length (`DAT_003837e8` 2, `DAT_003837f0` -2, `DAT_003837f4` 4). The curve is
  the cubic Hermite (`FUN_002d4298` / `FUN_002d2460`, weights 2s^3-3s^2+1, -2s^3+3s^2, s^3-2s^2+s, s^3-s^2).
  `FUN_002d2a98` (record +0x87 clear, stop index +0x8c = -1 from the ctor `FUN_002d2100`): the object is first
  projected onto the curve (`FUN_002d51e0`: for more than two points the local search, `FUN_002d4ad8` on a loop /
  `FUN_002d4800` on an open path: the rest of the current segment ahead and the next one, each by 33 halvings
  `FUN_002d4688` against the chords `FUN_002d4040`, accepted below `speed * 10`, `DAT_003837d0`; else the
  global `FUN_002d4dc8`: the nearest chord, refined by halvings strictly inside (1e-4 .. 0.9999)); if the
  projection is as far from the object as the old place within 1e-4 (`DAT_003837d4`) the old place stays. The
  walking parameter (+0x1c) then steps `speed / chord * 0.5` up to 20 times until the curve point is more than
  `speed` (horizontally) from the projection; past a segment end it takes the next one (wrapping on a loop), at
  an open end it turns around (+0x85, one more pass) or stops. `FUN_002d25b0`: the direction from the object
  to that point when it is more than `speed * 10` away (`DAT_003837c0`), else (blend 1.0, `DAT_00383e80` for
  5.3, `DAT_003837c4` for 1.178) from the projection. The tt02 paths are loops of 11 to 14 points with tension
  0.5 (`tt02_townspeople_name_each_other_and_carry_their_parts` walks each round).
- *The script* (`rtn_default`, the same program in every `N_EX5x0_RTN` read). Init registers thirteen modes
  (`near_wait`, `near`, `rtn_freeze`, `before_check`, `after_talk`, `encount_player`, `look`, `rtn_avoid`,
  `rtn_action`, `path_move`, `talk`, `static_idle`, `fix_idle`), sets the walk speed 5.0 in its context (+0x44)
  and hands nine floats (360, 720, 180 three times) to host 5.33 (`FUN_00189990`: object +0xb10..). Main: for
  four object ids (1075, 1872, 2162, 2313/2314) it binds a companion (1.114, 1.18 link, 5.26 = +0xb08); with
  bit 5 of the array it clears the dash mover (1.58); with bit 0 it goes to `fix_idle` (plays the follower's
  motion from 5.5), else 1.177(-1) and `path_move` when a path exists, `static_idle` otherwise. `path_move`:
  motion 129 (a turn) first; each tick 5.3 gives the direction, 0.21 the angle to the facing; walking (state 1)
  it switches to 129 when the angle passes 90 degrees, else to 1 (the walk) when within 3.6 degrees;
  `turn_toward` by 5 degrees per tick; 5.0 (the follower's test) hands a follower of kind 4 / 2 / 5 / 3 / 1 / 6
  / 7 to `encount_player` / `rtn_action` / `rtn_action` / `rtn_avoid` / a direction flip / `fix_idle` (and
  bit 0 set) / `rtn_freeze`. `static_idle` waits for the player within 200 cm then queues `look` (head toward
  the player, 1.174, and turning with 129 while 1.172 says he is outside the 15 degree cone, `0x37b950`).
  Motion 129 is started with `play_motion_at` at the walk's frame when the walk plays (1.69, 1.61); from 129
  to the walk the script plays 0 and queues 1 (1.13).
- *What the class does on its own* (vtable slot 3, `FUN_00189ae8`, every frame): with bit 0 clear it runs
  `FUN_0018a130`, the native attention logic: a region test of the player against the object's parts
  (`FUN_0018aa08`, part 5) starts a kind-4 follower with a timer from +0xb10 / +0xb14 (`FUN_00106408`), a
  partner townsperson whose return entry names this one (`FUN_0018a598`, `FUN_0018a7a0`) a kind-5 pair
  (`FUN_0018a828`, +0xb1c / +0xb20), else the region the object stands in (`FUN_00199d68` over the kind-6
  group's activators, byte +0x6c + 2 of the region keyed in the return entries) picks a weighted return entry
  (`FUN_0018b308`: byte 2 is the weight of a hundred, byte 1 the motion, byte 3 the target mode: 1 an object
  by id at +4, 2 the player, 3 a point at +4..+0xc) into a kind-6 or kind-2 follower. 5.0 (`FUN_0018a8b0` ->
  `FUN_0018b148`) answers true when the follower is forced (+0x31) or its region (+0x20) test (`FUN_001d8c18`)
  passes against +0x28 / +0x2c, and sets +0x30; 5.11 (`FUN_0018b250`) answers 0 while a done follower's timer
  (+0x50, `FUN_0018b228`) runs and the kind-4 part test or the kind-5 partner (motion 0x32, `+0xa4c` back-link)
  holds; 5.4 (`FUN_0018ae08`) copies the follower to +0xaa0, resets it and loads the +0xb00 timer from +0xb18 /
  +0xb24 / +0xb30 by kind; 5.5 / 5.7 read the follower's words 0..2 and its byte +0x24; 5.6 (`FUN_0018af10`) the
  unit direction to its target. 5.34 (`FUN_0016bbe8`) is the object's part-0x11 point (`FUN_001894d0`) or its
  lock-on point.
- *The attention logic, ported (WP-E2b, `class_tick` in `npc.rs`).* `FUN_0018a130` per frame while array bit
  0 is clear (not linked, `FUN_001beb00`): the follower timer (+0x50, `FUN_0018b228`) and +0xb00 count down by
  1 (`DAT_00356f40`). Unless the follower is of kind 4, or the previous one (+0xaa0) was and +0xb00 runs: the
  player's type-1 body meeting the object's first type-5 part (`FUN_0018aa08`: `FUN_0016b558(obj, 5)`,
  `FUN_001894d0`, the player `DAT_0035c998`'s type 1, `FUN_00189208`) keeps the follower as the previous one
  (when it has a kind) and starts a forced kind-4 follower, mode 2 (the player), flag 0, timer
  `FUN_00106408(+0xb10, +0xb14)` (360..720 frames from `rtn_default`'s block). Unless the follower is of kind 4 or
  5 (or the previous was 5 and +0xb00 runs): `FUN_0018a598` finds a partner: `FUN_0018a918(obj, 0)` takes the
  first other object (spawn-record halfword +0x1e different, `FUN_0016b928`) whose first type-0 part meets this
  one's; it must have +0x588 bit 3 and a follower not done, and both objects' return entries with byte 0 = 0
  must name the other by objentry id at +4 (`FUN_0018a7a0`, `FUN_0016b078`: 0x236 is the player, else the
  first object of that objentry). Then one timer `FUN_00106408(+0xb1c, +0xb20)` and `FUN_0018a828` on both:
  forced kind 5, mode 1 toward the other, the entry's motion (`FUN_0018b1e0` of byte 1), flag 1 for this one
  and for the partner when the facings (`FUN_0016a890`, +0x550) point away from each other (dot < 0). Else,
  once +0xb00 ran out (it is zeroed), `FUN_0018a4b0`: the region the object stands in (its position 10 cm higher,
  `DAT_0037ab88`; `FUN_00199d68`: kind-6 group only, the first of the group's volumes in record order whose
  record halfword +0x2c is 0 and that contains the point, `FUN_00198750` makes one per activator) is compared
  with +0xa30; on a change, unless the follower is done, it is reset and, inside a region, `FUN_0018b308(+0xb28,
  +0xb2c, ...)` draws: a number below 100 (`FUN_00106308`) against the summed weights (byte 2) of the return
  entries whose byte 0 equals the volume record's +2 and byte 1 is not 3; the last entry with byte 1 = 3 gives
  the flag (+0x24) with the chance of its byte 2. The chosen entry makes kind 6 (byte 1 = 1) or kind 2, motion
  `FUN_0018b1e0(byte 1)`, target by byte 3 (1 object by id, 2 the player, 3 the point at +4..+0xc, else none),
  region +0x20, threshold +0x28 = `FUN_001063b8(1.0)` (`DAT_0037abd4`), start +0x2c = `FUN_001d8c18(volume,
  position)` = (x' + sx) / 2sx in the volume's frame (sx at volume +0x50 = record +0x10), timer
  `FUN_00106408(+0xb28, +0xb2c)`. 5.0 (`FUN_0018b148`) passes when forced, or across the region past the
  threshold (at or below it when the start was >= 0.5, `DAT_0037ab98`, else at or above); 5.11 (`FUN_0018b250`)
  holds (0) while done with the timer running and: kind 4 the parts still meet, kind 5 the partner exists,
  points back (+0xa4c) and does not play motion 0x32 (+0x154), any other kind always; 5.6 (`FUN_0018af10`):
  live (forced or a region) with a mode: the target at height 0 (mode 3: the point with its own height) minus
  the object at height 0, normalized, else the facing. 5.4 (`FUN_0018ae08`) reloads +0xb00 always from +0xb30:
  it reads the kind after `FUN_0018b0f0` cleared it (instructions 0x18aeb0 `jal 0x18b0f0`, 0x18aeb8 `lw 0xa40`).
  tt02 data: the three townspeople (0x51d, 0x51e, 0x51f) name each other in byte-1 = 2 entries (motion 0x11f,
  `rtn_action`); their volumes are keyed 0, 1, 2: key 1 stops 30 of 100 (idle motion, flag 30), key 2 40 idle
  and 20 looking at the player (flag 50). Parts: type 0 a sphere of 90 lifted 115, type 5 a sphere of 200 at
  200 up, both on the object's own frame; Sora's type 1 an ellipsoid 35 / 55 lifted 85. Log: `KH2_NPCLOG=1`
  prints `attention: the player meets the talk part`, `attention: paired with`, `attention: region`.
  [approx] the game's one global `rand()` is a per-object generator; the object list searched for partners and
  ids holds the townspeople and the player only; the class pass runs before the script's tick and not before
  the script's first tick [inferred order]; the parts' enable masks are not applied.
- **Still not ported (GAP)**: 1.172 / 1.173 use the 15 degree cone without the part's own (`FUN_001af878`;
  `N_EX500_RTN` has no type-0x14 part, but `N_EX510_RTN` and `N_EX520_RTN` do: the cone stays a gap); the
  head look-at controller (hosts 1.174 / 1.176) is ported, see "Head look-at" below, with 1.175 (a point)
  and the pose code's own joint turn left. 1.13 on the object starts
  the queued motion when the playing one ends; 5.34 answers the position (the tt models have no part 0x11; the
  lock-on fallback `FUN_0016b6e0` reads a type-3 part on a bone). The native `FUN_00189ae8` also raises +0x108
  bit 6 after five frames of spawn kind 2 without `FUN_00190668` (not read further); `FUN_00189ff0` (the
  class's reaction to a talk: it ends the look-at, sets the previous follower to kind 4 and +0xb00 to +0xb18 +
  `DAT_0037ab84`) is for E3.
- **Talk** [V; closed by WP-E3]. A type-5 reaction part with id 0x83 offers the command in the
  halfword +0x2c of the object's spawn record (docs/limits-reactions.md: 31 "talk" for most townspeople;
  `twilight_town_talk_programs` counts 206 townspeople with 31, one with 30, others 209..213, 293,
  555..559, 633). Command 31 has `Execute` 31 (0x1f). Earlier notes said every reaction command reaches
  the target's script as event 0xb; **that is not so for the talk**: Sora's class handler
  `FUN_001cf030` (decompiled for this WP: its own cases are Execute 0x48, 4, 5, 0x51, 0x49, 0x13c)
  hands everything else to `FUN_00177b18`, which has its own cases before the generic path
  `FUN_00177c20`: 0x1e (`FUN_00177d28` + `FUN_001a54a8`), **0x1f (`FUN_00177d28` + `FUN_001a56c0`)**,
  0x20 (`FUN_001a6db8`), 0x37 (`FUN_001784b8`, section 4) and 0x7f (`FUN_00197b70(record +6, 0)`). So
  the talk bumps no use counter, applies no CmdAction and sends no event 0xb (which is why
  `rtn_default` has no handler for it).
  - `FUN_00177d28` clears the player's vectors +0x20, +0x10 and his target record's +0x9c0.
    `FUN_001a56c0`: `FUN_001bef48(0xc)` (HUD list record 0xc), `FUN_001b7ce8(0)` (bit 0 of
    0x1d5d7b8: the player's control is locked; `FUN_001b7d50` tests the bit of the object's objentry
    type, read through `FUN_00176a18` by `FUN_001794f0`), then `FUN_001a53f8(target + 0x500, player,
    target record, task FUN_001a5728, callback FUN_001a5a60)`.
  - **The talk table** (`kh2_ard::libretto`). `FUN_0016b870` copies the spawn record's halfword
    +0x28 into the object's +0x500 (when not 0): the talk id. `FUN_001b6408` (called by `FUN_00168ba8`
    at area load) loads `libretto-%s.bar` with the world prefix (`FUN_001b6398`, format at 0x35f1d4)
    and keeps its first type-2 entry: a u32 count at +4, records of 8 bytes from +8 (`u16 id, u16 n,
    u32 offset`), searched by binary search (`FUN_001b64b0` -> `FUN_0030ff40`, compare
    `FUN_00332dc8` = key - id). `FUN_001a53f8` takes a reader from a pool of 8 (`FUN_001b6740`,
    `FUN_001b6510`) and keeps it at +0x504; the program is `u16 op, u16 count, count u32` commands
    ended by op 0 (`FUN_001d8d18`); `FUN_001b65e0` runs 2 wait (`FUN_00102460`), 3 fire event
    (`FUN_00197b70(w, 0)`), 4 stop (the place is kept: the next talk goes on after it,
    `FUN_001b6578` / `FUN_001a53b8`), 5..8 fades (`FUN_0014a7c8` / `FUN_0014a7f0`, time x
    `DAT_0037bb08` 0.5); other ops go to the callback: `FUN_001a5a60` op 1 = a message window of class
    `FUN_001aba58` (the same values as `FUN_001ab9e0`: font 3, colour 5, (256, 336), frame kind 0, the
    speech tail `FUN_001aa628(1, 1, 0)`) with the word as message id, registered (`FUN_001aad78`),
    waited for until its state (+0x70) is 3, then closed through its +0x20. The wait also ends when
    `FUN_001a5b18(10)` answers 0 (`FUN_0019abb0` of the player with command row 10) [GAP: which
    player state that is; not ported]. On the disc (`twilight_town_talk_programs`): 305 talk ids,
    ops used 1 (333) and 3 (4, event 101 = 0x65 after a message, tt11/tt12/tt36 `b51m`); nine tt
    entities name talk id 0x4a6 or 0x402, which have no record (they talk to nothing). tt02's four
    townspeople (`a72_`, `b72_`, `c72_`, `b50_`) each say one message of two pages.
  - **The task** `FUN_001a5728`: player record +0xb40 +0x2c bit 0 and 0x35d268 bit 8 are set, the
    target's +0x108 bit 2 (its only reader found is `FUN_0019a530`); the player faces the target
    (`FUN_0016a948` with `FUN_0018efa0` of the target record minus his position); the talk camera
    (`FUN_00159688`: camera kind 4 with the record, `FUN_00140d08` +0x40 = 4; second argument 1, or 0
    for a class-5 target whose array has bit 0x10 or 0x40, `FUN_00189e90`) [ported in WP-E8: docs/camera.md
    "Talk camera"];
    the three party members turn their heads (`FUN_001abf40` on +0x82c) [not ported]; the target faces
    the player (`FUN_0016a828`, or for class-5 objects `FUN_00189f40`: unless array bit 4, and its
    companion +0xb08 plays motion 0x32) and plays motion 0x32 (`FUN_0017e220`); `FUN_00192c50(target,
    0x21)` (on the object's +0x670 list, meaning GAP). After the program, unless an event runs
    (`FUN_00196418`): the facing comes back, `FUN_00192c68`, the class-5 end `FUN_00189ff0` (motion
    = the follower's +0xa48 with array bit 0, else 0; with bit 0 clear the head look-at ends, the
    previous follower +0xaa0 takes kind 4, the follower is reset and +0xb00 = +0xb18 + 60
    (`DAT_0037ab84`); the script goes to mode "after_talk" (`FUN_001870d8` / `FUN_00186db0`, string
    0x37ab78) when it has one), the camera comes back (`FUN_00159768`), the heads, the lock bit
    (`FUN_001b7d18`) and the flags are cleared, `FUN_001befc0(0xc)`. Command 30 is **Examine** (message
    0x81eb; WP-SH1 corrects "the shop people"): `FUN_001a54a8` runs the same table with the task `FUN_001a5510`
    (the player faces the target; no talk camera, no motion 0x32) and callback `FUN_001a5608` (window class
    `FUN_001ab7d8`, wait `FUN_001a5b18(0xe)`) around the script's "before_check" mode (`FUN_00189eb8`, string
    0x37ab68) [ported in WP-SH1: `npc::talk_step`, window class 2]. The shops are the "Shop" rows (Execute 0x7f,
    `FUN_00177b18` case 0x7f -> `FUN_00197b70`): docs/menus.md section 21.
  - **The window** (docs/hud.md "Message windows"). The base update `FUN_001ab170`: state 1 waits
    for the page (`FUN_001a8b48`) and starts the prompt (`FUN_001aa3a0`), state 2 reads the pad
    (`FUN_001aa430`: logical bits 0x40c, sound 0x18 `FUN_002c14a8`, then `FUN_001aacd8`: the next page
    when `FUN_001a8c78` -> `FUN_00190fa8` finds text after a command 0x10 (back to state 1), else the
    window's method +0 (`FUN_001ab328` for this class: prompt group 0x42, state 3)); the owner's +0x20
    (`FUN_001ab358` -> `FUN_001aacb0`: state 4, `FUN_00197b70(0x22, +0x80)`), state 4 the tail's
    closing group (`FUN_001aa680`: group + 1), 5 waits for it and starts the frame's closing group, 6
    waits, 7 gone. The tail is drawn from state 1 on at (x + w x -0.5, y - h / 2) (`FUN_001ab370` ->
    `FUN_001aa6b8`, `DAT_0035ea68` = {-0.5, -0.15, 0.15, 0.5}).

### Head look-at (WP-HL1)

What turns the head: the controller at object +0x4a0 (0x60 bytes), updated every tick by `FUN_001aea28` and read
by the motion code (`FUN_0017e6c0`, `FUN_0017e928`: `FUN_001aead0(obj + 0x4a0)` is the last argument of the
pose call `FUN_0011c2a0` / `FUN_0011c460`). Fields: +4 mode (0 off, 1 toward a point, 2 toward an object, 3
toward a direction), +8 low bit "needs the object's +0x160 bit 0x4000000" (set to 1 by the script hosts),
+0xc the joint, +0x10 / +0x14 the weight and its step per tick (`FUN_001abf40` sets, `FUN_001abf78`
advances: clamps at 0 and 1), +0x20 the aim point (w = the eased weight), +0x30 the target object, +0x40
the last look direction, +0x50 the head's yaw against the body.

Who uses it: the objects whose model has a collision record of type 0x14 (the BAR entry 23 table). Of the
disc's `obj/*.mdlx` listed in `local/kh2idx.txt`, 110 have one, all `*_RTN` townspeople; `N_EX510_RTN` and
`N_EX520_RTN` of tt02 have it, `N_EX500_RTN` has not, Sora's table has none. The record's halfword +6 is the
head bone (`N_EX510_RTN` 128, `N_EX520_RTN` 68, `N_EX680_TT_MAN_A_RTN` 62), its offset (+8..+0xc) a point in
the bone's frame (7 or 9 along x), and its two radii (+0x10, +0x12; 50 and 20 for most) are the yaw and
pitch limits in degrees (`DAT_0037b938` = pi / 180). `FUN_001aece8` (1.174) does nothing for an object
without the record. In the townspeople's scripts (`rtn_default`, mode `encount_player`; 683 calls in the
disc's scripts, `KH2_BDXLOG=1`) 1.174 is called with 20 frames when the kind-4 follower starts and 1.176
with 20 frames when it ends; 1.175 is in none of them. `FUN_00189dc8` (a class-5 companion's mode 1) and
`FUN_00228778` (mode 3 and the end, from the event player `FUN_00223a88`) are the other writers, not ported.

The rule (host 1.174 = `FUN_001aece8`, mode 2): the weight goes from 0 to 1 over the frames (`DAT_0037b930`
= 1; at once for frames of 0 or less) and the head's yaw against the body (+0x50) starts at 0. Every tick
(`FUN_001aea28`): advance the weight; a weight of 0 ends the mode; if the target object is gone
(`FUN_0019a3f8`) the mode ends; else `FUN_001aee90`:

1. The head point is the record's centre (`FUN_001894d0`: the bone's matrix times the offset, y lifted by
   +0xe); the target point is `FUN_0016bbe8(target)`: the target's type-0x11 record's centre, else its
   lock-on point. The direction to it gives a target yaw (horizontal) and a target pitch.
2. Yaw: the head's current yaw is the body's facing turned by +0x50. If the target yaw is more than 5
   degrees away (`DAT_0037b93c`) the head turns 5 degrees toward it (sign from the cross product's y), else
   it takes the target yaw whole. Then, if that is more than the record's yaw limit from the body's facing,
   it is put at the limit on that side.
3. Pitch: the last look direction's pitch (+0x40) against the target's: only when they differ by more than
   3 degrees (`DAT_0037b944`) does the pitch move, by exactly 3 degrees toward it (else it stays); then it
   is clamped to the pitch limit of the body's pitch (level for an upright object).
4. The new look direction is stored at +0x40, +0x50 = the signed angle between the body's facing and the
   new horizontal direction, and +0x20 = the head point plus the direction times the distance, w = 1
   (`DAT_0037b94c`).

The pose code gets +0x20 and the weight eased by `FUN_001aead0`: `(1 - cos(pi w)) / 2` (`DAT_0037b920` = pi,
`DAT_0037b924` = 1, `DAT_0037b928` = 0.5; `FUN_003000d8` is read as the cosine, which is what a 0 to 1
ease needs [inferred]). Host 1.176 (`FUN_001aec50`) tweens the weight from where it is to 0 over its frames;
the mode ends when it arrives.

In the app (`npc::look`, `skel::JointTurn`): `Look` holds mode, weight, step, +0x50 and the pitch;
`Look::tick` is steps 2 and 3 above in the app's space (the game's space is the app's turned half a turn
about x, which keeps the turning direction, so no sign mapping is needed); `npc_update` calls it after the
townsperson's move with the head point from the last pose (`pose[joint].transform_point3(offset)` in the
object's frame, as the talk camera reads bone records) and Sora's type-0x11 record (joint 0x8c, offset
(3, 0, 0)) read the same way from his own last pose. The result is a rotation (yaw +0x50, then pitch) scaled
from nothing by the eased weight, handed to the animator, which rotates the head bone and everything below
it about the head bone's origin on top of the motion's pose (`skel::turned_pose`; the pose itself is not
changed, so cross-fades start from the motion's pose). Log: `KH2_NPCLOG=1` prints `head look-at Start(..)` /
`Release(..)` and once a second the yaw, pitch and weight.

Harness: tt02 with `KH2_PROGRAMS=0,0,10 KH2_NPC_WATCH=2 KH2_NPC_WATCH_DIST=150 KH2_NO_ENEMIES=1`, shot at
9 s: the townsperson stands behind Sora with only his head showing; before the port his head follows the
idle motion (turned away, eyes shut), after it the head faces Sora's side (the 50 degree yaw limit and the
20 degree pitch limit, the target being 20 cm from him).

[inferred] and not done: (a) how the pose code behind `FUN_0011c2a0` (a virtual of the model class, vtable
+0xc of the pose object) turns the joint toward the aim point was not traced; the app applies the yaw and
pitch as a rotation of the head bone about its own origin and assumes the animation's head looks along the
body's facing. (b) The enable test of `FUN_001ae9e8` (bit 0 of +8 set by the hosts: then the object's +0x160
bit 0x4000000 must be set; its writer was not found) is not read: the controller runs while the object
exists. (c) The target of 1.174 is taken to be the player: its first argument arrives as 0x20001010 (not the
object's own handle) and the block the second one points at is the object's own address plus 0x40; the
scripts' `encount_player` passes the object `R03eb` returns for the player. (d) +0x40's initial value is
the target's facing vector (`FUN_0016a890`); the app starts level. (e) Modes 1 and 3 (`FUN_001aec78`,
`FUN_001aeda8`), 1.175, the class-5 companion writers and the party members' head turns of the talk
(`FUN_001abf40` on +0x82c) are not ported. (f) A target straight above or below the head has no horizontal
direction: the yaw stays (the game divides by a zero length). Tests: `npc::look::tests::*`,
`npc::tests::the_head_record_names_the_head_bone_and_the_limits`,
`skel::tests::a_joint_turn_rotates_the_subtree_about_the_joint`.

## 4. Save points and world points

- **Objects** [V]. `SAVE_POINT` (objentry 570) and `WORLD_POINT` (569) have type 7 -> `FUN_00194ab8` (0xa00 bytes,
  vtable `0x35e300`): sets object flag +0x588 |= 0x1000, +0x9fd = 1, starts its own effect element 1 (`FUN_0016a570(obj, 1,
  ..)`, handle +0x9f4, ended by `FUN_00194b68`). For object 569 (the world point) the pair (world, area) is looked
  up in `00worldpoint.bin` (`FUN_00194f68`: 4-byte records world, area, entrance, -; at most 0x36; the file is 216
  bytes, 0x36 records) and that point's bit is set at `0x33f910` (`FUN_00194fa8`; `FUN_00195010` answers 0 unseen,
  1 seen, 2 seen and locked). In tt02 they stand in `m_20` (world point) and `m_21` (save point), both at the same
  place; with the programs (0, 0, 0) the app loads `m_20` only (run below; tt02 is record 47). The object files hold no mesh and no motion
  set: one collision record (type 5, id 55, shape 1 = a cylinder of radius 50 and half height 50, lifted 50) and
  the effect file `obj/<name>.apdx` (elements 0, 1, 2) [V, `savepoint::tests::the_points_on_the_disc`].
- **The offer** [V]. The part offers command 55 like any object part (`FUN_0019cb70`). The cylinder against the
  player's ellipsoid is `FUN_00188fd0` (cylinder pairs `FUN_001890d8`; dispatch `FUN_00189208`; ported in
  `savepoint::overlaps`, the other pairs stay `reaction_parts::overlaps`). Its status (`FUN_001ba2c0` case 0x37):
  5 while the word 0x35fec0 has any bit (`FUN_001c6c08`: bit 1 the drive form start `FUN_001b44c8`, bit 2 a summon
  `FUN_001b4170`, bit 3 `FUN_001c6578`), then the flags and form mask as for every command.
- **On approach** [V]. The player's reaction manager `FUN_0019c0f0`: while the offered command (+0x26) is 55 and
  the offer status is below 4, `FUN_00194c08(the point)` runs every frame over the four party slots: a member
  whose object is not in the world (status block +0x24c = 0) gets its MP refilled (`FUN_001a0240`) and its maximum
  HP added (`FUN_001a0100`); a member in the world that is full (HP at maximum, no MP charge `FUN_001a05d0`, MP at
  maximum) is left; else it gains **1 HP** (`FUN_001a0140` -> method +0x78 = `FUN_001792f0` -> `FUN_00176c98` ->
  `FUN_00186238`) and **1 MP** (`FUN_001a0210` -> `FUN_00186368` -> `FUN_001a0180`; during a charge the charge's
  time is shortened by the factor at `DAT_00360130` +0x7c instead, `FUN_001a0540`) and +0x9fc is set; the first
  call also sets +0x9fc (+0x9fd). The point's update `FUN_00194b98` starts effect element 2 (handle +0x9f8) when
  +0x9fc is set and none runs, and ends a running one (`FUN_001d9f20`) when it is clear; +0x9fc is cleared every
  frame. So the heal is gradual (60 HP a second) while the prompt is shown, not on use, and the heal effect plays
  only while something is restored. (Earlier notes read "restored to their maximum": corrected.)
- **On use** [V]. Command 55 (`Execute` 0x37, message 34054) does not reach the point's script: `FUN_00177b18`
  case 0x37 = `FUN_001784b8(player, point)`: party reset kind 7 (`FUN_001abfe8`), every member's method +0x88
  (`FUN_00176758`), byte 0x33e86e = the low byte of the point's spawn word +0x20 (`FUN_0016b940(point, 1)` reads
  +0x820, copied by `FUN_0016b870`), bit 0 of 0x33febc = the player's objentry kind is 0xe (`FUN_001699a8`), then
  `FUN_0024f0b8(1, 0)` for the world point (objentry 0x239) or `FUN_0024f0b8(1, 2)` for the save point: **menu kind
  1** (scene 3, init `FUN_00255598`, which sets the three message ids 0x9fcf..0x9fd1 or a list from 0x380cd0 via
  `FUN_00254bf0`, and starts the task 0x2556f0), not the field save menu kind 7. **Kind 1 is the points' save
  screen** [V, WP-E7, read from the instructions of 0x2556f0 and the decompiled callees; docs/menus.md 18.5]: the
  same memory card driver and `save.2ld` groups as the file screens of kind 2, with a top list of "Save" (0x8a7e)
  and "World Map" (0x8a80); the mode word (0x36cd28, `FUN_0024f498`) 2 drops the World Map row, so the save point
  shows only "Save" and the world point both. `menu_save.rs` serves it (`SaveMenu::request_point`).

## 5. Story progress

- **Where** [V]. Flags are bits at `0x33f6b0` (save block): flag `id` is bit `id & 0x1f` of word `(id & 0x3ff) >> 5`
  of the 0x20-byte page `id >> 10` (`FUN_001802e0`; 0xffff is always clear). So each world owns `0x400 * world`
  upwards and uses 256 flags (`flag_count` finds 256 in the tt page). Test: `FUN_001802e0` (host 0.15), clear:
  `FUN_00180298`, set: `FUN_0017ff60` (host 0.14, opcode 0x19, trigger sub 2, `FUN_00180350`).
- **Side effects** [V]. Setting a flag runs its list from `00progress.bin` (BAR; `FUN_0017fd10` stores the 19
  type-2 world entries at `0x35ccc0` and hands the named entries `dsa`, `wldf`, `epsd`, `wlds`, `link`, `wmky` and the
  type-0x2c entry to other systems): commands `u8 op, u8 n, n halfwords`, op 0 ends (`kh2_ard::progress`). Ops: 1
  area programs (records `area, map, btl, evt` written to `0x33e86f + world * 0xc0 + area * 3`, 0xff keeps), 2..5
  `FUN_00195260` (lock entry h0 = state h1, message h2) / `FUN_00195288` (state 0) / `FUN_001952a0` (state
  0xffff, clears the world-point bit) / `FUN_00195300` (state 0, clears the bit) on the `dsa` entry h0, 6 `FUN_001a7730(world, x)`, 7 clear flags, 8
  / 9 `FUN_001a52b0` / `FUN_001a52f0` per halfword, 0xc set flags (recursive), 0xd two bytes of the world's record
  (`FUN_001b4e80`). After the list: `FUN_001803b0`, `FUN_00180450`, `FUN_001805d0`, `FUN_001d39d0`,
  `FUN_001d3238`, `FUN_001b6da0`, `FUN_001be9c8`, `FUN_001d5718`, `FUN_00195130`. Twilight Town: 186 of 256 flags
  have a list; ops used 1 x116, 2 x55, 3 x39, 5 x28, 6 x71, 7 x18, 8 x8, 0xc x61, 0xd x28.
- **Which battle program runs in tt04** [V]. Three ways set tt04's btl byte: (a) a flag's op 1 record for area 4:
  0x814 sets (map, btl, evt) = (0, 0, 0), 0x883 btl 1, 0x891 btl 0 (the others for area 4 leave btl alone: 0x833,
  0x841, 0x844, 0x869, 0x875, 0x876, 0x880, 0x895, 0x8ab, 0x8b6, 0x8e9, 0x8ea, 0x8ec, 0x8f6 change map or evt);
  (b) a type-2 jump to a tt04 story program (0x4c..0x53, 0xb2, 0xb5..0xb8, 0xcc, 0xcd) sets all three to it; (c) a
  jump record or host 1.30 with explicit bytes. tt04's `btl` script has programs 0x4c, 0x4d, 0x4e, 0x1, 0x50, 0xb6,
  0x4, 0xb7, 0x5, 0xb8, 0x4f, 0x7, 0x1b, 0xb5, 0x1c. Example chain from the triggers of tt04's `evt` script:
  program 0x4c on event 0x4c (the next event of a chain) plays event "110" and jumps (type 2) to program 0x4d;
  program 0x4d on 0x26 plays "111" then goes to 0x4e; 0xb6/0xb7/0xb8 (the Struggle bouts, docs/enemies.md) on 0x8b
  repeat themselves, on 0x8d go (type 1) to tt04 entrance 0x37 with world record value 1. (The event tag is the
  u32 of sub 0 read as four characters: halfwords 0x3131, 0x30 are "110".)
- **Scripts read/write** them through hosts 0.14, 0.15, 0.70 (flag 0x102b), and the party, menu and drop code
  reads them (docs/party.md, `memt`).

## 6. The event player

- **Where events live** [V]. In the area file, as BAR entries of type 0x16 named by number (`102`, `117`, ...;
  `FUN_00180a28` finds one by tag). Twilight Town has 190. Animation files are loaded by name: `anm/` + world +
  `/` + model + `/` + name + `.anb` (`FUN_0022cbb0`, strings at `0x37f208`, `0x37f210`, `0x37f218`); also
  `event/layout/%s/%s.2ld`, `movie/%s/%s.pss` (`FUN_00225860`, event opcode 0x44), `voice/%s/event/` + name + `.vag`
  (`FUN_0022bff8`), `event/effect/` + `.pax` [V for the strings and their users].
- **Format** [V]. A list of commands `u16 size (bytes, header included), u16 opcode, operands`, ended by a size of 0
  (`kh2_ard::parse_event`; every tt event parses to its last two bytes). Command 0 carries the memory size at +4
  and the event's name at +0xa (`FUN_00223910`: `tt_event_102`). Opcodes used in tt (hex counts): 0 be, 1 50c,
  2 f57, 3 be, 6 56f, 8 be, 9 496, a 168, b 238, c 18, d 239, e 6a, f be, 10 be, 12 129, 13 566, 14 1b4, 15 8f0,
  16 1a5, 17 73, 18 110, 19 790, 1a 721, 1b 2e, 1d 36, 1e 36, 1f 33, 20 6, 22 88, 23 74, 24 1ea, 27 12, 28 6, 29 bc,
  2a b4, 2b 41d, 2c 16f1, 2d 1d1, 2e c0, 2f 31, 30 31, 35 7, 36 6b, 3a e, 3d 7, 3f 42, 40 2, 42 12, 44 5, 47 70,
  49 10, 4b 8, 4d 6, 4f a, 53 e.
- **Two readers** [V]. `FUN_002245d0` walks the list once to set up (cases 1, 8, 0xf, 0x10, 0x13 (camera: sets
  `DAT_0036bd21` = 1, `FUN_0022b4f8`), 0x1d, 0x1e, 0x23, 0x24, 0x2a, 0x2e, 0x2f, 0x32, 0x44, 0x46, 0x4c, 0x4e); at
  the end it loads (`FUN_00222a88`), and sets the ready flag `0x36bcf4`. The thread `FUN_00223a88` (started by
  `FUN_002248f0`) waits for ready, then runs the timeline commands (cases 2, 6, 9..0xe, 0x12, 0x15..0x1c, 0x1f,
  0x20..0x22, 0x27..0x29, 0x2b..0x2e, 0x30, 0x31, 0x33..0x37, 0x3a..0x3d, 0x3f..0x45, 0x47..0x4b, 0x4d, 0x4f..0x53).
  A timeline command's first operand is a frame: `FUN_00224a90(frame)` yields until the event's frame counter
  `DAT_0036bcf0` reaches it, and the command is dropped when the event left the play state or is being skipped
  (`FUN_00222630`).
- **Per frame** [V]. `FUN_002233d8`: when the event has a camera, `FUN_0015c8f8(DAT_0036bcdc)` (the override of
  docs/camera.md); pad/fade handling; `FUN_00224fa8` (frame step); `FUN_001963e0`. The camera per frame is
  `FUN_0022b0f8` (called from the setup reader's frame path at `0x22481c`): it counts the entries of the
  128-entry table `DAT_0036bbb4` (`id, u16 start, u16 end`) active at the current frame and feeds the track to
  `FUN_0015ce00`. Opcode 6 commands in tt events are `u16 index, u16 start frame, u16 end frame, u16 0`, which
  matches that table [I: the camera cuts]. The camera keys themselves (opcode 0x13 lists of counts) are not
  decoded [GAP: `FUN_0022b4f8`, `FUN_0022b0f8` read].
- **Actors** [V for the layout seen, I for the meaning]: opcode 1 = `u16 objentry id, u16 actor index, name`
  (`p_ex110`, `h_ex510`, ... in tt02 event 102); opcode 0x24 lists the `.anb` motions per actor
  (`anm_tt/...`); opcode 2 places an actor (index, position floats). Motion playback: `FUN_0022cc88` builds the
  `.anb` name; the rest not read.
- **Message lines** [V]. Opcode 0x2d (`FUN_002275a8`): +4 frame, +6 key, +8 message id, +0xa action (0 open,
  1 close, 2 wait until the window reaches state 3), +0xc window kind (0: class 0x1ab6a8, 1: class 0x1ab8e0;
  docs/hud.md). [V, WP-E6b] Action 1 finds the slot of (key +6, message +8) among the 32, calls the window's
  close method (vtable +0x20) and clears the slot, so the wait that follows finds nothing and returns at once
  (close without waiting). Action 2 finds the slot and, unless the event is skipped (`FUN_002225e0`), sets
  `DAT_0036b9c9` (`FUN_00222528`) and yields until the window's state (+0x70) is 3, then clears it
  (`FUN_00222538`); while `DAT_0036b9c9` is set `FUN_00222588` answers 1 and `FUN_00222728` does not advance the
  counter: the event's frame stands still until the window waits for the button. Opcode 0x15 (`FUN_00227478`): +4 frame, +6 key, +8 message id, +0xa 0 open / else close, the
  subtitle line class 0x1aae60. Up to 32 open windows at `0x1dc00e0` keyed by (key, message id). In tt02 event 102
  the 0x15 lines open and close ids 0x3376..0x337b.
- **Start and end** [V]. A trigger with an event tag calls `FUN_00196428`: a 0xb0-byte event object
  (`FUN_00195898`: kind row from the argument, `FUN_001d1128`; bit 7 of the argument -> `FUN_00138708(1)`), the
  jump record copied into it, `FUN_00195ae0` starts the player (`FUN_002232c8`), stores the object at
  `DAT_0035e3a4` (host 1.34 `event_active` reads it), hides HUD parts (`FUN_001bef48(0xc)`, `FUN_0014c748(4)`).
  `FUN_00195c08` steps the object's own states (0..9). When the event ends (`FUN_001964d0`) the chain event 0x4c
  fires for kinds with bit 0; the event's end (`FUN_00225028`) fires 0x12 or 99. Other starters: `FUN_001ad1c0`,
  `FUN_001ad378` (revive), `FUN_001b3468`.
- **What the start holds back, and what the end gives back** [V, WP-E6b]. `FUN_00195898` takes the kind row of
  `arg & 0x3f` (`FUN_001d1128`: `FUN_0030ff40` searches the `evtp` rows of `03system.bin`, 8 bytes, by their kind
  byte; +2 the flags; `struggle::event_kinds`). `FUN_00195ae0` then: flag bit 3 clear -> `FUN_00196280`
  (`FUN_001b7ce8(0)`, `(1)`: the players' control bits of 0x1d5d7b8); bit 2 clear -> `FUN_00196760` (`FUN_0014ef48`
  -> `FUN_0013da78`: the field HUD's draw tags 3 to 6, docs/struggle.md). The two calls the brief named are not HUD
  hides: `FUN_001bef48(0xc)` (disassembled: `a0` passes to `FUN_001bef10`, a search of the 0x40-byte rows of
  `DAT_01d5df08`, whose row is copied into slot `row +0x24` of 0x1d5dd88; `FUN_001befc0(0xc)` clears the slot)
  is pause-menu record 0xc (docs/struggle.md: the pause-menu record table), and `FUN_0014c748(4)` sets bit 4 of
  `DAT_00356f1c`, which only `FUN_0014c780` reads (callers `FUN_0014a080` pause menu, `FUN_0014c4a8`, `FUN_0014c7b8`
  the play clock): both are undone by `FUN_001959f0` (`FUN_0014c760(4)`, `FUN_001befc0(0xc)`) when the object is
  destroyed. **Nothing hides the player or the party models**: `FUN_00195ae0`, `FUN_00195d28` (state 0: the fade by
  kind row +1, `FUN_0014a918` / `FUN_0015d058`), `FUN_00196280` and `FUN_00196760` touch control bits, faders and
  HUD only [the event's own commands may hide actors: opcodes 3, 0xd, 0xf, 0x10 not read].
  State 9 (`FUN_00196170`): `FUN_0019d9c8(+0xc)` (the jump record's word 0 is not 0) decides; without a jump the
  fade-ins of kind flag bit 4 (`FUN_0014a7f0(.., 6, 0)` on two faders), `FUN_0016f8e0(-200)` for flag bit 0,
  control back (`FUN_001962a0`) and, for bit 2 clear, the field HUD back (`FUN_00196790`); with or without, it then
  calls `FUN_0019d9d8(+0xc, 0x80000000)`, the trigger jump of `area_jump.rs` with bit 31 in the fade flags
  (`FUN_00104e18`).
- **Skipping** [GAP]: `FUN_00224b18` (an event is running and `FUN_002c17e0`) and `FUN_00222630` drop commands;
  the pad check and what a skip jumps to were not read.
- **The timeline cursor** [V, WP-E6]. The setup reader and the timeline thread each keep one offset into the
  list (`DAT_0036ba28`, `DAT_0036ba24`); the thread runs the commands in FILE order, and a command of its switch
  waits in `FUN_00224a90` until `DAT_0036bcf0` reaches its frame, so a later command with an earlier frame runs
  right after the one before it. Commands outside the switch (the setup ones among them) pass without waiting.
  Frames: opcode 2 at +0x26, 6 and 0x1f none, the rest +4 [V for 0x12, 0x15, 0x2c, 0x2d, 0x44; inferred for the
  others]. The counter (`FUN_00222728`): `DAT_0036bcec += DAT_00356f40` (1.0) a game frame, times `DAT_0037ec50`
  (0.5); while a movie plays (`FUN_002f5ed0`) it is the movie's frame (`FUN_002f5eb8(7, 0)`). The end
  (`FUN_00222668`): once `DAT_0036bcf8` (opcode 8, -1 when absent, `FUN_002237b8`) is reached, `FUN_002235c0`
  (state 4). A size-0 command parks the thread for good.
- **Actor table** [V, WP-E6]. `DAT_0036bbcc`, 0x60 rows of 0x28 bytes (`FUN_00224548` takes the first row with
  word 0 = -1): word 0 the objentry id (opcode 1 +4), word 1 the actor index (+6), word 2 the object (0 until
  made), +0x17 the model name (+8). `FUN_002290e8` finds a row by objentry id (the model name for `.anb`
  paths), `FUN_002244b0` by actor index.
- **Load blocks, opcode 0x24** [V, WP-E6]. `FUN_00222a48` lists the command (64 slots, `DAT_0036bbd8`);
  `FUN_00222a88` takes the next block while `DAT_0036bcf0` lies within its +6..+8 frames. +4 counts the
  sub-records from +0xc (`u16 kind, u16 size`): 0x25 a motion (`FUN_0022cc88`: +4 objentry id, +6 actor
  index, +8 flag, +0xa name `anm_tt/ETT10201E`), 0x26 (`FUN_0022c070`), 0x38 a model (+4 objentry id, +6 index,
  +8 name), 0x39 (`FUN_00227df0`), 0x3e (`FUN_00226100`) [read by WP-E6d, below]. The `.anb` path
  (`FUN_0022cbb0`, strings 0x37f208 `anm/`, 0x37f210 `/`, 0x37f218 `.anb`): `anm/` + the name's prefix past
  its first four characters + `/` + model + the rest + `.anb`, e.g. `anm/tt/h_ex510/ETT10201E.anb`.
- **Motions, opcode 0x2c** [V, WP-E6]. `FUN_0022cd90`: at frame +4, actor +0xe: the actor's object is made
  here if it has none (`FUN_0016e8c0(DAT_0037f220, objentry | 0x40000000)`), the motion record of (model, name
  +0x12) is found (`FUN_002242a0`) and started through `FUN_0017e1c8(2 * +0x10, 2 * +8, object + 0x140, ..)`
  = `FUN_0017dd38(blend, start frame, ..)` (the same start as every other motion). +6 is read by nothing
  [V, WP-EV1: searched, see Unknowns]; +0xa / +0xc are a loop (below, WP-E6d).
- **Placements, opcode 2** [V, WP-E6]. `FUN_002293b8`: only when +4 is 0, at frame +0x26, actor +0x24:
  position +8..+0x14 (`FUN_0016a700`, the game's space) and rotation +0x14..+0x20 in degrees (`FUN_002292b0`,
  `DAT_0037f0c0` = pi / 180; yaw only when x and z are 0, `FUN_0016a7f0`).
- **Fades, opcode 0x12** [V for the layout]. `FUN_00226830`: at frame +4, +8 picks the fade object
  (`FUN_0014a978`, `FUN_0014a948`, `FUN_0014a918`) and +6 its length; not ported.
- **Motion loops, opcode 0x2c +0xa / +0xc** [V, WP-E6d]. `FUN_0022c9b0` fills the actor's row of the motion
  slots `DAT_0036bbc8` (64 rows of 0x40 bytes; `FUN_0022c928` the actor's row by index +0xe, `FUN_0022c970` a
  free one): +0x10 a counter = 2 * (+8 - +0x10), +0x14 = 2 * +8, +0x18 = +0xa, +0x1a = +0xc, +0x30 = 1.
  `FUN_0022ca18` (every frame, called at `0x22482c` by the setup reader `FUN_002245d0`) steps every row: with
  +0x1a not -1 and 2 * +0x1a < counter, the counter becomes 2 * +0x18 + the overshoot and the motion is started
  again there with no cross-fade (`FUN_0017e1c8(0, frame, ..)`), and unless `FUN_001beb00` the object is put
  back at the place `FUN_0016b960` kept at the row's first step (+0x20); then the counter counts
  `DAT_00356f40` (1.0). In tt02/03/04, 306 of 1159 opcode-0x2c commands loop. Ported (`MotionClock`) except
  the place put back [the app's event actors take no root motion, inferred]. [WP-E6e] The put-back is ported:
  `FUN_0016b960` answers object +0x540 (the position) unless `FUN_001beb00` (object +0x570 set: +0x70), the row
  copies it at its first step (+0x30 = 1, only on looping rows) and `FUN_0016a618` writes it back to +0x540
  (w = `DAT_0037a1b4`) and refreshes the object (`FUN_001921e8`) at every loop. Here `keep_place` / `put_back`
  on the last opcode-2 place (test `a_looping_motion_puts_its_actor_back_where_the_loop_began`); the rotation
  stays.
- **Opcode 0x4d: hide or show objects** [V, WP-EV1: the rule below; WP-E6e read the call]. `FUN_00226e80`: at
  frame +4, +6 = 0 -> `FUN_001b7fa8(2000)`, else `FUN_001b8040(2000)`: every object of the list at
  `DAT_01d5d7a0` (next at +0x8b4) without +0x588 bit 0x20 whose method +0x3c answers below 2000 gets object
  +0x10c bit 0x80 set (resp. cleared). Bit 0x80 is a hide: `FUN_0019af68` (true when +0x10c 0x80 or 0x100, ...)
  makes `FUN_00191c18` clear bit 4 of +0x768 [inferred: the draw bit] and the weapon test of docs/movement.md.
  Method +0x3c of the actor class (vtable 0x35b950) is `FUN_0016bc68`: the +0x28 tag of the object's
  `obj/%s.mdlx` resource (`FUN_0016ed08`, format 0x37a430; `FUN_0016f800` finds it in the list `DAT_0035bd88`;
  `FUN_0016f8e0(2000)` at the event start (state 2) moves tags of 2000 and above to 10000+). **WP-EV1, the rule.**
  Every object class vtable read (0x35b950, 0x35ced8, 0x35eea8, 0x35f580, 0x35c9a8, 0x35c908, 0x35e300) has
  `FUN_0016bc68` at +0x3c, and the compare in `FUN_001b7fa8` / `FUN_001b8040` is signed. The tag is the second
  argument of `FUN_0016e430` (the model load): -1 means -200 for an object the event made (id or'ed with
  0x40000000, `FUN_0016ef80`, objentry type not 7 or 0x16: the model load of load record 0x38 `FUN_00229198`;
  that opcode 0x2c's `FUN_0016e8c0` object uses the same resource is [inferred]), else the word of 0x37a278 by ObjectType (E6f, above). Result: opcode 0x4d (+6 = 0) hides
  every object that exists at that moment and is either made by the event (-200) or of ObjectType 5, 6, 8, 10,
  11, 13, 18 or 23 (tags 1030..1110, the field's props: the tt areas spawn F_EX000, F_TT010 and F_TT010_SORA of
  them, 217 entities); the player, friends, townspeople, enemies, save points and summons (2100..2700) are
  never touched, and the explicit tags other callers pass (0x13ef `FUN_002a18a0`, 0x96a / 0x974 from
  `FUN_001b3f78` and `FUN_00174fb0`, -200 `FUN_002c4c58`) lie at 2000 or above except the -200 ones.
  +6 not 0 clears the bit on the same set. It is a one-time pass over the list, not a state: an object made
  afterwards (the event's actors appear at their first opcode 0x2c) is not hidden by an earlier hide. How it
  ends: only a later opcode 0x4d with +6 not 0, or the object's end. Bit 0x80 is the hide flag (above). On the disc, over all 268 `.ard` files (test `opcode_0x4d_on_the_whole_disc`), there are 29 of them in 21
  areas: bb12 (112, 113), ca01 (102), ca06 (206), ca07 (120), ca24 (214), eh16 (126), he00 (124, 141),
  he03 (102: hide at frame 4, show at 379), nm06 (126: hide at 6181, show at 6843), nm10 (206), po04 (102),
  tr01 (106), tt01 (101), tt06 (305), tt09 (209), tt12 (521: a show only), tt16 (515), tt32 (312), wi01 (102, 103),
  wi02 (121), wi03 (119), wi04 (109, 129), wi07 (106). Most are a hide at frame 0 or 1, before the event's actors
  exist, so they hide the area's props, not Sora. Only he03 102 and nm06 126 hide and later show. Not ported: an
  id with no objentry row (`FUN_0016ef80` needs the row) and the objects with flag +0x588 bit 0x20 that the list
  walk skips; the app's event actors have neither (comment on `resource_tag`). Ported: `event_player::resource_tag`, `op_4d_touches`, `op_4d_hidden` and
  `Spawned::hidden` (an event actor that exists at the pass is hidden or shown; a hidden one stays hidden when
  opcode 2 places it); tests `opcode_0x4d_hides_and_shows_the_objects_whose_tag_is_below_2000`,
  `opcode_0x4d_touches_the_object_types_the_table_puts_below_2000` (reads the table from the executable).
  The app draws none of the prop types (no F_* object is spawned by the app), so nothing else is hidden
  [inferred: those objects are not in the app]. Log: `op 0x4d hides 0 event actor(s)` at tt01 101 frame 0.
- **Opcode 2 +6** [GAP]: `FUN_002293b8` reads +4 (must be 0), +8..+0x20, +0x24, +0x26 only; values seen 0,
  1247, 2258, 30716. No other reader searched beyond the two switches.
- **Opcode 3** [V for the readers]: in neither switch (`FUN_002245d0`, `FUN_00223a88`), so both readers pass it.
  Every tt02/03/04 event has one: `u16 area number, "tt"` (tt02: 2, tt03: 3, tt04: 4) [inferred: the area the
  event belongs to]. Not a show/hide.
- **Opcode 0xd: the drawn mesh groups** [V, WP-E6e]. `FUN_00228b48`: at frame +4, actor +8's object
  (`FUN_002244b0`, row +8), `FUN_0011c550(object +0x670, cmd + 0xa)`: object +0x670 is the model instance
  (`FUN_00168d30`: made by the model resource's method +0x24); `FUN_0011c550` calls method +0x34 of its +0x14
  (the model class) with its +0x104 (one draw-flag byte a mesh) and the u16 list at +0xa. The model classes'
  vtables were found by the address of their +0x24 create method in the executable's data: 0x354b18
  (`FUN_0011f390`, a map-style model; +0x34 `FUN_0011f4a0`), 0x354b98 (`FUN_00120b48`, the skeletal model;
  +0x34 `FUN_00120fd0`, +0x3c `FUN_00121178`, +0x40 `FUN_00121278`), 0x354c00 (`FUN_00123be0`, a multi-model;
  +0x34 `FUN_00123f68` forwards to each sub-model). `FUN_00120b48` sets every mesh's flag to 1; `FUN_00120fd0`
  first clears the low four bits of every flag (`FUN_0011eda0`), then for each list value up to 0xffff ORs 1
  (bit 15 clear) or 2 (bit 15 set) into every mesh whose group, bits 6..10 of its 0x20-byte header's flags word
  (model +0x1c the count, headers from +0x20), is `value & 0x7fff`; the same for the second model at model +0xc
  (`FUN_001205b8`, group = bits 0..4). The draw `FUN_00121a78` skips a mesh whose flag has no low bit set. So
  opcode 0xd says which mesh groups of the actor are drawn (the others are not) [what flag 2 changes in the
  draw: WP-EV1, below]. +0x3c / +0x40 show / hide one group (callers `FUN_00192cd0`, `FUN_00192ce8`, `FUN_001485a8`).
  Ported: `mesh_groups`, `mesh_flags` hide the actor's mesh entities of unlisted groups (test
  `opcode_0xd_draws_only_the_listed_mesh_groups`: all 150 commands in tt02/03/04 reach a readable model; the
  tt lists name groups 0 and 2..0x1f, i.e. they hide group 1 (N_EX500_RTN: mesh 2 of 4) [inferred: an alternate part]); the second model is not drawn by the app.
  **WP-EV1 (vtable +0x34 and flag 2).** The +0x34 call is exactly `FUN_00120fd0` above for the skeletal model
  (vtable 0x354b98); the map-style model's (0x354b18) `FUN_0011f4a0` and the multi-model's (0x354c00) `FUN_00123f68`
  are the same call for those classes, nothing else calls it. Flag value 2 (bit 15 of a list value): the draw
  `FUN_00121a78` tests `flag & 0xf` (twice) and `flag & 4`, `FUN_0011ee30` (called by `FUN_0011c5e0` /
  `FUN_0011c648`) tests `flag & 4`; nothing tests value 2 on its own (`FUN_0011c7d8`, `FUN_0011cba8`,
  `FUN_0011d4f8` only pass the array on), so a mesh with flag 2 draws like one with flag 1 [V for the readers
  found by `mips_scan --load-off 0x104` in 0x11c000..0x124000; a reader outside it is not excluded].
- **Opcode 0xf** [V]. Setup reader, `FUN_00226f50`: waits while `FUN_001962c0` (the event object not yet in
  its play states), sets `DAT_0036bd10` = 1 (the timeline thread `FUN_00223a88` waits for it before anything)
  and makes the event's tables (`FUN_00222cc0`: the camera table `DAT_0036bbb4`, the motion slots, the actor
  table, ...). Every tt02/03/04 event has it. Ported: without it the timeline does not run.
- **Opcode 0x10: the fade-in after the setup, not an end fade** [V, WP-E6e corrects E6d]. Setup reader,
  `FUN_00226fa0`: `DAT_0036bd0c` = +4. The thread `FUN_00223a88`, once the setup is done (`DAT_0036b9e0` 2,
  ready `DAT_0036bcf4`) and the event object has bit 2 (`FUN_00196320`), calls `FUN_00196348(2 *
  DAT_0036bd0c)` BEFORE its first command: event object +0x90 = that, state 5 (`FUN_00195fb8`), then state 6
  (`FUN_00195ff8`): fader 0x3564d0 (`FUN_0014a918`) mode 1 (`FUN_0014a7f0`, the fade IN, as area_jump.rs reads
  it) over `(int)(+0x90 * DAT_0037b054)` frames for kind row byte +1 = 1 or 3 (black, `+0xf4` 0) or 2 (white),
  then state 7 or 8 (play). tt events: 20 frames, 40 ticks (`FUN_0014a710`: twice the frames). Before it, state
  0 (`FUN_00195d28`, at the trigger) fades the same fader OUT (`FUN_0014a7c8`, mode 3) over `(int)(row +4 (a
  float) * DAT_0037b050)` frames for row byte +1 = 1 (black) / 2 (white), or plays the keyhole layout for 3
  (`FUN_0015d058`); states 1..3 (`FUN_00195e68`, `FUN_00195ef8`, `FUN_00195f68`) wait for that fade
  (`FUN_001966f8`: fader `+0xfc` != 4) before they set bit 2, so the timeline starts on a covered screen. Flag
  bit 4 adds two other faders (0x3566f0, 0x356800) at 0 frames (state 6) and 6 frames (state 9, fade-in): not
  ported. The event's END (`FUN_002233d8` -> `FUN_001963e0`, state 9 `FUN_00196170`) fades only those and the
  faders 0x356910 / 0x356a20 (`DAT_0036bef0`); the screen fader goes black at the end only through the trigger
  jump (area_jump.rs). Ported in `event_player.rs` (`kind_rows`, `start_fade`, `setup_fade`, `ScreenFade`,
  `EventPlayer::screen_fade`; test `the_event_object_fades_out_then_in_by_its_kind_row`): the timeline is held
  while the start fade runs; drawn only once `area_jump::draw_fader` reads `screen_fade` (outside this WP's
  files). The keyhole layout of kind 3 is not started from here [GAP].
- **Load sub-records 0x26, 0x39, 0x3e** [V, WP-E6d]. 0x26 (`FUN_0022c070(rec + 4, block parity)`): a voice,
  `FUN_0022bff8` = 0x37f178 `voice/%s/event/` with `DAT_00349d40` + name + 0x37f188 `.vag`, loaded into a file
  slot of kind 3 (`FUN_00224200`, `DAT_0036bbc0`, 128 rows of 0x54) through `FUN_00224030` / `FUN_00167d20`
  (callback 0x222a28). 0x39 (`FUN_00227df0(rec + 6, rec +4)`): an effect, a free slot (`FUN_002241c8`) of kind
  2 with +4 at slot +0xc, `FUN_00227d80` = 0x37f090 `event/effect/` + name + 0x37f0a0 `.pax`. 0x3e
  (`FUN_00226100(rec + 4)`): `FUN_002260b0` = 0x37f038 `event/layout/%s/%s.2ld` with `DAT_00349d40` and the
  name, handle at `DAT_01dc0078`, `FUN_0016fac8(.., 0x13ec, 0)`. `DAT_00349d40` is the region (`us`), not the
  world [inferred: the USA disc has `voice/us/event/tt10203or.vag`, `event/layout/us/tt_telop.2ld`,
  `movie/us/opn.pss` and no `tt` folder there]. Test `twilight_town_event_load_records_name_files_on_the_disc`:
  35 events, 290 voices, 8 effects, 1 layout, every file on the disc. Ported: parsed, paths logged at the
  start; the voices are not played (which command starts one was not read), the effects and layout not drawn.
- **Movies, opcode 0x44** [V]. `FUN_00225860`: at frame +4, plays `movie/<region>/<+6>.pss` (format 0x37f010
  with `DAT_00349d40`, `us`; WP-E6d) and waits for it; the tt01 events `100`, `200`, `400`, `600` are the movies `opn`,
  `me2`, `me4`, `me6` with their subtitle lines (test `the_tt01_short_events_are_movies_with_subtitle_lines`).

## 7. Area music

- [V] `FUN_00137150` (area load) takes the pair (field, battle) from `DAT_01c95638` (`FUN_001a7720`) and starts or
  keeps the BGM (`FUN_001371d8` equal numbers, `FUN_00137300` different: the battle switch of docs/audio.md). The
  only writer found (`mips_scan --addr 0x1c95638 --jal 0x1a7700`) is area-script opcode 0x10 (`FUN_001a7700`); the
  area-script reset does not clear it [I: a program without opcode 0x10 keeps the previous music].
- [V, `twilight_town_story_flags_and_music`] In Twilight Town the music is set by `evt` programs: e.g. tt04 0x4c
  (119, 119), 0x50 (121, 121), 0xb6..0xb8 (53, 53), program 1 (0, 0: no music). The `arif` pairs the app uses
  (`audio::area_music`, set 0) are (118, 119) for every tt area, set 1 (52, 53), set 2 (0, 119). [GAP: where the
  game reads `arif` +0x10 for music; not found in this work package. The app's set-0 pair equals what a "free
  roam" program would need only by assumption.]
- Events change music through their own commands [GAP: which opcode].
- [V] Per slot (field 0x354e98, battle 0x354ea8) `FUN_001371d8` / `FUN_00137300` leave a slot whose
  stored number equals the new one alone and take a changed one; number 0 silences the slot
  (`FUN_00136cf8(slot, 0)`). The app (WP-E1c, `audio::area_bgm` with `audio_bgm::area_pair` and
  `AreaMusic::load_pair`): once `area::run_area_script` has run, the pair of opcode 0x10 is applied;
  a run without opcode 0x10 keeps the pair already playing [inferred, above]; before any pair was
  set (the app starts in an area instead of booting through the story) the `arif` pair stands in,
  logged `arif stand-in`. The field music restarts only when its number changes; 0 stops both
  slots. Log: `<area>: music field F battle B (area script opcode 0x10)`. Test:
  `the_area_pair_comes_from_opcode_0x10_and_restarts_only_a_changed_field_number`.

## 7b. Camera and party at the entrance (WP-E1c)

- [V] `FUN_001047a8` (area start) calls `FUN_00149bb8` -> `FUN_00140c00` -> `FUN_0013f470(0x355840)`:
  the field camera is placed straight behind the player with no smoothing, before the fade-in
  `FUN_0013efe8`. The app's `Rig::new` is that placement; `camera.rs` now rebuilds the rig once
  `AreaRecord.placed` names the area Sora was just put in (a jump or `KH2_ENTRANCE`), logged
  `camera: placed behind Sora at the entrance of ttNN (FUN_0013f470)` [inferred: the player is
  created before `FUN_001047a8`, so the game places the camera behind him at his entrance].
- [V] The friends (`FRIEND_1` / `FRIEND_2`) are spawn-kind-1 entities like the player and pass
  the same filter `FUN_001981a0`. With a real entrance (a jump or `KH2_ENTRANCE`) `party.rs`
  takes their rows only from the groups the area scripts loaded and only those the filter creates
  for the entrance (`party: spawn ... entrance N`); without one the old rule stays (the first row
  of each friend in the `map` script).

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
