# Core runtime: the memory card unit

Ladder step L1, spec WP-L1-S2 (2026-10-23). Ledger note `L1:memcard` (129 rows in `sheets/functions.json`, system `core.save`,
`0x230e90..0x2331b0` and `0x23c830..0x23e598`). Everything below is read from the decomp of the USA executable unless marked;
field names are ours. The port target is WP-L1-P9 (`crates/kh2-save`, new crate): read the user's PCSX2 cards into `Progress` and
write saves the original loads. The screens that drive this unit are in `docs/systems/ui/menus-save.md`.

The unit has two layers:
- the **card driver** (`0x230e90..0x2331b0`): a 64-entry request ring, a per-frame state machine per request type over the IOP
  memory-card library (`FUN_00303*`, `FUN_00304*`, outside this unit), result words and a per-port status table;
- the **save file layer** (`0x23c830..0x23e598`): buffers, the save directory and file names, `icon.sys`, the slot table of 99
  files, the CRC and the save body checks.

## 1. Card driver (request ring)

State words: ring write index `DAT_0036c258`, read index `DAT_0036c25c` (both `& 0x3f`), request id counter `DAT_0036c260`, id of
the running request `DAT_0036c264`, running type `DAT_0036c268` (0 = idle), sub-step `DAT_0036c26c`. Ring entries are 0x20 bytes at
`0x1dc0348`: +0 type, +4 id, +8 port, +0xc slot, +0x10..+0x1c arguments (meaning per type). Every enqueue function returns the
new id (the counter before increment).

| type | enqueue (port, slot = 0 wrapper) | per-frame handler | what it does (library call) | result word |
|---|---|---|---|---|
| 1 | `FUN_002310c8` (`FUN_002315a0`) | `FUN_00231f90` | card info poll (`FUN_00303db0`), up to 3 tries | status table |
| 2 | `FUN_00231120` (`FUN_002315c0`) | `FUN_00232228` | directory listing (`FUN_00303fa0`); count into status +0x1c | `0x1dc0b84` |
| 3 | `FUN_002311b8` (`FUN_002315f0`) | `FUN_002323c8` | directory listing, other result mapping | `0x1dc0b88` |
| 4 | `FUN_00231218` (`FUN_00231620`) | `FUN_00232558` | format (`FUN_003042c0`, RPC 0x10), retry 60 frames | `0x1dc0b8c` |
| 5 | `FUN_00231270` (`FUN_00231640`) | `FUN_00232668` | unformat (`FUN_00304898`, RPC 0x11), retry 60 frames | `0x1dc0b90` |
| 6 | `FUN_002312c8` (`FUN_00231660`) | `FUN_00232778` | change directory (`FUN_00304180`) | `0x1dc0b94` |
| 7 | `FUN_00231320` (`FUN_00231680`) | `FUN_00232880` | make directory (`FUN_00303778`) | `0x1dc0b98` |
| 8 | `FUN_00231378` (`FUN_002316a0`) | `FUN_00232988` | open with mode 0x203 (create, read-write) then write, close | `0x1dc0b9c` |
| 9 | `FUN_002313d8` / `FUN_002314f0` (`FUN_002316d0`, `FUN_00231750`) | `FUN_00232b70` | delete (`FUN_00304390`) | `0x1dc0ba0` |
| 10 | `FUN_00231430` (`FUN_002316f0`) | `FUN_00232c80` | open mode 1, read (`FUN_00303a00`), close | `0x1dc0ba4` |
| 11 | `FUN_00231490` (`FUN_00231720`) | `FUN_00232e58` | open mode 2, write (`FUN_00303b18`), close | `0x1dc0ba8` |
| 12 | (none in this range) | `FUN_00233040` | set file info (`FUN_00304740`) | `0x1dc0bac` |
| 13 | `FUN_00231550` | `FUN_00233140` | call a function (the request's argument) in the queue's order | `0x1dc0bb0` (1 when it returned 0) |

- Dispatch: `FUN_00233178` calls the handler table `0x36c278[DAT_0036c268]` (14 entries, entry 0 = `FUN_00231a60`). The game's
  per-frame call site was not traced [gap]; `FUN_00230f48` (wait for a request id, or for all when 0) loops `FUN_00233178` plus
  `FUN_002f8ec8(0)` (a vsync wait [inferred]).
- `FUN_00231a60` (type 0, idle): when the ring is empty and polling is on (byte `0x1dc0b68`), counts down `0x1dc0b70` from the
  reload `0x1dc0b6c` and then starts a type-1 poll itself on port `0x1dc0b74`, slot `0x1dc0b78`, cycling slots up to the port's
  slot count (`0x1dc0cd0 + port * 4`, from `FUN_00303590`) and the two ports. When the ring is not empty it pops the next entry,
  copies its arguments into the running words (`0x1dc0b50..0x1dc0b64`, port `0x1dc0bc0`, slot `0x1dc0bc4`) and sets the type.
- `FUN_00231a18` ends a request: type 0, reloads the poll countdown and calls the completion function `0x1dc0b64` (when non-zero)
  with the id.
- `FUN_00230e90`: true when no request runs. `FUN_00230ea0(id)`: 0 = idle and empty (id 0) or the id is neither queued nor
  running; 1 = busy.
- Each handler: step 0 issues the library call, step 1 polls completion (`FUN_00303c90(1, 0, &0x1dc0bb8)`, non-blocking), step 2
  maps the library result in `0x1dc0bb8` into the type's result word and ends. Types 8, 10 and 11 run open (steps 0..2, file
  descriptor kept in `0x1dc0bbc`), read or write (3..5, length `0x1dc0b5c`, buffer `0x1dc0b58`; a short count is an error), close
  (6, 7).
- Result getters: `FUN_00231778` (`0x1dc0b80`, the info "changed" flag), `FUN_00231788`, `FUN_00231798`, `FUN_002317a8`,
  `FUN_002317b8`, `FUN_002317c8`, `FUN_002317d8`, `FUN_002317e8`, `FUN_002317f8`, `FUN_00231808`, `FUN_00231818`, `FUN_00231828`
  return `0x1dc0b84..0x1dc0bac` in order (types 2..12).

### Status table (per port and slot)
`0x1dc0bc8 + (port * 4 + slot) * 0x20`: +0 card type, +4 and +8 the info call's two other outputs (format word and free space
[inferred from the library's argument order]), +0xc connection status, +0x10 a "changed" latch, +0x14 a "new or unformatted card"
latch, +0x18 a "card gone" latch, +0x1c file count of the last listing (-1 on error).
Getters `FUN_00231878` (+0), `FUN_00231898` (+4), `FUN_002318b8` (+8), `FUN_002318d8` (+0xc), `FUN_002319f8` (+0x1c);
`FUN_002318f8` forces +0xc to 3.

Connection status written by `FUN_00231f90` from the info result: card type 3 -> 3; result -1 -> 1 (a new card: +0x10 and +0x14 set,
`0x1dc0b80 = 0`); -2 -> 2 (unformatted, same latches); 0 -> 0 (same card, present); any other -> 3 (no card; on the first such result
+0x18 and +0x10 set and `0x1dc0b80 = 1`). Results below -2 retry while the try counter `0x1dc0b48` (3 for a queued poll, 0 for the
idle poll) lasts. Before the call it sets `0x1dc0cc8[0..1] = 1` and fetches both ports' slot counts.

### Result codes (library result -> result word)
Library meanings are [inferred] from the public IOP memory-card library documentation (0 ok, -1 changed card, -2 unformatted,
-3 full or no space, -4 not found, -5 access denied, -7 too many open files, -8 not a file); the mapping is read.

| handler | 0 | -2 | -3 | -4 | -5 | -6 | -7 | -8 | other negative |
|---|---|---|---|---|---|---|---|---|---|
| list A `FUN_00232228` | 0 (count stored) | 3 | 2 | 1 | 2 | 2 | 2 | 2 | 2 |
| list B `FUN_002323c8` | 0 | 4 | 3 | 2 | 3 | 3 | 3 | 3 | 3 |
| format / unformat | 0 | (any non-zero: retry, then 1) | | | | | | | |
| chdir `FUN_00232778` | 0 | 4 | 2 | 1 | 2 | 3 | 2 | 2 | 2 |
| mkdir `FUN_00232880` | 0 | 4 | 1 | 2 | 3 | 3 | 3 | 3 | 3 |
| create+write `FUN_00232988` open / write | 0 | 6 / 6 | 1 / 1 | 2 / 7 | 3 / 3 | 5 / 5 | 7 / 5 | 5 / 4 | 5 / 5 |
| delete `FUN_00232b70` | 0 | 4 | 3 | 1 | 5 | 2 | 3 | 3 | 3 |
| read `FUN_00232c80` open / read | 0 | 5 / 5 | 6 / 3 | 1 / 7 | 2 / 2 | 4 / 4 | 7 / 4 | 4 / 4 | 4 / 4 |
| write `FUN_00232e58` open / write | 0 | 7 / 7 | 1 / 1 | 2 / 4 | 3 / 3 | 6 / 6 | 4 / 6 | 6 / 5 | 6 / 6 |
| set info `FUN_00233040` | 0 | 3 | 2 | 1 | 2 | 2 | 2 | 2 | 2 |

A read or write that returns a non-negative count other than the requested length maps to 3 (read), 4 (create+write) or 5
(write). The screens turn these words into the messages of `docs/systems/ui/menus-save.md` (not traced per code [gap]).

## 2. Save file layer

### Init and buffers
- `FUN_0023c830(allocator)`: stores the allocator (`DAT_0036cb08`), CRC init `FUN_0023e3d8`, buffers `FUN_0023c948`, then sets bit 8
  of save `+0x31a8` from `FUN_0023e318(save +0xc)`. Shutdown `FUN_0023c8a8` (`FUN_0023cac0` frees, `FUN_0023e410` CRC free).
  Allocator calls `FUN_0023c8d8` (vtable slot 0, alloc) and `FUN_0023c910` (slot 1, free); the CRC unit has its own pair
  `FUN_0023e450`, `FUN_0023e488`.
- `FUN_0023c948` allocates and zeroes: name buffer 0x20 (`DAT_0036cb0c`), path buffer 0x12 (`DAT_0036cb10`), the file buffer of
  `max(kb(0x8a58), kb(0xb4e0)) * 1024` = 0xb800 bytes (`DAT_0036cb14`), the card info block 0xc68 (`DAT_0036cb18`; byte +0 = 0xff,
  u16 +2 = 0xffff), the file list 0xf80 (`DAT_0036cb1c`), the directory table 0x1900 (`DAT_0036cb24`, its 64-byte aligned start in
  `DAT_0036cb20`). `FUN_0023cd38(n)` = `(n + 0x3ff) >> 10` (bytes to KB, rounded up). `FUN_0023dd50` returns the file buffer,
  `FUN_0023cd28` the name buffer, `FUN_0023e350` the card info block.

### Names
- `FUN_0023cc98(n)` puts a name into the name buffer: -5 keeps it; -4 = `*`; -3 = `icon.sys`; -2 = `kh2.ico`; otherwise
  `FUN_0023cb90` formats the save directory name: `<region prefix><product code part 1>-<part 2><part 3>-<NN>` with `NN` = `n` as
  two digits, or `??` when `n < 0` (the listing pattern). The region prefix (`FUN_0023cc38`, region word `DAT_00349d44` via
  `FUN_00105cd8`): region 1 `BI`, region 2 `BA`, else `BE`; the product code parts are three strings at `0x349d5c + region * 12`
  (`FUN_00105d10`). For the USA disc this gives `BASLUS-21005NN`-style names of 15 characters [inferred: the parts were not
  printed from the region table; the slot parser below requires length 15 with the digits at index 13 and 14].
- Directory path: `FUN_0023dad8(mode)` (change directory) builds `/` in the path buffer, and when `mode == 0` appends the name buffer
  (`FUN_00311c78`); `FUN_0023db48` makes the directory `/<name>`. So a save is the directory `<name>` holding three files.

### The three files of a save (sizes are rules: `FUN_0023dd60(kind)`: 0 -> 0x3c4, 1 -> 0x8a58, else 0xb4e0; `FUN_0023e360` table)
| file | size | contents |
|---|---|---|
| `icon.sys` | 0x3c4 | `FUN_0023cd48`: the PS2 browser header (below) |
| `kh2.ico` | 0x8a58 | the 3D icon, copied from the disc or a resource [gap: the writer of the icon bytes was not traced] |
| `<name>` (same as the directory) | 0xb4e0 | the save body: a copy of the save block `0x33e860` |

`FUN_0023d120`: the space a new save needs, `((kb(0x3c4) + kb(0x8a58) + kb(0xb4e0) + 13) / 10) * 10` = 90 KB.

### icon.sys (`FUN_0023cd48(buf or 0, n)`)
Zeroes 0x3c4 bytes; +0 the 4-byte magic `PS2D`; u16 +6 = 0x20 (the title's second-line offset); u32 +0xc = 0x60 (background
transparency); +0x10 the background colours (0x40 bytes from `0x37f590`), +0x50 light directions (0x30 from `0x37f5d0`), +0x80 light
colours (0x30 from `0x37f600`), +0xb0 ambient (0x10 from `0x37f630`) (read at run time from the executable, never copied);
+0xc0 the title (0x44 bytes, `FUN_0023ce40(+0xc0, n + 1)`); +0x104, +0x144, +0x184 the icon file names, all `kh2.ico`. Layout of
the standard browser header [inferred, public PS2 icon.sys descriptions].

Title (`FUN_0023ce40`): two Shift-JIS lines from `0x36caf0` (game name) and `0x36caf4` (the template with the file number,
LV and play time); the line-2 digits are made by adding a digit to the full-width zero's second byte: the file number `n + 1` at
+3/+5, Sora's level (party member 1, or 0xe when bit 0 of save `+0x165c` is set, `FUN_00187d40`) at +0xd/+0xf, hours (to 3 digits,
leading zeros blanked with the full-width space `0x36cb04`) at +0x13/+0x15/+0x17, minutes at +0x1b/+0x1d (play time from
`FUN_0014c950`); at line +0x3e a 2-byte mark by game level (`FUN_00105750`: table `0x36caf8`, levels 1 and 2 have the two star
marks, 0 and 3 a space).

### Save body header and CRC
- Header of the 0xb4e0 body (`FUN_0023d1c8(buf)`): +0 the magic `FUN_00105758()` = `region_char << 24 | 0x32484b` (bytes `KH2` and
  `J` / `U` / `E` for regions 1 / 2 / other), +4 the version 0x2d, +8 the CRC. Valid means all three match.
- CRC (`FUN_0023e3d8` init: allocator `DAT_0036cb38`, a 0x400 table `DAT_0036cb3c`, `FUN_0023e4c0`, `FUN_0023e528`):
  - Table (`FUN_0023e4c0`): for each byte value `i`, `c = i << 24`, then **255** rounds of `c = (c << 1) ^ (c & 0x80000000 ? 0x04c11db7 : 0)`
    (the round limit is `sltiu ..., 0xff` at `0x23e4f0`, not 8; the table is therefore not the common CRC-32 table. The oracle
    must confirm; never substitute a library CRC).
  - Reset `FUN_0023e528`: state `DAT_0036cb40 = 0xffffffff`. Update `FUN_0023e538(p, n)`: per byte
    `s = (s << 8) ^ table[(s >> 24) ^ b]`; returns `~s` (the state itself is not inverted, so calls chain).
  - Body CRC `FUN_0023d180(buf)`: reset, update over bytes 0..8, continue over bytes 0xc..0xb4e0 (0xb4d4 bytes); the CRC field +8 is
    skipped.
- Writing (`FUN_0023d230(buf or 0)`): copy the live save block (0xb4e0 from `FUN_001057a8`) into the buffer, byte +0x31a8 = a random
  pick of `FUN_0023e258`, bit 8 of the word +0x31a8 = `FUN_0023e318(world byte +0xc)`, then +8 = body CRC.
  - `FUN_0023e258`: over the 8 flag pairs at `0x37f640` (u16 a, u16 b), entry `i` is a candidate when (a == 0 or flag a set) and
    flag b set, or when a != 0 and flag a clear [as read: the else branch makes it a candidate]; returns
    `candidates[FUN_0014c910() % count]` (save `+0x1604` as the seed; a trap when there are none). Flag check `FUN_001802e0`.
  - `FUN_0023e318(world)`: 1 when world == 4 and story flag 0x108b is set [inferred: the alternate world name on the slot row].
- Loading (`FUN_0023d2b8(buf or 0)`): copy 0xb4e0 bytes from the buffer back over the save block. `FUN_0023dcf8(p)`: true when the
  CRC of 0x8a58 bytes at `p` equals that of the file buffer (an icon compare [inferred]).
- `FUN_0023e598(v)` sets the save block's munny word (a neighbour, ledger-filed here).

### Card info block (`DAT_0036cb18`, 0xc68 bytes)
+0 card port (`FUN_0023d2f8` set, `FUN_0023d308` get; the screens' row index), +1 cursor file (`FUN_0023d328`, `FUN_0023d338`),
u16 +2 free space in KB (`FUN_0023d318`), byte +4 a flag (`FUN_0023dfe8`, `FUN_0023e000`), +8 99 slot records of 0x20 bytes
(`FUN_0023d348` clears all).

Slot record (`FUN_0023d370(slot, buf or 0, bits)`, writer of everything below): +0 flag word: bit 0 a directory exists, bit 1
(bits & 2), bit 4 (0x10) a valid save is described, bit 5 (0x20) empty and writable, bit 6 (0x40) empty but no space. With bits 0x10:
+4 place id `FUN_0019e178(area +0xd, world +0xc)`, +8 save `+0x1604` (play time [inferred]), +0xc save `+0x1600` (munny [inferred]),
+0x10 save `+0x1658`, +0x11 the level (party member 1 or 0xe as for the title), +0x12 world (0xff when above 0x12), +0x13 save byte
`+0x31a8`, +0x14 bit 8 of save `+0x31a8`; +0x18 the 8-byte modify time from the directory entry (`FUN_0023d578`,
`FUN_0023de68(slot, clear)`: copy from the table's first entry or fill 0xff).
- `FUN_0023dd80(slot or -1)` = `FUN_0023d370(slot, 0, 0x13)` after a save; `FUN_0023d520` clears a record; `FUN_0023d698(slot or -1,
  bits)` ORs bits.
- Queries: `FUN_0023ddb0(slot or -1)` bit 4; `FUN_0023e060(slot)` bit 5; `FUN_0023e018` any slot with bit 4; `FUN_0023da78` the first
  slot from the cursor with bits 0 and 1 clear (-1 when none below 99).
- `FUN_0023dde0`: when the free space (+2) is known and below 90 KB, every empty record (word 0) becomes 0x40.
- `FUN_0023ded0`: the slot with bit 4 whose modify time is latest (compared year u16 +0x1e, month +0x1d, day +0x1c, hour +0x1b,
  minute +0x1a, second +0x19; ties keep the earlier slot); 0 when none. The default cursor [inferred].

### Directory scans
Directory entries are 0x40 bytes in the table at `DAT_0036cb20` (the IOP library's listing format): +8 modify time (8 bytes), +0x10
size, u16 +0x14 attributes (0x20 = directory), +0x20 name (32 bytes).
- `FUN_0023d538`: list the root (`FUN_0023cc98(-1)` pattern [inferred: the argument is not visible in the decomp], `FUN_002315c0(port,
  table, 99, name)`).
- `FUN_0023d578`: after a root listing, clears the 99 records and the cursor, then for every directory entry whose name is 15
  characters with a digit at index 13: slot = `name[13] * 10 + name[14] - 0x210` (the two digits), record word = 1, modify time copied.
  Returns the entry count (`FUN_002319f8(port, 0)`).
- `FUN_0023d6c8`: after listing one save directory, fills the file list `DAT_0036cb1c`: +0 count, +4 current, entries of 0x28 from +8
  (+0 kind 1 directory / 2 file, +4 size, +8 name), skipping `.` and `..`.
- `FUN_0023d820`: checks the list; returns bits: 1 `icon.sys`, 2 `kh2.ico`, 4 the body file, each also with 0x100 / 0x200 / 0x400 when
  its size is not 0x3c4 / 0x8a58 / 0xb4e0; 0x10 for any other file. Bad and unknown files get entry bit 0x10.
- `FUN_0023d9e0`: the first entry with bit 0x10: current = it, name buffer = its name (to delete it [inferred]); -1 when none.
  `FUN_0023d9a8` clears the current entry.
- `FUN_0023e080(exact)`: counts the three expected files (exact: size equal; else size at least) and sums their KB; exact -> all
  three; else all three or the sum is at least 90 KB [as read: the comparison is `sum < 90` -> false].

### Card operations used by the screens
`FUN_0023d538` list root; `FUN_0023dad8(0|1)` change into the save directory or root; `FUN_0023db48` make it; `FUN_0023dbb0` delete
the name; `FUN_0023dbf0` create the name empty (type 8 with length 0); `FUN_0023dc28(kind, name or 0, buf or 0)` read and
`FUN_0023dc90(kind, ..)` write `FUN_0023dd60(kind)` bytes. The order of a save and a load (which page issues which call) is in
`docs/systems/ui/menus-save.md` (kind 1 writes, kind 2 reads); the card is the port in +0 (two rows: slot 1 and 2).

`FUN_002331b0(obj)`: an object constructor in the driver's file (vtable `0x36c9c8`, clears +0x38, +0x3c, +0x194); its user was not
traced [gap].

## 3. The PCSX2 card image (`.ps2`), [inferred: not in the game, which talks to the IOP]
Source: the public PS2 memory-card file-system descriptions (as used by community card tools); to be confirmed on the user's own
card in P9 before any write.
- 8 MB card: 16384 pages of 512 data bytes + 16 spare bytes (8,650,752 bytes). Spare: 12 ECC bytes (3 per 128-byte chunk, a Hamming
  code), the rest 0xff. 2 pages per cluster, 16 pages per erase block.
- Page 0 superblock: the magic text "Sony PS2 Memory Card Format " plus a version; page length, pages per cluster, pages per block,
  clusters per card, allocation offset and end, root directory cluster, two backup blocks, the indirect FAT cluster list, the bad
  block list, card type 2, card flags.
- FAT: indirect cluster list -> FAT clusters -> 32-bit entries; bit 31 set = used, low 31 bits the next cluster, 0xffffffff end of chain.
- Directory entries are 512 bytes: mode u16 (directory, file, exists bits), length u32 (entries or bytes), created time (8), first
  cluster u32 (relative to the allocation offset), modified time (8), attributes, name at +0x40 (32 bytes). A directory starts with `.`
  and `..`.
- PCSX2 also offers folder cards (one host folder per card, a `_pcsx2_superblock` file); P9 handles the image form first [decided:
  folder cards are a follow-up].

## 4. Oracle vectors plan (for L1-P9)
Factory-runnable (pure, or state-only through globals the spec sets):
- CRC: `FUN_0023e4c0` (needs `DAT_0036cb3c` pointing at a 0x400 guarded buffer), `FUN_0023e528`, `FUN_0023e538` (random buffers and
  lengths, chained calls), `FUN_0023d180` and `FUN_0023d1c8` (random 0xb4e0 bodies, half with a correct CRC; region word `0x349d44`
  as a global), `FUN_0023cd38`, `FUN_0023d120`, `FUN_0023dd60`.
- Body copy: `FUN_0023d230` (stub `FUN_0023e258` only if its flag reads cannot be set up; otherwise set story flags as globals),
  `FUN_0023d2b8`, `FUN_0023e318`.
- icon.sys: `FUN_0023cd48` / `FUN_0023ce40` with the save block's level, game level and play time as globals (the colour tables come
  from the executable image the oracle runs).
- Slot table: `FUN_0023d370`, `FUN_0023dd80`, `FUN_0023de68`, `FUN_0023ded0`, `FUN_0023da78`, `FUN_0023dde0`, `FUN_0023ddb0`,
  `FUN_0023e060`, `FUN_0023e018`, the getters and setters; fuzz the 0xc68 block.
- Directory parsing on a **directory table built by the test** (64-byte entries, random names around the 15-character pattern,
  random sizes and attributes): `FUN_0023d578`, `FUN_0023d6c8`, `FUN_0023d820`, `FUN_0023d9e0`, `FUN_0023e080`; the count comes from
  the status table (+0x1c), a global.
- Request ring: every enqueue function and `FUN_00231a60`, `FUN_00231a18` (ring and running words as globals, completion pointer 0),
  `FUN_00230ea0`, `FUN_00230e90`.
- Handlers `FUN_00231f90..FUN_00233040` call the library (`FUN_00303*`, `FUN_00304*`): these need a scripted stub (sync returns 1 and
  writes a chosen result to `0x1dc0bb8`), which the factory's return-0 stubs cannot do; port them with hand tests over every result
  code in the table of section 1 [decided: hand tests, listed in P9's brief as not factory-verified].
- Card image: the `.ps2` reader and writer are tested on an image the test builds (format an empty 8 MB image, write one save
  directory with three files whose body is a zeroed block with a correct header and CRC). Never a real save in the repo; the rig
  scenario loads the user's own card and compares the save block.

## Decompile index
Driver: `FUN_00230e90`, `FUN_00230ea0`, `FUN_00230f48`, `FUN_002310c8`, `FUN_00231120`, `FUN_002311b8`, `FUN_00231218`, `FUN_00231270`,
`FUN_002312c8`, `FUN_00231320`, `FUN_00231378`, `FUN_002313d8`, `FUN_00231430`, `FUN_00231490`, `FUN_002314f0`, `FUN_00231550`,
`FUN_002315a0`, `FUN_002315c0`, `FUN_002315f0`, `FUN_00231620`, `FUN_00231640`, `FUN_00231660`, `FUN_00231680`, `FUN_002316a0`,
`FUN_002316d0`, `FUN_002316f0`, `FUN_00231720`, `FUN_00231750`, `FUN_00231778`, `FUN_00231788`, `FUN_00231798`, `FUN_002317a8`,
`FUN_002317b8`, `FUN_002317c8`, `FUN_002317d8`, `FUN_002317e8`, `FUN_002317f8`, `FUN_00231808`, `FUN_00231818`, `FUN_00231828`,
`FUN_00231878`, `FUN_00231898`, `FUN_002318b8`, `FUN_002318d8`, `FUN_002318f8`, `FUN_002319f8`, `FUN_00231a18`, `FUN_00231a60`,
`FUN_00231f90`, `FUN_00232228`, `FUN_002323c8`, `FUN_00232558`, `FUN_00232668`, `FUN_00232778`, `FUN_00232880`, `FUN_00232988`,
`FUN_00232b70`, `FUN_00232c80`, `FUN_00232e58`, `FUN_00233040`, `FUN_00233140`, `FUN_00233178`, `FUN_002331b0`.
File layer: `FUN_0023c830`, `FUN_0023c8a8`, `FUN_0023c8d8`, `FUN_0023c910`, `FUN_0023c948`, `FUN_0023cac0`, `FUN_0023cb90`,
`FUN_0023cc38`, `FUN_0023cc98`, `FUN_0023cd28`, `FUN_0023cd38`, `FUN_0023cd48`, `FUN_0023ce40`, `FUN_0023d120`, `FUN_0023d180`,
`FUN_0023d1c8`, `FUN_0023d230`, `FUN_0023d2b8`, `FUN_0023d2f8`, `FUN_0023d308`, `FUN_0023d318`, `FUN_0023d328`, `FUN_0023d338`,
`FUN_0023d348`, `FUN_0023d370`, `FUN_0023d520`, `FUN_0023d538`, `FUN_0023d578`, `FUN_0023d698`, `FUN_0023d6c8`, `FUN_0023d820`,
`FUN_0023d9a8`, `FUN_0023d9e0`, `FUN_0023da78`, `FUN_0023dad8`, `FUN_0023db48`, `FUN_0023dbb0`, `FUN_0023dbf0`, `FUN_0023dc28`,
`FUN_0023dc90`, `FUN_0023dcf8`, `FUN_0023dd50`, `FUN_0023dd60`, `FUN_0023dd80`, `FUN_0023ddb0`, `FUN_0023dde0`, `FUN_0023de68`,
`FUN_0023ded0`, `FUN_0023dfe8`, `FUN_0023e000`, `FUN_0023e018`, `FUN_0023e060`, `FUN_0023e080`, `FUN_0023e258`, `FUN_0023e318`,
`FUN_0023e350`, `FUN_0023e360`, `FUN_0023e3b8`, `FUN_0023e3d8`, `FUN_0023e410`, `FUN_0023e450`, `FUN_0023e488`, `FUN_0023e4c0`,
`FUN_0023e528`, `FUN_0023e538`, `FUN_0023e598`.
Callees looked at outside the unit: `FUN_00105758`, `FUN_00105cd8`, `FUN_00105d10`, `FUN_0014c910`, `FUN_001057a8`,
`FUN_00303c90`, `FUN_00303590`, `FUN_003042c0`, `FUN_00304898`, `FUN_003037b0`, `FUN_00303778`.
`FUN_0023e3b8` calls `FUN_0023e360(1, 0xffff)`, which fills the expected-file table `0x1dc0dd8` (names and sizes) used by `FUN_0023e080`.

## Gaps
- The per-frame caller of `FUN_00233178` and who turns idle polling on (`0x1dc0b68`, `0x1dc0b6c`): not searched.
- The writer of the `kh2.ico` bytes and of the type-12 enqueue (no enqueue in the range reads type 12).
- The region table strings at `0x349d5c` were not printed; the exact USA directory name is [inferred] until P9 lists the user's card.
- `FUN_00303*` / `FUN_00304*` (the IOP library client) are outside the ledger range; their result meanings are from public docs.
- Which screen state maps which result word to which message: in `menus-save.md` only for the paths already read.
