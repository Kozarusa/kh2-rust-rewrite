# Areas and events (WP-7G): implementation spec

[Contract](contract.md) | [Index](../../README.md)

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

Earlier notes used: `docs/systems/script-host/contract.md` (the spawn groups of tt04), `docs/systems/player-camera/camera-event.md` ("Event camera", "Area
camera types"), `docs/systems/ui/hud-banners-windows.md` ("Message windows (WP-6D)", area card), `docs/systems/audio/audio.md` ("Battle music"),
`docs/systems/drive-limits-summons/contract.md` (reaction commands, the 0x83 spawn part), `docs/systems/script-host/contract.md` (hosts 0.14, 0.15, 1.202,
1.203), `docs/systems/core-runtime/progress.md` (the save block).

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
| 0xf | `world_record` | `FUN_001a7330(current world record, argument)` (the world records of docs/systems/core-runtime/progress.md) | V |
| 0x10 | `music` | the music pair word `DAT_01c95638` (u16 field music, u16 battle music); `FUN_00137150` reads it at area load | V |
| 0x11 | `word_35cd34` | `DAT_0035cd34` (default -1), read by `FUN_00179140` (a test of the object's +0x5f4) | V; meaning GAP |
| 0x12 | `heap_base` | `DAT_0035cd24` (default 0x100000); `FUN_001048e0` calls `FUN_00105250(cd20 + cd24, cd24)` | V |
| 0x13 | `camera_type` | `DAT_0035cd38`, the area camera type of docs/systems/player-camera/contract.md (default 0) | V |
| 0x14, 0x17 | `flag_35d268_4`, `_10` | set bit 2 / bit 4 of the global flag word `DAT_0035d268` | V |
| 0x15 | `battle_manager` | `FUN_001b9338(argument 0, rest)`: creates the battle/minigame manager at `DAT_01d5dd20` (docs/systems/script-host/contract.md, Struggle) | V |
| 0x16 | `load_file` | formats the arguments as a string with the language code (`DAT_00349d40`) into `0x1c893d8` and loads it (`FUN_0016fac8(.., 0x38e, 0)`) | V; files: `file/%s/*.2ld`, `menu/%s/jiminy.bar`, `minigame/*.bar` (section 1a) |
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

## 1a. Area-script port (WP-L1-P5)

`crates/kh2-ard/src/script.rs` ports `FUN_00180b48` (one command), `FUN_00180ad8` (a run of commands) and the area-start defaults of `FUN_00181148`
(`ScriptVars::finish`) on the original's layout: the command is a slice of `u32` words (opcode low half, signed count high half, arguments), read
unbounded like the game reads memory. Everything a command asks of the rest of the game is a `ScriptHost` call. Oracle: `crates/kh2-ard/tests/orig_180b48.rs`
runs 4,000 random commands (every opcode at least 90 times, nested 0x1c blocks, counts from -3 to 8) in kh2-ee with the callees replaced by recording
hooks and compares the answer, the variables (`0x35cd10..0x35cd44`, `0x35d268`, `0x1c893b8..`, `0x3716a0`) and the calls, byte for byte [V].

Findings the oracle forced [V]:
- The random word `DAT_003716a0` is zero-extended before `FUN_00325b38` (a 64-bit unsigned divide): opcode 2 picks `lcg / ceil(2^32 / count)`, opcode 3 rolls
  `lcg / 0x28f5c29` (0 to 99), opcode 0x1f and the lottery use the same form. A sign-extended reading disagrees.
- Opcodes 0x19, 0x1a and 0x1b take argument 0 when the count is positive and then argument `k + 1` only while `k + 1` is below the signed high half of
  argument `k` (the loop bound is re-read from the moving pointer): with story flags and bit numbers below 0x10000 only the first argument is used. The disc has no
  command of these opcodes with more than one argument (area survey), so the earlier "every argument" reading and this one agree on all data.
- The 1 << n shifts of 0x1a/0x1b use the low six bits of n (64-bit shift), not "0 for n above 63".
- Opcode 0x1c: the nested commands may read past the block's end (the game does not stop them); the nested run ends when the block's words are used up.
- Opcode 0xa asks the file for the entry (`ScriptHost::entry_by_index`); 0x16 `load_file` formats the command's argument words as a C format string with the
  language code (`kh2_ard::script::file_name`; the disc uses `file/%s/<name>.2ld`, `menu/%s/jiminy.bar`, `minigame/<name>.bar`) and requests the file
  with buffer id 0x38e (`FUN_0016fac8`).
- Opcode 0x1f's lottery (`FUN_001d7970` with `FUN_001d7928`, `FUN_001d7a98`, `FUN_001d7908` -> `FUN_001d7848`): `script::Lottery`, oracle `orig_1d7970_matches_the_game`;
  the five `(world, area)` pairs at `0x37c5f8` are read from the executable at run time (the port carries no table).
- Area survey (`area_survey_runs_every_script_of_every_area` in `crates/kh2-ard/tests/disc.rs`): 269 `ard/*.ard` files on the USA disc, 268 are area BARs
  (`ard/hb10.ard` is not a BAR), 3,788 programs, 14,112 commands, 29 distinct opcodes, none above 0x1f; 8, 0xd and 0x20 and up do nothing in the game and
  appear nowhere. Every program ran for five entrances.

**Group kinds 7 and 9** (`crates/kh2-ard/src/kinds.rs`). Kind 7 (`FUN_00198b30` case 7, answer 0): `FUN_00199e68` calls `FUN_001ccb10(player, header word +0x1c,
bit 0 of *(*(player + 0x6c) + 0x28))`, which fills the request object `DAT_003601d0` points at (+4 value; flag set: bit 0 of +8 and the player at +0x34, else bit 1 and
the player at +0x38; nothing when the pointer is null) [V, oracle `orig_1ccb10_matches_the_game`]. Nobody in this repository reads that object yet [gap: its consumer].
Kind 9 (case 9, every frame, `FUN_0019a000`): per entity (0x40-byte record from header +0x2c; serial halfword +0x1e, position +4; radius = float at header +0x1c)
a missing, uncached unit appears (`FUN_001982c8`) when the larger of |dx|, |dz| to the player is below the radius and the direction to it has a dot product above
`DAT_0037b314` with the player's facing; a live unit stays when within the radius and either within `DAT_0037b310` or in front (dot at least `DAT_0037b314`) and is
dismissed (method +0x20) otherwise [V by reading; no oracle: the callees are object-list and VU0 code; the dot product summing all three lanes is [inferred]].
The two constants are read from the executable at run time (`kinds::StreamLimits`). Searched for the writers of the request object's +4 and the readers of
`DAT_003601d0` beyond `FUN_001ccb10`: not done [gap].

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
  0x140f / 0x1822 / 0x200b set -> world 4, area 0, entrance 0, programs kept; else 0x8cd clear and one of 0x3834 / 0x1c1c set ->
  `FUN_00198080(0x40, 0x12)` (world 0x12; a set 0x8cd goes to the world map, section 2a); in both the record's world byte is remembered in `DAT_01c8bd90`
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
  (sub 4, default -1, `FUN_0019d8d0`) a party arrangement index (`FUN_001a7330(FUN_001a71c0(world), index)`, ported in WP-PT1: `party::jump_party`, docs/systems/party/party-built.md section 21); word 5 (sub 5, default 0) the party reset code (the 5th argument of `FUN_00104e18`; when the record has no destination world and the word is non-zero `FUN_001abfe8(word 5)` is called (ported); with a destination the reset runs in `party::jump_reset`, WP-PT1);
  word 6 (sub 7, default 0) "menu before": 1 or 2 opens menu kind 5 first (`FUN_0024f300`) and sets the jump's wait (the 3rd argument of
  `FUN_00104e18`, read as ticks [inferred]) to 2, logged only (the menu is missing); any other code leaves both at 0
  (`LAB_0019db28`); sub 8 is not read by `FUN_0019d9d8`. Tests:
  **WP-JT4** (types 5 and 6, "menu before"): type 5 opens menu kind 0xe, the battle report (`menu_ending.rs`, docs/systems/ui/menus-battle-report.md section 25:
  `ending.2ld`, title 0xcc24, eight statistic rows; `area_jump::JumpMenus.ending`), and no jump follows. The record's "menu before" code
  (word 6) is ported only as its wait: the jump task's `JumpTask::Wait` holds `JumpExtras::delay_ticks` (2) ticks before the fade-out
  (also after a trigger's event, where the fade-out is skipped); the camp it opens first (`FUN_0024f300`) is always page 0x15, the party
  switch notice, ported in WP-PT1 (docs/systems/ui/menus-battle-report.md section 25, docs/systems/party/party-built.md section 21). Type 6 (`FUN_00149eb0`): `JumpTask::Black` fades to black over 30 ticks
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
  files, kind 0 keeps the 30-tick black fade (logged). **Layering (WP-KH1)**: the wipe covers the HUD. Evidence: reference video 8.mp4 (0.4 s, 0.7 s, 1.9 s frames) shows the command deck and the HP ring gone outside the hole. In the original the fader object (`0x3564d0`, `FUN_0014a818` update, task `FUN_0014aa90` priority 330000) is a later task than the screen copy (75000, `FUN_00149f70`), the letterbox fader tasks (80000, `FUN_0014ab80`) and the keyhole task `FUN_0015d0e8` (120000, `FUN_0015d1a8`), so the fader draws last; the keyhole's sprite object (`0x359040`, `FUN_002332e0`) carries its own depth from the layout (`param+0x18 >> 8`). The HUD's own task priorities were not traced [inferred: the video decides the order]. Before: the port drew the keyhole as the last HUD-layer node but the ring panels' camera (order 12) and the menu camera (13) drew over it (HP ring, party portraits, command deck visible over the hole). Now the node has `top` set and draws on `hud::WIPE_LAYER` with its own camera (order 14); `hud_gfx::node_layer` is the rule. On a window wider or taller than 4:3, four black `WipeBar` sprites on that layer fill beyond the layout's 512 x 448 screen while the wipe node is shown [inferred: the original's layout covers its whole frame; the port's widescreen extension]. The UI fader stays above (GlobalZIndex 1000). The bars reach 2 units onto the screen (WP-KH1b): at 2560x1080 two one-pixel ticks of the world leaked at the top and bottom edges at the layout's seams.
  **The original's task order (WP-KH1b, read)**: the task lists are sorted by ascending priority and run in that order, lower first (`FUN_00102518` inserts after the last entry whose priority is not above the new one; `FUN_001028a0` walks from the head, `+0x38` next; the main loop `FUN_00101d68` runs the master list `DAT_0033e69c` through `FUN_001029d8` every frame). So the fader (master list, 330000) runs after the keyhole task (120000, list `DAT_0033e6e4`), the kind 1 fade task (`FUN_0013ef00`, 300000, same list), the letterbox faders (80000) and the screen copy (75000): the fader draws over the keyhole and the keyhole over what ran before it, which is the port's order. The field HUD does not use these priorities: its objects are queued on the HUD's own list (`DAT_003550e0`, `FUN_001394e8(obj, priority)`) with priorities 6000 (`FUN_00138568`, the `gaug` layout), 8000 (the INFORMATION bar, `FUN_00139430`) and 11100 (the HUD's main object, `FUN_00137da8`), all far below 120000. If that list runs inside the same priority scale the HUD draws before the keyhole, under it, as the port now does. Not found: the task that runs the HUD list (its runner is reached through vtables; `FUN_001029d8` has callers only in the main loop and `FUN_001b7390`), and the link from list `DAT_0033e6e4` to the master list (the object at `0x33e6e0` carries 130000 at `0x33e6f8` [inferred: its priority in the master list]; `DAT_0033e6e4` is 0 at the title screen under PCSX2). The cutscene video 8.mp4 shows the HUD hidden anyway, so it is weak evidence; the order above rests on the code and is [inferred] where stated. No contradiction with the port was found, so the order is unchanged.
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
entities [I: two entrances, one of them 0x32 = 50, the entrance tt02's triggers jump to]). The object faces `DAT_0037b270 - rotation y` (`FUN_00198220`; docs/systems/script-host/contract.md uses
the same rule: app yaw `PI - r`).

**What is kept** [V for the calls; I for the rest]: the save block (party, HP, items) is not touched by the area
change; the summon is cleared and reloaded (`FUN_00168ba8`, docs/systems/drive-limits-summons/contract.md); the drive form may end
(party reset kinds); the music is restarted only if the pair changes (`FUN_001371d8`). The world's message file
is bound when an area loads (`FUN_00168ba8` -> `FUN_00190e70`, docs/systems/ui/contract.md).

**Twilight Town links** [V, `twilight_town_groups_exits_and_events` prints them per area as `exits:`]. Each exit
group has a walk-in volume (option 0) and a prompt volume (option 1 or 2). Examples: tt02 (the Usual Spot by the
community's names) -> tt03 entrance 0; tt03 -> tt02, tt04, tt06, tt37 (entrance 1), tt05, tt04; tt04 (the
Sandlot) -> tt03, tt06, tt07 (entrance 1 each); tt06 -> tt03, tt04, tt07, tt08, tt05, tt04; tt08 -> tt06, tt09,
tt37; tt40 -> tt19 and area 100. The full list is in the test output.

## 2a. Area control port (WP-L1-P6)

`crates/kh2-game-core/src/area.rs`, module `ctl`, ports the area change on the original's byte layouts, and `crates/kh2-game-core/tests/orig_area.rs`
runs the original (kh2-ee, 123 tests, 38,000 cases over seven scenarios) against it. The state is the row of globals from 0x33e828: the
active byte, the current place (0x33e830), the destination of the jump in progress (0x33e838), the last exit (0x33e840), the previous place
(0x33e848), the allocator (0x33e850) and the jump task (0x33e854), plus the area thread's gate byte (0x3b2180) and saved destination
(0x3b2188); the place bytes in the save block (+0xc, +0xd, +0xe) and the per-area program table (+0xf) are written through a slice. A place is six
bytes (world, area, entrance, map, btl and evt program). Everything asked of other units (fades, task creation, party reset, story flags,
the loaded files' memory, virtual calls) goes through the `AreaHost` trait; the oracle lays the same memory on both sides, makes every callee of
another unit a stand-in that logs its address and arguments (low 32 bits; float arguments of `FUN_001abf40` are the registers f12 to f14) and
compares the log, every byte of the windows and the arenas after each case.

Ported and verified [V, oracle]: the merge of program bytes (0x104b90) and the destination apply (0x104bd0, 0x105188); the reset of all five
records (0x1052e0, 0x105420: it fires only for the arguments (1, 0xffff)); the jump start (0x104e18: with the gate set it only records the fade kind and the
saved destination), the jump task (0x104d58), the area-thread start (0x104c68), the area start and end (0x1047a8, 0x104b60, 0x104ce0) with their
call runs (0x1688a0, 0x168928, 0x168960, 0x168ae0, 0x168b10, 0x168ba8, 0x168ca0, 0x168ce8, 0x149b08, 0x149b40, 0x149ba0, 0x149bb8, 0x149cb8, 0x149ce0,
0x149d40, 0x149d78); the area allocator (0x105250 create, 0x1052a0 destroy, 0x104fa8 and 0x104fe0 the virtual calls, 0x105f18..0x105fd8 the object);
the place names (0x105010, 0x105098, 0x105d60, 0x149db8); the exit walk-in (0x198fa0), the world-map jump (0x149df0), the title task (0x149f38,
0x149eb0), the local-set lookup (0x198080), the record parser and player (0x19d8d0, 0x19d8f0, 0x19d9c8, 0x19d9d8); the group records (0x198300,
0x198328, 0x198340 and its twin 0x198430, 0x1985f8 and its twin 0x198660, 0x199698, 0x19a308, 0x19a3b8, 0x19a3d8, 0x1982c8) and the group handlers
(0x1981a0, 0x1986c8, 0x198920, 0x198b30, 0x198e10, 0x1989d8, 0x199378, 0x199398, 0x1993f8, 0x199580, 0x1995c8, 0x1996f0, 0x199930, 0x199bf8,
0x199c48, 0x199c70, 0x199ca0, 0x199cc0, 0x199d18, 0x199d68, 0x199e10, 0x199e98, 0x199f70); the area-info table readers (0x1933a0..0x1935e8,
0x193380, 0x193ef8); the camp-poll leaves (0x149fa8..0x14a060, 0x14a1e0, 0x149f70).

What the oracle showed [V]:
- Type 4, second case: the old reading "one of 0x8cd, 0x3834, 0x1c1c set" was wrong. Flag 0x8cd set sends the record to the world map; the story case
  needs 0x8cd clear and 0x3834 or 0x1c1c set (`FUN_0019d9d8` tests 0x8cd first and goes to the world-map label when it is set).
- The end of `FUN_0019d9d8` has a story override that the app lacked: with flag 0x872 clear and 0x86e, 0x86f, 0x870, 0x871 all set, every jump that has a
  destination (types 1 to 3 and the first two cases of type 4) goes to `FUN_00198080(0x7a, 2)` (world 2, the first area the table lists for key 0x7a,
  entrance 0, programs 0x7a) instead.
- The "menu before" code: 1 opens the camp page 0 and 2 page 2 (after `FUN_001d73d0`), and every non-zero code sets the jump's wait to 2 ticks, also codes
  other than 1 and 2, which open no menu.
- The jump task's party reset argument is the reset word itself (the register is still loaded from the task block), and `FUN_001abfe8` runs before
  the area is unloaded; `FUN_00102498` (the task end) clears the task's function word.
- The area group's event argument is passed as an unsigned halfword (`FUN_001975e0`, `FUN_00197838`, `FUN_00197870` get 0x0000fffd for -3).

The app (`crates/kh2-event/src/area_jump.rs`, `crates/kh2-game-core/src/area.rs` `run_area_script`) now calls `ctl::world_map_entrance` for the world-map
entrance and `ctl::merge_programs` for the jump's program bytes, and it carries the three fixes above (`type4_dest`, `story_override`, `delay_ticks`).
Not wired: the rest of the port has no host in the app yet (its own fade, task and menu paths do the work). Not ported here: the area thread
`FUN_001048e0` (two yield loops), the pause-poll task `FUN_0014a080`, the object constructors `FUN_001692f8` and `FUN_001694f4`, `FUN_00168d30`,
`FUN_00193610` (the file-table loader), `FUN_001698d0`, the kind-4 handler `FUN_00199b18`, the VU0 handlers `FUN_00199058` and `FUN_00198c98`,
`FUN_00198c18`, and the random-position placement `FUN_00199970` and `FUN_00199a80`; the callees the host stands in for belong to other units.

## 7. Area music

- [V] `FUN_00137150` (area load) takes the pair (field, battle) from `DAT_01c95638` (`FUN_001a7720`) and starts or
  keeps the BGM (`FUN_001371d8` equal numbers, `FUN_00137300` different: the battle switch of docs/systems/audio/audio.md). The
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

