# Progress: the player's saved game state

Work package 7S. `crates/kh2-app/src/progress.rs` holds one Bevy resource, `progress::Progress`: what the
original keeps in its save block for the player's party. Later work packages (items, prizes, magic, summons)
read and change it. Function names are addresses in the main executable (`SLUS_210.05`). No values from the
game are in the repository: the tables come from the user's disc at run time, and the routine constants
ported below are checked against the game's own code by the kh2-ee oracle test.

Marks: **[V]** read in the code (or checked by the oracle), **[I]** inferred, **[GAP]** not found.

## Where the state lives [V]

- The **save block** starts at `0x33e860` and is `0xb4e0` bytes long. At boot, `FUN_00101d68` calls
  `FUN_00105638(0x33e860)`, which clears the whole block (`FUN_00310008`), sets up its parts and then calls
  `FUN_00187d20` (below). `FUN_00105638` has no other caller.
- The **party block** is save + `0x1660` = `0x33fec0`. `FUN_00187b38(0x33fec0)` sets it up (it also runs
  from the static initialiser `FUN_001057b8`). `Progress` keeps the first `0x1998` bytes of it, the part
  `FUN_00187b38` touches, as raw bytes with typed accessors: every rule is then a port of the routine that
  reads or writes those bytes, and the oracle compares whole blocks.

### Party block layout (offsets from `0x33fec0`)

| offset | address | what | writer / reader |
|---|---|---|---|
| `+0 + (i-1) * 0xf4` | `0x33fec0` | party member records, 13 of them; `FUN_00187d40(0x33fec0, member)` gives record `i = FUN_001b58d8(member)` (member 1..15; 14 maps to 1, 15 to 6) | `FUN_001875c8` defaults |
| `+0xc3c + f * 0x28` | `0x340b24` (f=1) | form records `f` 1..9 (`FUN_00187f60`); 1..5 Valor, Wisdom, Master, Final, Anti | `FUN_00187ed0` defaults |
| `+0xdcc` | `0x340c8c` | current form (`DAT_00340c88` byte 4) | `FUN_001b4c28` |
| `+0xdcd`, `+0xdce`, `+0xdcf` | `0x340c8d..f` | active summon (1-based `sumn` index), summon level, Genie form / Tinker Bell byte | `FUN_001cf210`, `FUN_001d4ec8` |
| `+0xdd0..+0xdd3`, `+0xdd4`, `+0xdd8` | `0x340c90` | saved drive gauge bytes and two floats | `FUN_0019f920` copies them to status `+0x1b0..+0x1b8` |
| `+0xddc + 4 * w` | `0x340c9c` | world records, 19 of 4 bytes (`FUN_001a71c0`); Sora's outfit = byte 0 & 0x1f (`FUN_001a71e8`) | `FUN_001a71a8` defaults |
| `+0xe28` | `0x340ce8` | **item counts**, one byte per item `Slot` (`item` +0x12), 0x140 slots | `FUN_001a1a18` adds, `FUN_001a1c30` subtracts, `FUN_001a1cc8` reads |
| `+0xf68` | `0x340e28` | key-item bits by `Slot` (items whose `Flag0` bit 0 is set), 8 words | same three routines |
| `+0xf88` | `0x340e48` | experience total (i32) | `FUN_001b51e0` adds, caps at 9,999,999, then runs the party's level-up check (`FUN_001b52f8`); not the munny (the wallet is the save word +0x1600, `Progress::wallet`) |
| `+0xf8c` | `0x340e4c` | summon experience | `FUN_001d4ec8` |
| `+0xf90` | `0x340e50` | 8 consumable ids Sora received | `FUN_001a2120` |
| `+0xfa0` | `0x340e60` | 4 command shortcuts [I: by their filling; the menu that uses them was not read] | `FUN_001a2120` (0), `FUN_001a1a18` (1..3) |
| `+0xfec` | `0x340eac` | per-spell switch bits (bit = magic slot) | `FUN_001a4b30` / `FUN_001a4b68` (pause menu `FUN_00274e50`), read by `FUN_001a4bb8` |
| `+0x1630` | `0x3414f0` | "obtained once" bits by `Slot`, 10 words | `FUN_001a18d8` |

### Party member record (0xf4 bytes)

| offset | what |
|---|---|
| `+0`, `+2` | weapons of hand 0 and hand 1 (`FUN_0016ee98` reads `+2 * hand`; `FUN_0019f920` applies both) |
| `+4`, `+5` | HP, max HP; `+6`, `+7` MP, max MP |
| `+8`..`+0xb` | AP, strength, magic, defense |
| `+0xe` | weapon set (the `lvup` column, `FUN_001b52f8`); `+0xf` level |
| `+0x10`, `+0x11`, `+0x12` | number of armor, accessory and battle item slots |
| `+0x14`, `+0x24` | armor and accessory slots, 8 u16 each (`FUN_0019f920` applies the first `+0x10` / `+0x11`) |
| `+0x34`, `+0x44` | battle item slots and their reserved ids, 8 u16 each (`FUN_00187728`) |
| `+0x54` | ability list, 64 u16, bit 15 = equipped (`FUN_00187868`; `FUN_001c0e50` applies) |

### Form record (0x28 bytes)

`+0` the form's second keyblade (`FUN_001a2010` writes, `FUN_0016ee98` hand 1 reads); `+2` level; `+3` a byte
set to 1 with the level (no reader traced); `+4` experience (for form 5 the Anti counter, `FUN_001d4278`);
`+8` 16 ability slots, always equipped (`FUN_00187f18`).

## Inventory rules [V, all checked by the oracle]

- **Item row**: `FUN_001a66e8(id)` finds the 0x18-byte `item` row (table pointer `DAT_01c95588`). The rules
  read `Type` (+2), `Flag0` (+3), `Command` (+0x10) and `Slot` (+0x12).
- **Count** (`FUN_001a1cc8`, `Progress::item_count`): a key item answers its bit; an ability (type 0x13)
  answers 0; anything else the count byte of its slot. Spells, form items, summon charms and keyblades are
  ordinary counted items.
- **Spell level**: `FUN_001a4c78` takes the slot's first `magc` row and, when the row has an `Item`, uses
  that item's count as the level (0: no row, the spell is not owned). `Progress::spell_level(slot)`; the row
  choice itself is `magic_rules::pick_row`.
- **Room** (`FUN_001a1768(id, to, n)`, `Progress::room`): a key item only once; for a party member (`to` <
  100, n = 1) a consumable goes to the first free battle item slot whose reserved id is empty or this item,
  an ability goes to the ability list; otherwise into the inventory if the count stays at or below 99.
- **Give** (`FUN_001a18d8(id, to, equipped)`, `Progress::give`, `add_item` = n gives to the inventory):
  member slot or ability list as above, else `FUN_001a1a18(id, 1)`; a non-key item also sets its
  "obtained" bit.
- **Count add** (`FUN_001a1a18`): count (or key bit), then by type: a consumable is noted (`FUN_001a2120`:
  the 8-id list, and shortcut 0 = its command when both are empty); a **form item** runs `FUN_001a2010`
  (form record 5 `+4` = 0, and Valor/Master/Final with no second keyblade take the first keyblade in table
  order with a count, `FUN_001a20b0(2)`, which removes one); a **spell** obtained for the first time puts its
  command into the first empty shortcut 1..3.
- **Take** (`FUN_001a1c30(id, n)`, `Progress::use_item`): count minus n, at least 0; a key item loses its
  bit. (The brief named `FUN_001a12a0`: that is the item's *effect* (heal HP/MP, drive), not the count.)
- **Experience** (`FUN_001b51e0`, `Progress::add_experience`): sum, 9,999,999 when it reaches 10,000,000; no floor.
  The routine then refreshes every member's level state (`FUN_001b52f8`), not part of `Progress`.
- An item drop (`FUN_001bd490`) spawns the prize object; adding to the inventory happens when it is picked
  up (prize code, WP-7B).

## The new game [V]

`FUN_00105638` clears the block; `FUN_00187b38` then sets the party block:

- 13 member records (`FUN_001875c8`): HP, max HP, MP and max MP 100, level 1, one armor and one accessory
  slot, no battle item slot, empty ability list.
- 9 form records (`FUN_00187ed0`): no keyblade, level 1, byte `+3` = 1, experience 0, no abilities.
- current form 0, no summon, summon level 1, drive bytes `+0xdd0` = 100, `+0xdd1` = 3, `+0xdd2` = 3.
- 19 world records `{0, 0x10, 0x10, 0x10}` (outfit 0 everywhere).
- experience 0, wallet 0, no key items, spell switch bits 0, shortcuts empty.

Then `FUN_00187d20` -> `FUN_001c0cb0(0)`: the `plrp` rows with `Id` 0 (the first one and the rows after it with
the same `Id`), each through `FUN_001c0b80`. A member row (`Character` < 0x80) sets HP and max HP, MP and max
MP, AP, strength, magic, defense and the armor/accessory slot counts, sets the battle item slot count
(`FUN_00187658`), then gives each of its 32 items with `FUN_001a18d8` (bit 15 = equipped). A form row
(`Character` = 0x80 | form) adds its items to the form's ability list. On this disc Sora's row gives HP, MP and
slot counts and no items; the other members' rows fill their battle item slots and ability lists; the form rows
129..137 give the forms their abilities. Everything fits into the records, so the new-game inventory is empty
(the `KH2_PROGRESSLOG` line without the baseline items lists nothing else). `FUN_00187d20` also clears tutorial
bits (`FUN_001b6870`), not part of `Progress`.

[I] This boot-time state is the new game: it is the only writer of the whole block, and the title menu's New
Game path was not traced (it may run more story setup).

**Oracle** (`progress::tests::new_game_block_matches_the_game`, `cargo test -p kh2-app progress -- --nocapture`):
kh2-ee loads the user's executable, the `item`, `plrp`, `cmd` and `magc` tables from the disc at their pointers
(`DAT_01c95588`, `DAT_01d5df5c`, `DAT_01d5d798`, `DAT_01c95520`), clears the save block, runs `FUN_00187b38` and
`FUN_001c0cb0(0)` and compares the 0x1998 bytes with `Progress::new_game`. It then compares
`FUN_001a1cc8` with `item_count` for all 466 items, runs a give/take sweep (`FUN_001a18d8` to the inventory, to
Sora and to Donald, `FUN_001a1c30`) on both and compares the blocks again, and compares `FUN_001a4c78` with
`pick_row(spell_level)` for the six slots at levels 0..3. The test prints
`oracle: new-game party block equal (6552 bytes, 20 plrp rows with Id 0); 466 item counts equal; ...`.
Left out of the sweep: item 0x20 and spells, whose side calls (`FUN_001803b0`, `FUN_00180450`) act outside
the block; `Progress` does not run them either.

## Start state in the app

`Progress::start` (from `progress::ensure`, called by `ProgressPlugin` and by the plugins that read `Progress`
while they build) does, in order:

1. the new game above, from the user's disc;
2. `KH2_STATE=<path>`: the JSON file (schema below) on top of the new game, and nothing else;
3. otherwise the **app baseline** (below).

`KH2_PROGRESSLOG=1` logs `progress: state from <new game + app baseline | file path>: experience ..; munny ..; spells
[...]; forms owned [...] levels [...] keyblades [...]; weapon ...; abilities [...]; ...; inventory [...]` and,
with the baseline, `progress: app baseline (not the original's new game, docs/progress.md): ...`.

### App baseline (a deviation, to remove when the story gives these)

The original's new game owns no spell, no form, no keyblade in hand and no Guard: in the real game the story
gives them later [GAP: where]. The app has always started with them so that the magic menu, the drive forms
and the guard work (the `magic-fire`, `drive-valor` and combat scenarios). Without `KH2_STATE`, `Progress`
therefore adds, on top of the new game and only where missing:

- one spare Kingdom Key per form that takes one (Valor, Master, Final), then the four form items Valor..Final
  through the original's give, so `FUN_001a2010` hands each of those forms a Kingdom Key as before;
- the items of magic slots 0..3 (Fire, Blizzard, Thunder, Cure) at count 1 (level 1);
- (WP-F1 item 6) Donald's weapon word = item 75 and Goofy's = item 49 when they are 0 (`DONALD_STAFF`,
  `GOOFY_SHIELD`): the original's new game leaves them 0 and their later writer was not found (docs/party.md
  "Weapons"), so the members' hand objects (`FUN_0016ee98`) come from the baseline, logged as
  `member 2 weapon item 75 equipped`;
- the Kingdom Key in Sora's hand 0;
- Guard equipped in Sora's ability list, unless `KH2_ABILITY_START=data`;
- Dodge Roll growth level 1 (`Progress::dodge_roll`, the user's wish 2026-10-06). The USA disc has no Dodge
  Roll ability item (Final Mix has it as growth slot 4, docs/final-mix.md), so the level is an app field
  beside the save block, not part of the original's records. `KH2_ABILITY_START=data` drops it with the
  Guard, and a `KH2_STATE` file sets it with `"dodge_roll": <0..5>` (without the key it is 0). The
  harness `KH2_GROWTH=a,b,c,d,e` fifth value overrides it (docs/movement.md section 2).
  A `KH2_STATE` key `sora_items` (array of item ids) gives each item to Sora through FUN_001a18d8(id, 1, 0): free battle item slots first, then the stock; it fills the received list. The helpers `battle_items`, `take_item` (FUN_001a1ba8), `refill_battle_items` (FUN_001877c8), `equip_battle_item` (FUN_0026c698) and `recent_items` are read by items.rs.

This reproduces the previous defaults exactly (`progress::tests::baseline_gives_the_old_app_defaults`).

### Who reads it, and the env vars that still win

| system | reads from `Progress` | env var on top (wins) |
|---|---|---|
| magic (`magic.rs` setup) | `spell_levels()`: the six slots' item counts | `KH2_MAGIC=f,b,t,c[,m,r]` |
| drive (`drive.rs` `DriveState::from_env`) | owned forms (`owns_form`: the form items), form levels (records `+2`), second keyblades (records `+0`), spare keyblades (`keyblades()`), outfit (`outfit(world)`) | `KH2_DRIVE_OWNED`, `KH2_DRIVE_LEVEL`, `KH2_DRIVE_INVENTORY` (either of the first or third switches the keyblades back to the old harness rule), `KH2_DRIVE_OUTFIT` |
| abilities (`abilities.rs` `with_defaults`, then `follow_progress` whenever `Progress` changes: WP-G8) | `equipment()` (Sora's weapons, armor, accessories), Sora's ability list (the growth levels are the highest equipped growth ability of each group, docs/menus.md section 17) and `dodge_roll` | `KH2_ABILITY_IDS`, `KH2_ABILITIES` (added on top, kept across the copies); `KH2_ABILITY_START=data` drops the baseline Guard; `KH2_GROWTH` overrides the growth levels outside a form |

Not wired: HP/MP/stats of the member record (the app takes them from `lvup`/`plrp` in `data.rs`), the saved
drive gauge bytes (the drive system keeps its own start points), the summon state (no summon system yet).

## `KH2_STATE` file schema

A JSON object; every field is optional and is applied in file order on top of the new game. Unknown fields are
an error (logged; the app then falls back to the new game + baseline). Example: `local/state-example.json`
(git-ignored).

| field | type | effect |
|---|---|---|
| `experience` | int | the experience total at 0x340e48 (capped at 9,999,999) |
| `munny` | int | the wallet, save word +0x1600 (held at 999,999) |
| `items` | object `"<item id>": count` | sets inventory counts directly (0..99; key items: 0 or not) |
| `magic` | array of up to 6 ints | spell levels by magic slot (sets the count of the slot's `magc` item) |
| `weapon` | int | Sora's hand 0 item |
| `armor`, `accessories` | array of item ids | Sora's slots (the slot count grows to fit, max 8) |
| `abilities`, `abilities_off` | arrays of ability item ids | replaces Sora's ability list: equipped ones, then unequipped ones (growth abilities too: 94..97 High Jump, 98..101 Quick Run, 102..105 Aerial Dodge, 106..109 Glide, LV1..MAX; `local/g8/state-swap.json`) |
| `dodge_roll` | int 0..5 | Sora's Dodge Roll growth level (app field, not an original record; docs/movement.md section 2) |
| `forms` | object `"<form 1..9>": { "owned": bool, "level": int, "exp": int, "weapon": int }` | `owned: true` gives the form item through the original's give (so the form takes a keyblade by `FUN_001a2010` if the inventory has one); the others set the record |
| `summon` | `{ "level": int, "exp": int }` | summon level and experience |
| `outfits` | object `"<world>": outfit` | low 5 bits of the world record's byte 0 |
| `spell_switch` | int | the switch bit word |
| `party_settings` | object `"<member id>": byte` or `"<member id>": [bytes]` | a member's AI settings block (record `+0xd4`, 32 bytes, zero at a new game by `FUN_001875c8`): a single number is byte 0, the battle style 0..5; an array sets bytes from 0 on (byte `1 + k` = use setting of item / ability `k`, docs/party.md). Member ids are the party ids (2 Donald, 3 Goofy). `Progress::member_setting` / `set_member_setting` read and write it; a slot file's `block` carries it (test `member_settings_default_to_zero_and_round_trip`). `party.rs` reads byte 0 every frame (`KH2_PARTY_STYLE` still overrides) and, since WP-PA4, the use-setting bytes; the Customize page 0x1b (docs/menus.md 14b) writes them (`menu::customize::write_friend`; test `the_pages_writes_survive_a_state_file_round_trip`) |
| `shop_seen` | array of places 0..127 | shop items seen (save `+0x3714`, one bit per place in the shop file's item list; `Progress::shop_seen` / `set_shop_seen`; WP-SH3, docs/menus.md 24.2). A slot file carries it |
| `shop_visits` | array of bits 0 / 1 | the shops' first-visit bits (save `+0x3724`; `Progress::shop_visit` / `set_shop_visit`; docs/menus.md 24.1). A slot file carries it |
| `record_bits` | array of ints | the record bit words (save `+0x1550`, `FUN_001a52b0`); a slot file carries them |
| `story_raw` | `{"words": hex, "programs": hex, "locks": [state, message, ...]}` | the story flag words, the area program table and the exit locks as `slot_json` writes them (replaces them whole; the new game's effect lists stay) |
| `moogle` | `{ "known", "learned", "made", "seen", "unlocks", "rewards": [bits], "points": n, "level": n, "stock": [50 ints], "totals": [50 ints] }` | the moogle's synthesis record (save `+0x3728`; `Progress::moogle`; `rewards` is the reward word at `+8`, docs/menus.md 24.3); every key optional. A slot file carries all of them, and a `block` key resets the record (and the shop and record bits) first, so a file without them loads the defaults; tests `a_slot_file_carries_the_moogle_the_shop_bits_and_the_story`, `rewards_are_not_paid_again_after_a_reload` |
| `story_flags` | array of flag ids | each set through the game's setter `FUN_0017ff60` in file order, side effects included (area program table, exit locks, other flags; section "Story flags" below) |
| `story_clear` | array of flag ids | each cleared through `FUN_00180298` (only the bit) |
| `comment` | any | ignored |

`Progress::story_json` writes the set flags back as `{"story_flags": [...]}` (test
`story_flags_round_trip_through_the_state_file`). Reading it runs the side effects again in id order, so it
rebuilds the flags exactly and the program table and locks as those setters leave them.

## Story flags (WP-E5)

`progress::Story` (held by `Progress`, shared with the script hosts through `progress::shared_story`) keeps three
save-block regions, all 0 in a new game (`FUN_00105638` clears the block at boot) [inferred: no other new-game
writer of them was found]:

| region | EE address | layout | writers / readers |
|---|---|---|---|
| flags | `0x33f6b0`, 0x260 bytes | 19 pages of 0x20 bytes, one per world: flag `id` is bit `id & 0x1f` of word `(id & 0x3ff) >> 5` of page `id >> 10` (0xffff tests clear). The world-point bits of `FUN_00194fd8` start right after, at `0x33f910` | `FUN_001802e0` test, `FUN_00180298` clear, `FUN_0017ff60` set |
| area programs | `0x33e86f + world * 0xc0 + area * 3` | (map, btl, evt) | flag op 1 (`FUN_00104b90`, 0xff keeps), read by `FUN_00104bd0` into the area record (`area.rs`) |
| exit locks | `0x33f918 + 4i` | (state, message) per `dsa` entry `i` | flag ops 2 `FUN_00195260`, 3 `FUN_00195288`, 4 `FUN_001952a0` (0xffff), 5 `FUN_00195300` (0); read by `FUN_00195358` (`area_jump::ExitLocks`) |

The setter runs the flag's list from `00progress.bin` (world entry `id >> 10`): ops 1, 2, 3, 4, 5, 7 (clear) and
0xc (set those not yet set, recursive) are ported; ops 6 (`FUN_001a7730`: `0x33fe0c[world]`), 8 / 9 (record bits
`0x33fdb0`), 0xd (`FUN_001b4e80`: `0x341858 + 4 * world`), the world-point bits of ops 4 / 5 and the calls after the
list (`FUN_001803b0` .. `FUN_00195130`) are not (recorded in `Story::unported`, logged at start). The kh2-ee oracle
`story_flag_setter_matches_the_game` runs `FUN_0017ff60` for every flag of every world and compares the three regions.
Users: hosts 0.14 / 0.15 / 0.70 (`enemy_host.rs`), area-script opcode 0x19 (`area.rs`), the Struggle mission's
flag reads (`struggle.rs`). Not yet wired to it: the menus' and items' flag tests (`items.rs`, `menu_*.rs` still
assume "no story flag set").

## Decompile index

| address | what |
|---|---|
| 0x101d68, 0x105638 | boot: clears and sets up the save block at 0x33e860 |
| 0x1057b8 | static initialiser of the same parts |
| 0x187b38, 0x1875c8, 0x187ed0, 0x1a71a8, 0x114318, 0x32c8f8, 0x1d7640 | party block set-up, member record, form record, world record defaults |
| 0x187d20, 0x1c0cb0, 0x1c0b80, 0x187658 | new-game `plrp` rows, one row, item slot count |
| 0x1c0b70, 0x1a66b8, 0x193610 | table pointers (`plrp` `DAT_01d5df5c`, `item` `DAT_01c95588`) set by the table loader |
| 0x1a66e8, 0x1a6730 | item row lookup, item row iteration |
| 0x1a1cc8, 0x1a1d50 | item count; count including members' slots and ability lists |
| 0x1a1768, 0x1a18b8, 0x1a18d8, 0x1a1a18, 0x1a1c30 | room, can-give, give, count add, take |
| 0x187728, 0x187868, 0x187f18 | battle item slot, ability slot, form ability slot |
| 0x1d3f18, 0x1d3da8, 0x37c556 | growth upgrade of an ability list (`Progress::upgrade_ability`, kh2-ee oracle `growth_upgrade_matches_the_game`), the form level-up that calls it on Sora's list at the display level minus one, the growth chains |
| 0x1a2120, 0x1a1f78, 0x1a4dd0 | consumable list and shortcut 0, shortcut command, magic resource command |
| 0x1a2010, 0x1a20b0, 0x1b6f48 | form item obtained (keyblade take), first owned item of a type, `cmd` lookup |
| 0x187d40, 0x1b58d8, 0x187f60 | member record, member index, form record |
| 0x1b51e0 | experience add and cap |
| 0x1a71c0, 0x1a71e8 | world record address, outfit bits |
| 0x1a4b30, 0x1a4b68, 0x1a4bb8 | spell switch bit clear, set, test |
| 0x1a4c78 | spell row from the item count |
| 0x0016ee98, 0x0019f920 | weapon word per hand; battle state from the member record |
| 0x1a12a0 | item effect (heal, drive) on use: not the count |
| 0x1bd490 | item drop: spawns the prize object |

## Gaps

- Where the story gives Sora his keyblade, spells, forms and Guard (the app baseline stands in for them); the
  title menu's New Game path (searched: callers of `FUN_00105638`, `FUN_00187d20`, `FUN_001c0cb0`, `FUN_00187b38`
  with `mips_scan --jal`; only the boot path and the two static initialisers were found).
- The selector of the `plrp` rows with `Id` 1 (only `FUN_001c0cb0(0)` was found, as in docs/abilities.md).
- The meaning of form record byte `+3`, of the saved drive bytes (`+0xdd0` = 100 does not fit the 0..99
  fraction of status `+0x1b0`) and of world record bytes 1..3 (`0x10` each).
- `FUN_001803b0` (item 0x20) and `FUN_00180450` (first spell: tutorial prompts by story flags) are not
  ported; they write outside the party block.
- A spell's shortcut 0 command (`FUN_001a1f78` type 0x12 -> `FUN_001a4dd0`) needs the loaded magic resources;
  `Progress` answers 0 (they are empty at boot, the oracle agrees).
