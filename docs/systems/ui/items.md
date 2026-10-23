# The field command menu and the item command (WP-7C)

Module `crates/kh2-party/src/items.rs`. Function names are addresses in the main executable (`SLUS_210.05`).
Marks: **[V]** read in the code (or checked by the oracle), **[I]** inferred, **[GAP]** not found.

## 1. The command manager [V]

The command menu is driven by one object per player, the command manager. Its methods are the vtable at
0x35f260 (the base class's is 0x35f238, set by the constructor `FUN_001baf48`):

| vtable | function | what |
|---|---|---|
| +0x8 | `FUN_001bbeb8` | reset: `FUN_001bb298` (lists cleared, list 11 gets command 0x1b7, flags) and the deck pushed |
| +0xc | `FUN_001bc2b8` | rebuild every list (section 2) |
| +0x10 | `FUN_001bbfd8` | one frame of input (section 4) |
| +0x14 | `FUN_001bc0c8` | issue the selected command (section 4) |

Fields (offsets in the manager):

| offset | what |
|---|---|
| `+0x20` | flag word: 2 cancel pressed at the deck, 8 Square pressed, 0x10 no consumable at the last reset, 0x20 no form item owned |
| `+0x24 + 0x74 i` | list `i`, 18 of them. A list: `+0` its index (the HUD's "kind"), `+4` the manager, `+8` the count, `+0xc` up to 8 entries of 12 bytes, `+0x6c` the cursor, `+0x70` the cursor's command (-1 none) |
| `+0x84c` | pointer to the top of the open-list stack; `+0x974..+0x980` the stack (4 list pointers, `FUN_001bb558` pushes, `FUN_001bb5c0` pops) |
| `+0x850 + 0x24 k` | 7 command slots (`FUN_001bb538(k)` answers slot `k` when its command is set): 1 is the reaction command (docs/systems/drive-limits-summons/contract.md), 3..6 the shortcut override, 0 and 2 stand in for commands with `CmdIcon` 1 and for command 0x51 (`FUN_001ba748`) |
| `+0x96c` | the player object; `+0x970` the pad (its `+8` word is the newly pressed logical bits) |
| `+0x9d4` | the spell commands (`FUN_001bbee8`), `+0x9e2` the summon commands, `+0x9ec` the members of the target list |
| `+0xa34` | the deck in use, list 0 or 6 |

An entry (`FUN_001ba670`): `+0` flags (bit 0: it came from a command slot), `+2` the command id, `+4` a count (1, or for
items the number of slots carrying it), `+5` the status (`FUN_001ba2c0`, then the overrides of `FUN_001ba748`), `+8` a
target record pointer (0 for the lists below). The HUD (0x13ca30 walks the stack, 0x13c4b0 draws one list) colours the
name by the status byte (tables 0x355600 normal, 0x355628 selected: 0 light, 1 grey, 2 orange, 5 and 6 grey), so the
"style" of docs/systems/ui/contract.md is the status. The brief's "player +0x974" is the manager's stack: 0x13ca30 reads it through the
HUD object's `+0x290`, the manager.

## 2. The lists and their builders [V]

`FUN_001bc2b8` rebuilds, in this order: list 4 targets (`FUN_001bcb98`), list 8 (`FUN_001bcd78`, the friends: party
members with their target records), 1 magic (`FUN_001bc870`), 2 items (`FUN_001bc910`), 3 forms (`FUN_001bca70`), list 9
(`FUN_001bcc60` -> `FUN_001d2320`; [I] the limits, Limit's `SubMenu` is 9), 7 summons
(`FUN_001bc8b0`), 0 the deck (`FUN_001bc578`), 6 the second deck (`FUN_001bc6e0`), 5 shortcuts (`FUN_001bcc78`). The
sub-lists come first because a deck entry's status reads them. Its per-frame caller was not traced [I: every frame
before the input method; the app rebuilds every frame, which the cursor rules below make harmless].

Every entry goes through `FUN_001ba748(list, command, target, count)`:

1. the command row (`FUN_001b6f48`; a magic command whose id is not 2 resolves to the loaded level's command);
2. a command whose `CmdIcon` is 1 takes command slot 0, command 0x51 slot 2, when that slot is set (the app has none);
3. status = `FUN_001ba2c0` (section 3);
4. an item command (`Execute` 3) with count 0 gets status 6;
5. a command with a sub-menu (`SubMenu` >= 0 and not 4): status 5 when that list is empty, else status 1 when no entry
   of it has a status of 3 or below (`FUN_001ba910`); otherwise the status stays.

The lists:

| list | builder | contents |
|---|---|---|
| 0 deck | `FUN_001bc578` | (command slot 3 set: the four shortcut slots instead, `FUN_001bc800`). The attack command (`FUN_001bc4c8`); Magic (2) when the spell list is not empty, else the empty bar 0x68; Items (3) unless flag 0x10 is set or no consumable was ever received (`FUN_001bbca0`: the first id of the received list `0x340e50`), else 0x68; then, unless flag 0x20: Revert (5) when the player is Sora in form 1..5 (`FUN_001793a8`), status forced to 5 in Anti during a battle; else Drive (4), status forced to 5 when the player's form is not 0; with flag 0x20 0x68 |
| attack command | `FUN_001bc4c8` | 0x88 in world 9; 0x16d in Wisdom form; 0x21d while the MP charge runs with ability bit 107 (`FUN_001768b8`: block `+0xc` & 0x800); else 1. The empty bar 0x68 when the story flag test `FUN_00180680` fails |
| 1 magic | `FUN_001bc870` | the spell commands of `+0x9d4` (`FUN_001bbee8`: slots 0..5, docs/systems/magic/contract.md) |
| 2 items | `FUN_001bc910` | the user's battle item slots (party record `+0x34`, reserved ids `+0x44`, count `+0x12`, docs/systems/core-runtime/progress.md): for each slot with an item or a reserved id, the command of the item's row (`item` `+0x10`, the reserved id first); one entry per command; a slot that carries the item adds 1 to the entry's count, a slot that only reserves it makes an entry of count 0, status 6. Then `FUN_001badc8` (the cursor follows its command) |
| 3 forms | `FUN_001bca70` | for index 1..5 of the u16 table at 0x37b420 (the form items, Valor, Wisdom, Master, Final, Anti), when the item's count is above 0: the item's command, left out when `FUN_001c7970` answers 2 (a companion the form needs is not in the party, `FUN_001c77f0`), status 5 when it answers 1. Flag 0x20 is set when no form item is owned; the list is not rebuilt while flag 0x20 or the lock `0x35fec0` bit 1 is up |
| 4 targets | `FUN_001bcb98` | for the three party slots (`FUN_00175b68`): each valid member that is not a summon and has a status; the member goes to `+0x9ec`, the entry is the member's command from the u16 table at 0x37bab0 (Sora 42) |
| 5 shortcuts | `FUN_001bcc78` | the four shortcut commands of `0x340e60` with the count of slots carrying an item |
| 6 second deck | `FUN_001bc6e0` | the attack command; Dismiss (0x13c) while the summon state `DAT_01d5e368` is set (`FUN_001c6918`), else Summon (0x48) when the summon list is not empty, else 0x68; Party (0x49) when `FUN_001807a0` holds, else 0x68 (0x25d in map mode 2 with 0xc9); Limit (0x51) when `FUN_001807f8` holds, else 0x68 |
| 7 summons | `FUN_001bc8b0` | the summon commands of `+0x9e2` |

On this disc: Attack 1 (`SubMenu` -1), Magic 2 (1), Items 3 (2), Drive 4 (3), Revert 5, Summon 0x48 (7), Party 0x49 (8),
Limit 0x51 (9), the empty bar 0x68 (`Execute` 0, its name is "?"); Potion's command 23 and Ether's 21 have `SubMenu` 4,
and so has Cure (52): choosing them opens the target list.

## 3. Availability: `FUN_001ba2c0` [V]

By the command's `Execute`: 0 gives 5; 1 (attack) gives 0 when the player's `+0xaf0` is set (from the player object's
entry, `FUN_001746e0` -> `FUN_001d5e48`), else 5; 2 (magic) docs/systems/magic/contract.md (MP, the charge, the spell switches, and the
special-battle bit 11 `FUN_001b9f70`); 3 (items) nothing of its own; 4 (drive) and 0x48 (summon) `FUN_001ba568` (a
command with a sub-menu passes; a form's command needs no lock, no form 6 or 11, the companions and `Cost` <= whole
points; summon also the special-battle bit 14); 5 (revert) `FUN_001c7a38` and the lock `0x35fec0`; 0x51 `FUN_001b3e38`;
0x67, 0x68 give 5; 0x13c (Dismiss) gives 5 unless the summon state `DAT_01d5e368` (`FUN_001c6918`) is 2, and 5 while
the limit task exists (`FUN_001b3c00`: `DAT_01d5d6b0` != 0; docs/systems/drive-limits-summons/contract.md); 0x14a and 0x1c4 their own tests; anything
else 0. Then the locks
`FUN_001b70b8` (`Flags` bit 1 airborne, bit 16 outside battle, without bit 5 the global lock `DAT_0035d268` & 2) and
`DisableForm` (`FUN_001b7290`) give 5. Port: `items::availability`.

## 4. Input and issue [V]

`FUN_001bbfd8`, every frame: `FUN_001bc370` picks the root (L1 held, logical 0x800000, opens the shortcut list 5 as the
root; otherwise the deck `+0xa34`; a stack whose bottom is not the root is dropped and the root's cursor reset); then
`FUN_001ba9b0` on the top list; when that read something and Left (logical 0x10000) was pressed: with more than one list
open the top one closes (sound 4), at the deck the other deck is swapped in (`FUN_001bc410`, sound 0x14, gated by
`FUN_001807a0`).

`FUN_001ba9b0` (one list; the pad bits are the logical bits of the field button set 0x379768, `FUN_00167908`):

1. `FUN_001bacd0`: from the top down to the deck, the first list that has become empty closes with everything above it
   (sound 4) and nothing else is read this frame;
2. a list that is not a deck gets `FUN_001badc8`: the cursor stays on its command (`+0x70`), moves to the entry that has
   it, or, when it is gone, stays and remembers the new entry's command; past the end it resets to 0;
3. confirm (logical 0x4 or 0x200000000) -> `FUN_001bb930`; else cancel (0x8): a sub-list closes (sound 4) and when the
   list below is a deck its cursor goes back to 0 (`FUN_001ba6e8`); at the deck flag 2 is set; else, when the story
   flag test `FUN_001806c8` holds: Square (0x200) sets flag 8; up (0x4000) and down (0x8000) move the cursor and both
   wrap (first <-> last), sound 1; Right (0x10000000) opens the selected entry's sub-menu when its status is 3 or below
   (sound 3).

`FUN_001bb930` (confirm): an entry of status 3 or below goes to method `+0x14` = `FUN_001bc0c8`; when that issued a
command, every list down to the deck closes and every cursor on the way, the deck's included, goes back to the first
entry. An entry of status 5 or 6 buzzes (sound 5) unless flag 8 is set.

`FUN_001bc0c8`: in the target list the command is the one selected in the list below and the target is the member at
`+0x9ec[cursor]`; in a shortcut list the target is the player for `SubMenu` 4; elsewhere a command with a sub-menu opens
it (sound 3, nothing issued), any other is issued with the entry's target or the lock-on controller's record. Issuing is
`FUN_001bb8c0`: `FUN_001b6fd0` (the command's `CmdReceiver` decides who must accept it, `FUN_0019abb0`) then the request
(`FUN_0017ad50`). The sounds are ids of `FUN_002c14a8` (1 cursor, 3 open, 4 close, 5 refused, 0x14 deck swap).

So the original closes the whole menu after a spell or an item is used and puts the cursor back on Attack; an item and
Cure first ask for the target. The app until now kept the Magic list open after a cast (docs/systems/magic/magic-spells.md "The command
list" [GAP], now answered).

Keys in the app: T up, G down, J confirm (the attack button; on the deck's attack command the press stays an attack for
Sora's controller), B cancel, H the d-pad Left (closes the top list; at the deck the deck swap to the second deck with
Summon / Dismiss, Party and Limit; `KH2_KEYS` `t:H` presses it). Right, Square and L1 have no key (the arrows turn the camera), so the
shortcut list is ported but not reachable. Port: `CommandLists::input`, `select_root`.

## 5. The item command [V]

1. Issued item command (`Execute` 3, `Argument` = the item id) with the target member -> the player's command executor
   `FUN_00174cf8` case 3 -> `FUN_00175e20(player, command, target record, 1)`: `+0xb2c` the item, `+0xb30` the target's
   member id (0 for none), `+0xb2e` = 1 (take it), and motion request 9 with blend 8.0 (`DAT_0037a4b8`, FUN_0017e220).
   Unlike magic there is no combo-window test (`FUN_00176b00` is not called).
2. Frame trigger 6 of that motion (`FUN_00188498`, jump table 0x37aa78, case 6) -> `FUN_001b8710`: `FUN_00175ec0` takes
   the item from the user (`FUN_001a1ba8(item, user's member id)`: the LAST battle item slot holding it is emptied, the
   reserved id stays; `progress::Progress::take_item`), and when that worked queues a use (`FUN_001b82b8`, a 0x18-byte
   object: time `DAT_0037bba4` = 30.0, item, user, user's member id, target; target 0 for the group items 5..7 and
   0x83, `FUN_001a1fe0`).
3. Each frame (`FUN_001b84c8`): while the time is at least `DAT_0037bba8` (30.0) system effect 0 of `00effect.bar`
   starts at the user's position (`FUN_001b8400`); the time drops by the frame step; at 0 the item row's `StatEntry`
   effect starts on the target (every party member for target 0, `FUN_001b85e8`) and `FUN_001b8658` applies
   `FUN_001a11a0(item, target, user, 100 + the user's Item Boost, ability block +0x4c)`.
4. `FUN_001a11a0` -> `FUN_001a12a0(row, target, user, mult)` once, or for the four party slots for a group item. With
   `amount` = the row's word at +4 (`Flag1 | Rank << 8`; Potion 40, Hi-Potion 60, Ether 100, Elixir 255):
   items 1, 2, 5 add `(max HP * amount * mult + 9999) / 10000` HP; 3 and 6 the same of max MP; 4 and 7 both; 0x83 the
   whole maximum HP and the MP share; 0x112, 0x113 add `amount * 100` to the drive gauge (`FUN_001a0290`); 0x114..0x117
   (the boosts) raise stats (`FUN_001a0ce0` ..`0dd0`, not ported: the game uses them only from the pause menu). HP is
   held between the record's minimum and maximum (`FUN_001a0100`); an MP gain during the MP charge shortens the charge by
   `gain * sstm[0x7c]` (12.0 on this disc) instead, and a charge brought to 0 ends and refills MP (`FUN_001a0180`,
   `FUN_001a0540`, `FUN_001a05a8`).

**Oracle** (`items::tests::item_effect_matches_the_game`, `cargo test -p kh2-party item_effect_matches -- --nocapture`):
kh2-ee runs the game's `FUN_001a12a0` on the user's executable with the `item` table at its pointer and a stub for
`FUN_0019fe48` that returns a status record in free memory, for every item below 0x114, four HP/MP/charge/drive
states and Item Boost 0, 25 and 50 %: `oracle: FUN_001a12a0 equal for 2688 cases`. The take and refill
(`FUN_001a1ba8`, `FUN_001877c8`) are compared on the party block in `progress::tests::new_game_block_matches_the_game`
(`oracle: battle item take/refill equal`).

The battle item slots refill from the stock (`FUN_001877c8`: an empty slot whose reserved id is in the stock takes one)
when `FUN_001a1e68` or the pause menu code (`FUN_0026a528`) calls it [GAP: when `FUN_001a1e68` runs]. The app does
not call it yet.

## 6. What the app does

- `CommandLists` (resource) holds the 18 lists, the stack, the deck and the flags; `command_menu` (Update, after the
  pad) rebuilds, selects the root, reads T/G/J/B and fills `hud_menu::CommandMenu` with one page per stacked list (the
  list index is the page kind, the status the style). An issued spell goes to `magic::MagicState::request_cast`, an
  item to the item use, Attack stays with Sora's controller, a summon command (from the SUMMONS list) and Dismiss go to
  `summon::SummonQueue` (`summon.rs` runs them, docs/systems/drive-limits-summons/contract.md); Drive, Revert and the other commands are not run from
  the menu in the app (logged `items: command ... is not run from the menu in the app`).
- `ItemUse` and `run` (FixedUpdate, `Sim::Sora`): the request waits up to 20 ticks for Sora to be free (ground, air,
  or the grounded / air / blend-to-idle range of an attack or cast motion [I: the window of `FUN_00174cf8`'s caller]),
  then motion request 9 (battle or field set by the battle flag) in `sora::State::Magic`; trigger 6 takes the item
  from Sora's slots and queues the use; the use starts effect 0, waits, starts the `StatEntry` effect on Sora and
  applies `item_effect`. WP-IT1: the target list is Sora plus the field members (`items::target_party`: `party::PartyMember`
  by slot, each with its command from the table at 0x37bab0, FUN_001bcb98), so Donald and Goofy are offered; the chosen
  member id travels as the use's target. On landing the effect goes to Sora (target 0 or his id) and to the members that
  match (target 0: all; else the member id), through `item_effect` on the member's `hp/hp_max/mp/mp_max/charge`; a KO'd member
  is not skipped (WP-PL3, below): his HP gain goes through the HP method; the `StatEntry` effect starts on Sora only [GAP: no
  owner binding for a member entity is known]. FUN_001b8658 then calls `Progress::count_healed` once per use when
  `items::counts_as_healed` holds: the user (Sora) is member 1 and FUN_001a11a0 answered true (`target != user`, or for a
  group item any other slot holding a member; it counts even when the heal changed nothing). Log `items: Healed Party
  Members counts <n>`.
  The pause menu's Items pages stay Sora-only (`menu_items.rs`: member rows, the "Sora" literals and the equipment pages are
  one member; its use calls FUN_001a11a0 directly, FUN_0025ea40, so it counts nothing) [GAP: Donald and Goofy as targets
  there need the members page and its names reworked].
- Inputs taken as constant in the app: no story flags (the tests `FUN_00180680`, `FUN_001806c8`, `FUN_001807a0`,
  `FUN_001807f8` read clear, so they pass), no command slots, no special battle, no map command mode, no command lock,
  `+0xaf0` set, the object flag `+0x588` 0x1000000 set, `FUN_001c7a38` true, `FUN_0019abb0` true (every issued command is
  accepted; the request then waits for Sora). Flags 0x10 and 0x20 are recomputed at each rebuild (the original keeps
  0x20 latched until a reset and sets 0x10 only at a reset).

Logs: `items: list <n> opened (...)`, `items: <name> chosen for member <m>`, `items: use <name> ...: motion request 9 ...`,
`items: trigger 6 at frame ..: <name> taken from Sora's slots (<a> -> <b>)`,
`items: used <name>: HP <a> -> <b>, MP .., count <n> -> <n-1> ...`; `KH2_ITEMLOG=1` adds every stack change.
`KH2_STATE` key `sora_items: [ids]` gives items to Sora as a pickup does (`FUN_001a18d8(id, 1, 0)`: free battle item
slots first, then the stock; the received list fills), e.g. `local/state-potions.json`
`{ "sora_items": [1, 1], "magic": [1, 1, 1, 1] }`.

## 7. Synthesis materials and recipe items (WP-SH2)

Items of type 0x10 are the synthesis materials (`FUN_00284798` lists them in table order; their rank, item +5, picks
their points in `mixdata.bar`'s `exp`). Opening the moogle's third row (docs/systems/ui/menus-shop.md 23) moves every material held
into the moogle's stock (save +0x3728 +0x34, `progress::Moogle`) with `FUN_001a1c30`, and, once the record's unlock bit
10 is set, takes the recipe items (`reci` +0: ids such as 0x17e and 0x1c1..0x1df) that teach a recipe. A make adds its
item with `FUN_001a1a18(item, 1)` and refuses when 99 are held (`FUN_00290e68`).

WP-SH3: the opening also pays the moogle's collection rewards (docs/systems/ui/menus-shop.md 24.3): 24 of the 50 `cond` rewards are items
(`FUN_001a1a18(item, 1)` when `FUN_001a18b8` allows it: fewer than 99 held, a unique item not yet owned), the others
unlock a Free Development set (rewards 0 and 1) or a record bit that opens a group of the moogle's shop (0x49..0x60,
docs/systems/ui/menus-shop.md 22.2). A shop item that is new (save `+0x3714` bit of its place in the shop file's list clear) carries a
NEW tag in the Buy list until the cursor has been on it (docs/systems/ui/menus-shop.md 24.2).
## Decompile index

| address | what |
|---|---|
| 0x13ca30, 0x13c4b0, 0x13c9f8 | HUD: walk the stack, draw a list |
| 0x1baf48, 0x1bb298, 0x1bbeb8 | manager constructor, reset, reset + push the deck |
| 0x35f238, 0x35f260 | base and player manager vtables |
| 0x1bc2b8 | rebuild every list |
| 0x1bc578, 0x1bc4c8, 0x1bc6e0, 0x1bc800 | the deck, its attack command, the second deck, the shortcut override |
| 0x1bc870, 0x1bbee8, 0x1a4dd0 | magic list, spell commands |
| 0x1bc910, 0x176898 | items list, the user's party record |
| 0x1bca70, 0x1a1e38, 0x37b420, 0x1a1fc0, 0x1c7970, 0x1c77f0, 0x1c77a8 | forms list, form item table, item command, companion tests |
| 0x1bcb98, 0x1b58a8, 0x37bab0 | target list, member command table |
| 0x1bcc78, 0x1bc8b0, 0x1bcd78, 0x1bcc60 | shortcuts (list 5), summons (7), friends (8), list 9 |
| 0x1ba740, 0x1ba748, 0x1ba670, 0x1ba650, 0x1ba910, 0x1ba958 | list clear, add, entry fill, entry reset, any usable, add a 0-terminated list |
| 0x1ba6e8, 0x1ba6f8, 0x1badc8 | cursor reset, set, follow its command |
| 0x1bb538, 0x1bb558, 0x1bb5c0, 0x1bb610, 0x1bbab8 | command slot, push, pop, pop all, set the root |
| 0x1baee0, 0x1baf00 | deck kinds 0 / 6, special roots 5, 10, 12..17 |
| 0x1ba2c0, 0x1b70b8, 0x1b7290, 0x1ba568, 0x1b7098, 0x1b9f70, 0x1ba0c0, 0x1b9550, 0x1c7a38 | availability, locks, form mask, drive test, special battle |
| 0x1bbfd8, 0x1bc370, 0x1bc410, 0x1bc468 | input frame, root select, deck swap |
| 0x1ba9b0, 0x1bacd0, 0x1babf0, 0x1bb930, 0x1ba6d8 | list input, close empty lists, shortcut input, confirm, status 5/6 |
| 0x1bc0c8, 0x1bb870, 0x1bb8c0, 0x1b6fd0 | issue (player and base), request |
| 0x180680, 0x1806c8, 0x1807a0, 0x1807f8, 0x1802e0 | story flag tests (flags at 0x33f6b0) |
| 0x379698, 0x379768 | pad bit tables (shared, field set) |
| 0x174cf8, 0x175e20, 0x37a4b8 | command executor, item command, its blend |
| 0x188498, 0x37aa78, 0x1b8710, 0x175ec0, 0x1a1ba8 | frame triggers, their jump table, trigger 6, take the item |
| 0x1b82b8, 0x1b84c8, 0x1b8400, 0x1b85e8, 0x1b8658, 0x37bba4, 0x37bba8 | queued use, its frame, effect 0, item effect, apply, times |
| 0x1a11a0, 0x1a1fe0, 0x1a12a0 | use on target or all, group items, the effect |
| 0x19fe48, 0x1a0140, 0x1a0100, 0x176c98, 0x186238, 0x1a0210, 0x186368, 0x1a0180, 0x1a0540, 0x1a05a8, 0x1a0290 | status record, HP add (record / player), MP add, MP charge feed and end, drive gauge |
| 0x1877c8, 0x26c698 | refill reserved slots, pause-menu battle item writer |

## Gaps

- Who calls the rebuild method (`+0xc`, `FUN_001bc2b8`) each frame (searched: `mips_scan --jal 0x1bc2b8` and `--addr
  0x1bc2b8`, no hit; only the vtable at 0x35f26c and `FUN_001bb298` through it).
- `FUN_001b3e38` (the Limit command's status), `FUN_001c7a38` / `FUN_001c7bc0` (Revert), `FUN_0019abb0` (whether the
  player accepts a command), `FUN_0017ad50` (the request): not decoded; the app takes Limit as 5 and the others as
  passing.
- The target record the deck's commands get (`FUN_001bc0c8` copies the lock-on controller's record `+0xb40`); the app
  casts on its own targets as before (magic.rs).
- The meaning of the story flags 0x817, 0x821, 0x849, 0x142b, 0x2001, 0x2018 (the app has no story flag block).
- The item counts on the HUD bars (groups 88..97) are not drawn (docs/systems/ui/contract.md).
- The motion request 9's air variant and `FUN_0017e378(ctrl, 0, 0)` after it are not decoded.
- When the battle item slots refill from the stock in the field (`FUN_001a1e68`'s caller).

## WP-PL3: items on a down member

- MP of an Elixir. `FUN_001a12a0` reaches the MP through `FUN_001a0210`, which for a block with an object at `+0x24c` (every
  party member and Sora) calls `FUN_00186368(object, amount, flag)`; with no object it calls `FUN_001a0180` itself. `FUN_00186368`
  answers 0 and changes nothing while the object's `+0x7f8` bit 4 (KO) is set; otherwise it calls `FUN_001a0180(block, amount)`:
  `+0x180` plus the amount held between 0 and `+0x184`, or, while the MP charge runs (`FUN_001a05d0`: `+0x1c0` above 0), the charge
  is shortened by `amount * sstm +0x7c` (`FUN_001a0540`). So an Ether (3, 6) does nothing to a KO'd member, while the items that
  change the HP first (4, 7, 0x83: the `FUN_001a0140` call precedes `FUN_001a0210` in `FUN_001a12a0`) revive him through the HP
  method and the MP lands in the same use. Port: `Vitals::is_ko` (HP 0 at a maximum above 0 stands for the KO bit, which the HP method
  sets exactly then [inferred]) and `add_mp_unless_ko` in `item_effect`; both the pause menu (`menu_items::use_on_member`) and the
  field use take it. Test `mp_items_leave_a_ko_member_alone_unless_the_hp_revives_him`.
- The field use (`items::use_on_members`) no longer skips a KO'd member: the HP gain goes through `party_battle::hp_change` with the
  revive scale and `ko` is written back to the member (the party tick then gets him up). Test
  `field_items_on_a_ko_member_follow_the_hp_method`.
- `FUN_0017cee8` flag-8 branch. After clearing the KO bit it takes the ability block (`FUN_001768a8` = status block `+0x1cc`) word
  `+0x10` and tests mask 8 (ability bit 131): set, the scale is the float at `0x37a560` (1.0 in the executable); clear, `sstm +0x54`.
  Bit 131 is ability 419 (test `hyper_healing_is_ability_419_bit_131`), the ability `FUN_001c0f40` handles as case 0x1a3 (it
  multiplies the block's `+0x58` by `sstm +0x154`, the revive time `party_battle::revive_time` already takes), so the name is Hyper
  Healing [inferred from the shared id; no text read]. The block is built from the equipped abilities of the member's record
  (`FUN_0019f920` -> `FUN_001a0c08` / `FUN_001c0e50` -> `FUN_001c0f40`; `0x8000` marks equipped). Port:
  `party_rules::revive_scale(abilities, full, sstm)` and `revive_full_scale`; the field use applies it (`ItemsData::revive_scale`).
  Test `revive_scale_takes_the_full_word_with_hyper_healing`.
- Drive Recovery (0x112, 0x113) on a down member: no case in the original (WP-PL2: `FUN_001a15a0` compares the gauge bytes
  `+0x1b1 < +0x1b2`, the item has the single Sora row `FUN_0025c2c8`); unchanged.

Gaps. (1) The pause menu (`menu_items.rs`) and the party's own item use (`party.rs` `party_item_uses` / `change_hp`) still pass
`sstm +0x54` as the revive scale and do not look at the member's equipped Hyper Healing; they need `party_rules::revive_scale` with
the member's record list (`Progress::member(neo).abilities`). Their `Progress` is in reach in the menu, not in `change_hp`.
(2) `FUN_00186368` with the flag set, a positive amount and the object's `+0x160` bit 0x4000 clear calls the object's vtable `+0x54`
with (0x14, 0) before the MP change (the voice method, `FUN_00175d18`, as the KO voices); not ported. Searched: `FUN_00186368`
callers (`FUN_001a0210`, `FUN_00176c98`), `FUN_00176c98`'s own call of it for a damage (the MP Rage gain, party.rs).
