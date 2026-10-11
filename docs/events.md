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
  reference video 8.mp4 shows this as the keyhole wipe, then the area card. The layout is not
  decoded [GAP: the table's loader and the `.2ld` file were not searched]; the app draws the same
  black fade as a labelled stand-in (`jump: fade kind 0 ... black stand-in`).
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
- Lock: `FUN_00195358(world, area)` reads the halfword at `0x33f918 + FUN_00195208() * 4` (0 =
  open) and then `FUN_001d32c0(world, area)`; neither was read further [GAP]. The app locks nothing.
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
  hold them with type-15 entities (e.g. tt02 `a72_`, ids 0xf3..0x2ff). Which entity follows which path (the id)
  was not traced [GAP: searched the group runtime `FUN_00198430..FUN_0019a000`; the reader is behind the
  class-5 path followers].
- **Talk** [V for the offer, GAP for the window]. A type-5 reaction part with id 0x83 offers the command in the
  halfword +0x2c of the object's spawn record (docs/limits-reactions.md: 31 "talk" for most townspeople). Command
  31 has `Execute` 31 (`kh2 table cmd`). Executing a reaction command sends event 0xb (command, args) to the
  target's script (`FUN_00177c20` -> `FUN_0016b420`), after counting it (`FUN_00187f88`). The tt townsperson
  script has **no event 0xb handler**, so the message must be opened elsewhere [GAP: searched every caller of
  the four window constructors (`mips_scan --jal 0x1ab6a8 0x1ab8e0 0x1ab7d8 0x1ab9e0 0x1aae60 0x1aae08`): host 1.32
  (`FUN_00172360`, script message window class 0x1ab6a8, returns its handle; 1.33 closes it through the window's
  method +0x20), the event commands 0x2d and 0x15 (section 6), `FUN_001a5608`, `FUN_001ab708`, `FUN_001abad0`
  (window children), `FUN_002c5cf0` / `FUN_002c5db0` (called from `FUN_002c4c58`, minigame code); no talk path
  found. Next step: trace what the reaction executor does for `Execute` 31 (its per-command switch was not
  located; `FUN_001cf030` is not in the decompilation) and look for a type-0x16 event started by the talk].

## 4. Save points and world points

- **Objects** [V]. `SAVE_POINT` (objentry 570) and `WORLD_POINT` (569) have type 7 -> `FUN_00194ab8` (0xa00 bytes,
  vtable `0x35e300`): sets object flag +0x588 |= 0x1000, starts its own effect element 1 (`FUN_0016a570(obj, 1,
  ..)`, handle +0x9f4, ended by `FUN_00194b68`). For object 569 (the world point) the pair (world, area) is looked
  up in `00worldpoint.bin` (`FUN_00194ee0`, 4-byte records world, area, entrance, -; at most 0x36) and that
  point's bit is set at `0x33f910` (`FUN_00194fa8`; `FUN_00195010` answers 0 unseen, 1 seen, 2 seen and locked).
  In tt02 they stand in `m_20` (world point) and `m_21` (save point).
- **On approach** [V]. The player's reaction manager `FUN_0019c0f0`: while the offered command (+0x26) is 55 and
  the offer status is below 4, `FUN_00194c08(the point)` restores the four party slots' HP and MP to their maximum
  (`FUN_001a0140`, `FUN_001a0210`; a knocked-out member is revived with `FUN_001a0240` + `FUN_001a0100`), and when
  something was restored (or on the first frame) sets +0x9fc; the point's update `FUN_00194b98` then starts effect
  element 2 (handle +0x9f8) once [V; I: the heal sparkle]. So healing happens while the save prompt is shown, not
  on use.
- **On use** [V for the delivery, GAP for the menu]: command 55 (`Execute` 55, message 34054) goes to the point as
  event 0xb like every reaction command. The save menu opener was not found [GAP: searched `FUN_0024f0b8` callers
  above; `00common.bdx` is run by `FUN_00194d78` (event 7, at boot) and `FUN_00194e68` (event 0xc with an
  argument)]. The world point leads to the world map (jump type 4 / `FUN_00149df0`: world 0xf) [I].
- **What the app needs**: spawn the two objects from the `map` program's groups with their own effect files; the
  offer of command 55 (the part in the object's collision records, docs/limits-reactions.md); the heal rule above
  in the reaction tick; a stub save menu.

## 5. Story progress

- **Where** [V]. Flags are bits at `0x33f6b0` (save block): flag `id` is bit `id & 0x1f` of word `(id & 0x3ff) >> 5`
  of the 0x20-byte page `id >> 10` (`FUN_001802e0`; 0xffff is always clear). So each world owns `0x400 * world`
  upwards and uses 256 flags (`flag_count` finds 256 in the tt page). Test: `FUN_001802e0` (host 0.15), clear:
  `FUN_00180298`, set: `FUN_0017ff60` (host 0.14, opcode 0x19, trigger sub 2, `FUN_00180350`).
- **Side effects** [V]. Setting a flag runs its list from `00progress.bin` (BAR; `FUN_0017fd10` stores the 19
  type-2 world entries at `0x35ccc0` and hands the named entries `dsa`, `wldf`, `epsd`, `wlds`, `link`, `wmky` and the
  type-0x2c entry to other systems): commands `u8 op, u8 n, n halfwords`, op 0 ends (`kh2_ard::progress`). Ops: 1
  area programs (records `area, map, btl, evt` written to `0x33e86f + world * 0xc0 + area * 3`, 0xff keeps), 2..5
  `FUN_00195260` / `FUN_00195288` / `FUN_001952a0` / `FUN_00195300` [GAP], 6 `FUN_001a7730(world, x)`, 7 clear flags, 8
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
  docs/hud.md). Opcode 0x15 (`FUN_00227478`): +4 frame, +6 key, +8 message id, +0xa 0 open / else close, the
  subtitle line class 0x1aae60. Up to 32 open windows at `0x1dc00e0` keyed by (key, message id). In tt02 event 102
  the 0x15 lines open and close ids 0x3376..0x337b.
- **Start and end** [V]. A trigger with an event tag calls `FUN_00196428`: a 0xb0-byte event object
  (`FUN_00195898`: kind row from the argument, `FUN_001d1128`; bit 7 of the argument -> `FUN_00138708(1)`), the
  jump record copied into it, `FUN_00195ae0` starts the player (`FUN_002232c8`), stores the object at
  `DAT_0035e3a4` (host 1.34 `event_active` reads it), hides HUD parts (`FUN_001bef48(0xc)`, `FUN_0014c748(4)`).
  `FUN_00195c08` steps the object's own states (0..9). When the event ends (`FUN_001964d0`) the chain event 0x4c
  fires for kinds with bit 0; the event's end (`FUN_00225028`) fires 0x12 or 99. Other starters: `FUN_001ad1c0`,
  `FUN_001ad378` (revive), `FUN_001b3468`.
- **Skipping** [GAP]: `FUN_00224b18` (an event is running and `FUN_002c17e0`) and `FUN_00222630` drop commands;
  the pad check and what a skip jumps to were not read.

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

## 8. Where it goes in the app

| file | function / item | change |
|---|---|---|
| `kh2-ard` (done) | `Area`, `Group`, `parse_trigger`, `parse_event`, `progress` | readers used below |
| new `area.rs` (kh2-app) | `AreaRecord` resource (world, area, entrance, programs) and `AreaPrograms` (the 0xc0-byte table per world from the save block) | E1, E5 |
| new `area_script.rs` | port of `FUN_00181148` / `FUN_00180b48`: run `map`, `btl`, `evt` programs, spawn groups, record triggers, music, map visibility | E1 |
| `assets.rs` | `load_area`: take the player placement from spawn kind 1 entities of the entrance; load by the area record | E1 |
| `enemy.rs` | `area_groups` / waves: read the btl program from the area record instead of taking turns | E5 |
| new `area_jump.rs` | exit volumes (`Activator` shape tests), the jump task with fade kinds, triggers and `FUN_00197b70` events | E1 |
| new `npc.rs` | class-5 objects with their BDX and the group 5 hosts (paths) | E2 |
| `enemy_host.rs` | hosts 0.14 / 0.15 on `Progress` story flags; 1.30 / 1.31 jumps; 1.32 / 1.33 message windows; 1.202 / 1.203 | E2, E3, E5 |
| `progress.rs` | story flag bits (0x33f6b0) and the area program table (0x33e86f), flag side effects from `00progress.bin` | E5 |
| `hud_msgwin.rs` | open/close from hosts and event commands (closing states 4..7 are a known gap there) | E3, E6 |
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
  played (E6)`; type-1 trigger jumps are taken, other types logged. Not done: the music pair of
  opcode 0x10 is logged, not applied (`audio_bgm` keeps the `arif` pair); the keyhole layout; the
  exit lock; the battle-state check; the name window of the prompt; the friends' entrance filter
  and the camera snap after a teleport (both listed under "Later" in the E1b brief).
- **E2 NPC spawn and idle/walk.** Class-5 objects of `map` program groups with their BDX (`rtn_default`), group 5
  hosts for the walk path (+0x508), walk path ids. Accept: tt02 log of spawned `N_EX5xx_RTN` with no `missing`
  host calls in `KH2_BDXLOG`; screenshot with townspeople looked at.
- **E3 Talk and message windows in play.** First close the talk GAP (section 3), then offer 31 from the 0x83
  part, deliver event 0xb, open the window class the code opens; hosts 1.32 / 1.33. Accept: `KH2_REACTIONLOG`
  shows 31 offered and executed; a screenshot of the window looked at.
- **E4 Save points.** Type-7 objects, effect elements 1/2, heal while command 55 is offered, the world point bit.
  Accept: log `save point heal` when HP was below max; screenshot of the point.
- **E5 Story flags selecting battle programs.** Story flags in `Progress` (bits, side effects op 1, 7, 0xc),
  hosts 0.14/0.15 real, the btl program from the area record, triggers with events 2/3 and the battle manager
  results (jump records type 1/2). Accept: with a state file setting flag 0x883, tt04 logs `btl program 1`; the
  Dusk drop rows of docs/enemies.md now follow real flags.
- **E6 The event player for one short event.** Parse, setup reader, frame counter, actors (opcode 1, 2), motions
  (0x24 + `.anb`), message lines (0x15, 0x2d), the camera cut table (opcode 6) into `CamEvent`; skip later. Start
  with a tt01 event of under 600 bytes (`100`, `200`, `400`, `600`). Accept: log of commands at their frames; a
  screenshot with a subtitle line looked at.

## 10. Unknowns (with what was searched)

- The talk handler for command 31 (section 3; constructor callers listed there).
- The save menu opener for command 55; `FUN_0024f300` meaning (trigger sub 7).
- Opcodes 0xa, 0xb, 0xe, 0x11, 0x16 meanings (readers named in the table); trigger subs 3, 6, 8; flag ops 2..5.
- Group kinds 7 and 9 (`FUN_001ccb10`, `FUN_0019a000` not read); activator +0x28..+0x3f beyond +0x2c/+0x2e/+0x2f.
- The event kind table `DAT_01d6a92c` (rows read by `FUN_001d1128`): its loader was not searched.
- Event camera key format (opcode 0x13), actor motion playback, music commands of events, skip.
- Where `arif` music pairs are read by the game (only opcode 0x10 writes the pair the area load reads).
- `FUN_00195358(world, area)` (exit lock) and `FUN_001d88a0` (walk-in test with header +0x1e): not read.
- The meaning of the area special values 100 and 101 in exits beyond the code's mapping.

## Decompile index

| address | what |
|---|---|
| 0x1048e0, 0x1689c0, 0x180870, 0x181148, 0x180a68, 0x180ad8, 0x180b48 | area thread, load, script run, interpreter |
| 0x180ff8, 0x1975e0, 0x197588, 0x197670, 0x1978b0, 0x1979f8, 0x197a60, 0x197b70, 0x197780, 0x1976d8 | triggers and event firing |
| 0x181728, 0x1817f0, 0x198430, 0x198750, 0x198920, 0x1981a0, 0x198220, 0x1982c8, 0x1989d8, 0x198b30 | spawn groups |
| 0x198e10, 0x198fa0, 0x199058, 0x1991a0, 0x199518, 0x199930, 0x199b18, 0x199e68, 0x199e98, 0x199f70, 0x199c48, 0x199c70 | group kinds |
| 0x1d8b48, 0x1d8600, 0x1d89a0, 0x1d8a70, 0x334388 | activator volumes |
| 0x1d88a0, 0x1d8908, 0x1d8958, 0x19a3f8, 0x19a440, 0x1b80e8, 0x1b7e00, 0x19b110, 0x17be98, 0x16a890, 0x195358 | exit gate and lock (E1b) |
| 0x13ef68, 0x13efe8, 0x13f148, 0x13f1a0, 0x14a7c8, 0x14a7f0, 0x14a710, 0x15d058, 0x15d1a8, 0x104d58 | fade kinds and the jump task (E1b) |
| 0x104e18, 0x104d58, 0x104c68, 0x104ce0, 0x104bd0, 0x104b90, 0x105188, 0x105010, 0x19d8f0, 0x19d9d8, 0x13ef68, 0x13efe8, 0x198080, 0x198058, 0x172118, 0x172170, 0x149df0, 0x1abfe8 | jumps |
| 0x1802e0, 0x180298, 0x17ff60, 0x17fd10, 0x180350 | story flags |
| 0x16e8c0, 0x189858, 0x1a51e8, 0x2ea050..0x2ea978 | objects by type, NPC class, group 5 hosts |
| 0x177c20, 0x16b420, 0x16b470, 0x172360, 0x1723f0, 0x2c5cf0 | commands to targets, script message window |
| 0x194ab8, 0x194b68, 0x194b98 (decompiled for this WP), 0x194c08, 0x194d78, 0x194e68, 0x194ee0, 0x194f68, 0x194fa8, 0x195010, 0x19c0f0 | save and world points |
| 0x196428, 0x195898, 0x195ae0, 0x195c08, 0x1964d0, 0x2232c8, 0x2233d8, 0x223910, 0x223a88, 0x2245d0, 0x224a90, 0x224b18, 0x225028, 0x2275a8, 0x227478, 0x22b0f8, 0x22cbb0 | event player |
| 0x137150, 0x1371d8, 0x1a7700, 0x1a7720 | music |
| 0x194188, 0x1be828, 0x1d7970, 0x1a6ff0, 0x195778, 0x1d3a68, 0x1d30d8 | other opcode targets and area start |
