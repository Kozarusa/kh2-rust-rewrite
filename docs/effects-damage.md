# Damage-side effects, death effects and unported particle programs (WP-1F, partial)

Work stopped early on a shift change. This file records only what was actually established, and
lists what is unfinished. No code was changed for this work package.

Status words: **verified** = read directly in the game's code or data; **inferred** = follows from
it; **unknown** = not found or not looked at.

## Unported particle programs: where they are and where their handlers live

Verified (program table at 0x362b10, handler addresses read from it; `pppKeShpTail2X` is the one
already ported in `crates/kh2-fx/src/progx.rs`):

| program | table index | frame | draw | init | reset |
|---|---|---|---|---|---|
| pppKeMvYpEff | 47 | 0x1f4628 | none | none | none |
| pppKeShpTail3X | 157 | 0x1fcf70 | 0x1fd388 | 0x1fdb90 | 0x1fdbf0 |
| pppSnoMdlDttNorm | 238 | none | 0x20a7e8 | 0x20a7b8 | 0x20a7d0 |

Where the data uses them (scan of `00effect.bar` and every `obj/*.apdx` with `kh2 pax`, grouping
by file):

* pppKeShpTail3X: `00effect.bar` (system effects), most magic files (`M_EX*`), many bosses and
  `P_NM000`, `P_MU000`, `SAVE_POINT`, `W_TR000`, among others (over 100 files). The
  Shadow and Soldier files (`M_EX020`, `M_EX010`) were checked: `M_EX010.apdx` and its `_NM`, `_TR`,
  `_WI` variants use it; `M_EX020` does not appear in the scan.
* pppKeMvYpEff: `00effect.bar`, `B_EX170_LAST`, `P_EX100_*_ULTF`, `P_EX200`, `P_EX220`.
* pppSnoMdlDttNorm: `00effect.bar`, `P_EX100` variants, `P_EX120`, `AL14_*`, `G_EX900*`,
  `B_EX130/140`, `M_EX600/670/720/880/890`, `N_BB050_BTL`, `N_HB630`, `PO06-08_PLAYER`.

Not yet determined: which element ids inside those files use each program (the scan was per
file). Because `00effect.bar` uses all three, a `KH2_FXTEST=<id>` run against the system file can
exercise them once the element ids are known.

### pppKeShpTail3X, partial reading (verified from the decompilation, not yet ported)

* init (0x1fdb90): clears byte +0x29a (ring head), u16 +0x294, u32 +0x290 (a time accumulator),
  sets u16 +0x298 from a random seed function, and zeroes 0x30 bytes at +0xa0 (eight i16 channels
  at +0xa0, their rates at +0xb0 and their rates of rate at +0xc0). reset clears only the channels.
* frame (0x1fcf70): a ring of 0x1c positions (stride 0x10, starting at +0xd0) is filled with the
  current position on the first frame; the head moves backwards each tick and the new sample is
  written there (world or bind-relative depending on the byte at params +0x53, as in Tail2X). Then
  the eight i16 channels accumulate: rate of rate into rate, rate into value, and on the exact
  frame named by the parameter entry the entry's eight deltas are added to the values and rates.
  The channel values are read by the draw handler >> 7 as floats.
* draw (0x1fd388): walks the ring, placing one shape or model instance per step with a spacing
  that shrinks along the tail (like Tail2X), but with a per-instance random rotation drawn from a
  16 bit linear congruential generator (x * 0x80d + 7), an optional rotation about the view axis
  by a sine table lookup when the u16 at params +0x1e is non-zero, and a color that steps from the
  start channels toward the end channels. It submits through a draw call at 0x21a660 using VU0
  matrix math the decompiler renders poorly (broadcast fields are missing).
* Unknown: the exact meaning of each of the eight channels (the draw reads them as a color
  triple, a scale pair and an alpha, inferred), the parameter layout, and the matrix composition.
  This needs a disassembly of 0x1fd388 and ideally a run of the handler in `kh2-ee`.

pppKeMvYpEff and pppSnoMdlDttNorm: handlers located only; not read.

## Damage-side effect ids

From `docs/hit-reaction.md` (verified there): FUN_0018d090 spawns `EffectOnHit` (atkp 0xb),
`RandomEffect` (0x2d) and `DamageEffect` (0x24), and effect id 0x12 when the hit is ineffective
(hit record flag 4). Hit sparks from `EffectOnHit` are already played by `fx.rs::hit_sparks`.

Not done: target-side `DamageEffect` (applied with `| 0x200`), effect 0x12, the Heartless death
effect, and the hook points in `combat.rs` / `enemy.rs`. Nothing about which PAX file or bind
the death effect uses was established.

## Where to continue

1. Dump the three handlers with the disassembler (`dis2.py`, file offset = VA - 0xff000), port
   Tail3X first (`progx.rs` has Tail2X to copy the structure from), check against `kh2-ee`.
2. Find the element ids that use each program (extend the per-file scan in `kh2 pax` output).
3. Read FUN_0018d090 for the exact effect ids, bind targets and the `| 0x200` meaning, then add a
   `StartEffect`-style message in `fx.rs` for the hooks and report the hook points in `combat.rs`
   (where `Hit` is emitted) and `enemy.rs` (where an enemy is defeated).
4. Find the defeat path of a Heartless (look from the victim's death handling after damage
   application, FUN_00185a68 / the victim +0x78 method) for the death effect id.
