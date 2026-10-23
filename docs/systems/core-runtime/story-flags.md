# Story flags

[Contract](contract.md) | [Index](../../README.md)

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
  program 0x4d on 0x26 plays "111" then goes to 0x4e; 0xb6/0xb7/0xb8 (the Struggle bouts, docs/systems/script-host/contract.md) on 0x8b
  repeat themselves, on 0x8d go (type 1) to tt04 entrance 0x37 with world record value 1. (The event tag is the
  u32 of sub 0 read as four characters: halfwords 0x3131, 0x30 are "110".)
- **Scripts read/write** them through hosts 0.14, 0.15, 0.70 (flag 0x102b), and the party, menu and drop code
  reads them (docs/systems/party/contract.md, `memt`).

