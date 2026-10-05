# Footsteps (frame trigger 2): findings so far

Work stopped early (shift change). No code was written; this file records what was read in the
game's main executable (`SLUS_210.05`, USA) so a later worker can continue. Addresses are function
addresses. Status words: **verified** = read directly in the decompilation; **inferred** = follows
from it but a name or meaning is a guess; **unknown** = not found.

## Sound number (verified)

FUN_0016baa0(actor, kind) plays SE number `bank * 1000 + material * 16 + variant` through
FUN_002c32b8 (position = actor +0x70, or +0x540 when FUN_001beb00 is 0).

- `material` is FUN_00183730(actor): 0x12 when bit 0x20 is set in the word at `*(actor + 0xc) + 8`,
  otherwise the int at `actor + 0x648`. If the material is 0, or there is no soundinfo record, the
  function returns without playing. So material 0 is silent.
- `bank` is a short from the current soundinfo record, chosen by the actor's `NeoStatus`
  (`objentry` +0x4c, read by FUN_001699a8): 1 or 0xe uses record +0xe, 2 uses +0x10, 3 uses +0x12,
  4 uses +0x16, any other value below 0x10 uses +0x14, 0x10 and above uses +0x16. A value of -1 in
  the record is stored for "none" (see tt below); what the game does with it was not checked.
- `variant` comes from `kind`: kind 0 or 4 increments the byte at `actor + 0x7fc` and uses
  `kind + (counter & 3)`; kind 0xd increments it and uses `0xd + (counter & 1)`; any other kind is
  used as is. So the variant alternates or cycles with a per-actor counter, not a random draw.

## Which soundinfo record applies (verified)

`12soundinfo.bar` is a BAR of 19 entries of type 2, named by world code (zz es tt di hb bb he al
mu po lk lm dc wi nm wm ca tr eh), loaded by FUN_001ad100 into a table at 0x01cb8240. Each entry
is a u32 count followed by `count` records of 24 bytes (size checks out for every world).
FUN_001ad170 runs on area load (called from FUN_001689c0) with the game's world byte (0x33e830) and
area byte (0x33e831): the record is `world_entry + 4 + area * 24`, or none when `area >= count`.
The record holds shorts at +0xe, +0x10, +0x12, +0x14, +0x16 (the bank numbers above); the first
0xe bytes are not used by the footstep code (meaning unknown).

Read from the USA disc for `tt` (41 records): +0xe/+0x10/+0x12/+0x14/+0x16 for area 2 (tt02) are
697, 698, 699, none, 729, so Sora (NeoStatus 1) uses bank 697 there. Other tt areas use 688, 691,
694 or 697 triples (688..690, 691..693, 694..696, 697..699), and the last short is 729 everywhere.
Inferred: the world and area bytes correspond to the area name digits (`tt02` is world tt, area 2),
the same convention as the `arif` lookup in `audio.rs`.

## Ground material (hypothesis not decided)

The hypothesis `material = (polygon flags >> 8) & 0x1f` is neither confirmed nor refuted. The
material is an int at actor +0x648, and the writer of that field for the actor class was not
found: the other functions in the decompilation that store to +0x648 (FUN_001175a0, FUN_00118730,
FUN_0014fa70) belong to different classes and do not set a ground material. The 0x12 fallback when
`*(actor + 0xc) + 8` has bit 0x20 is also unexplained (inferred: a "no ground / not touching
terrain" state).

## Callers and `kind` (mostly unknown)

The only caller of FUN_0016baa0 in the decompilation is FUN_0017af10, which passes kind 8 on a
landing path (inferred: landing). The frame-trigger dispatcher at 0x188498 (jump table 0x37aa78)
is not in the decompilation; the path from trigger 2's parameter to this function was not read, so
which `kind` values walk and run use is unknown.

## Next steps

1. Disassemble trigger 2 in the dispatcher at 0x188498 to get the `kind` it passes.
2. Find the writer of actor +0x648 (disassemble, since the decompiler skipped it) and the sampling
   of the COCT polygon under the actor's feet; compare with the flags hypothesis.
3. Implement the lookup (bank from `12soundinfo.bar`, material, variant) in `kh2-audio`, and a
   `footsteps` system in `kh2-app/src/audio.rs`; verify with `KH2_SOUNDLOG` on tt02 and one other
   area, and render one resolved SE with `kh2 se`. Nobody has listened to any of this.
