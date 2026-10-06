# Footsteps (frame trigger 2)

What the game's main executable (`SLUS_210.05`, USA) does when a motion's frame trigger 2 fires,
and how kh2-rs ports it. Addresses are function addresses. Status words: **verified** = read
directly in the decompilation or the disassembly (and, where noted, checked against the disc);
**inferred** = follows from it but a name or meaning is a guess; **unknown** = not found.

## The rule (verified)

Frame trigger 2 is handled by the frame-trigger dispatcher at 0x188498 (jump table 0x37aa78, case
2 at 0x1885a4). Its body is `FUN_0016baa0(actor, *(u16 *)(trigger + 4))`: the trigger's first
parameter is the `kind`, with no other condition. `FUN_0016baa0` plays SE number

    bank * 1000 + material * 16 + variant

through FUN_002c32b8 (position = actor +0x70, or +0x540 when FUN_001beb00 is 0). In the SE
number, `bank = number / 1000` selects `se/seNNN.seb` and `number % 1000` is the entry in it.

- **material** is FUN_00183730(actor): 0x12 when bit 0x20 is set in the word at
  `*(actor + 0xc) + 8`, otherwise the int at `actor + 0x648` (the ground material, below). A
  material of 0 plays nothing, and so does a missing soundinfo record.
- **bank** is a short of the area's soundinfo record, chosen by the actor's `NeoStatus`
  (`objentry` +0x4c, read by FUN_001699a8): 1 or 0xe uses the short at record +0xe, 2 uses +0x10,
  3 uses +0x12, 4 uses +0x16, any other value below 0x10 uses +0x14, 0x10 and above uses +0x16.
- **variant** comes from `kind`: kind 0 or 4 increments the byte at `actor + 0x7fc` and uses
  `kind + (counter & 3)`; kind 0xd increments it and uses `0xd + (counter & 1)`; any other kind is
  used as is. So run steps (kind 0) cycle through entries 0..3 and walk steps (kind 4) through
  4..7 of the material's block of 16, sharing one counter per actor.

The only other caller of FUN_0016baa0 is FUN_0017af10 (kind 8, on the landing path: a check on
actor +0x588 bit 0x80 (party member), +0x164 bit 0 and +0x154 (motion slot); inferred to be the
landing thud, played without a movement command). FUN_001b1c90
(the footstep dust effect, trigger 19) also indexes a byte table by FUN_00183730's material.

### Which `kind` the motions pass (verified from the disc)

`kh2 triggers obj/P_EX100.mset 2` lists every trigger 2 of Sora's motion set. The kinds in use
are 0, 4, 9, 10 and 11:

| motion | steps (frame, kind) |
|---|---|
| A001 (walk, slots 4, 6, 7, 558) | 26/62 or 16/48, kind 4 |
| A002 (run, slots 8, 10, 11) | 8/24 or 12/32, kind 0 |
| A003 (landing) | frame 2, kind 9 |
| A005, A211, A214 | frame 2, kind 10 |
| A201, A203 (get-up) | kind 0 and kind 4 |
| A300..A317, A320, A321, A322 (attacks, guard) | kinds 0, 4, 9, 10 |
| B003 | frame 1, kind 11 |

So walk is kind 4, run is kind 0, and landing and the shuffle sounds use the fixed kinds 8 to 11.
Shadow (`M_EX020`) and Soldier (`M_EX010`) have no trigger 2 at all.

## Which soundinfo record applies (verified)

`12soundinfo.bar` is a BAR of 19 entries of type 2, named by world code (zz es tt di hb bb he al
mu po lk lm dc wi nm wm ca tr eh), loaded by FUN_001ad100 into a table at 0x01cb8240. Each entry
is a u32 count followed by `count` records of 24 bytes. FUN_001ad170 runs on area load with the
game's world byte (0x33e830) and area byte (0x33e831): the record is `world_entry + 4 + area * 24`,
or none when `area >= count`. The record holds shorts at +0xe, +0x10, +0x12, +0x14, +0x16 (the bank
numbers above); the first 0xe bytes are not used by the footstep code (meaning unknown). Inferred:
world and area bytes are the digits of the area name (`tt02` is world `tt`, area 2), the same
convention as the `arif` lookup in `audio.rs`.

Read from the USA disc: `tt02` has banks 697, 698, 699, -1 and 729, so Sora (NeoStatus 1) uses
bank 697 there. `tt00` uses 694 for Sora. Other tt areas use the 688, 691, 694 or 697 triples, and
the last short is 729 everywhere in `tt`. A short of -1 is "no bank": inferred to give a negative
SE number that no bank answers, so such an actor is silent.

## Ground material (verified; the old hypothesis is refuted)

The material of the ground is **byte 0 of the 16-byte COCT polygon record** (the table at COCT
header +0x28, the same table `kh2-map` reads vertex indices from at +2..+8). It is not
`(flags >> 8) & 0x1f`; on `tt02` that expression is 3 on every floor and 0 or 1 on walls, while
byte 0 is 1, 3, 7, 16 or 17 on floors.

The chain, all in the main executable:

1. The per-tick actor update FUN_00182690 calls FUN_00143560 with the collision probe at
   `actor + 0x590` and the ground-contact block at `actor + 0x620`
   (FUN_00143438 does the same for a probe it builds itself).
2. The collision query stores the polygon it chose into the probe: FUN_00141910 writes the
   polygon index to `probe + 0x60`, that is `actor + 0x5f0` (a u16, 0xffff = none).
3. FUN_00143560 clears `actor + 0x648` and, when the index is not 0xffff, reads the polygon record
   at `collision + 0xc + index * 0x10` and stores its first byte at `actor + 0x648`. With a moving
   collision object (`probe + 0x78`, actor +0x608) it reads that object's own polygon table
   instead. (The same function also fills `actor + 0x640`, a second collision's colour byte, and
   `actor + 0x64c`.)

So the material is refreshed every tick from the polygon in contact and is 0 when there is none.
Inferred: the polygon in `probe + 0x60` is the floor the actor stands on (the query that fills it
is the actor's ground sweep; its filter masks were not decoded). The other writers of `+0x648`
that turn up in a search (FUN_001175a0, FUN_00118730, FUN_0014fa70, 0x2ae57c) belong to unrelated
classes (the camera and a menu object).

Evidence that byte 0 is the material: in `tt02` every polygon Sora can stand on (flag bit 0) has a
non-zero byte 0 (1, 3, 7, 16 or 17), and `se/se697.seb` has samples exactly for the blocks of
materials 1, 3, 7, 8, 10, 11, 16 and 17 (entries `material * 16 ..`), and has 284 entries, which
is 17 * 16 + 12. All the 500 polygons with byte 0 equal to 0 are walls and ceilings.

Not found: what sets bit 0x20 of `*(actor + 0xc) + 8` (it forces material 0x12 for that actor;
inferred to mark actors that do not stand on terrain). Sora does not have it set as far as the
footstep logic is concerned (not verified in the descriptor itself).

## The port

- `kh2-map`: `Polygon::material` is byte 0 of the polygon record. `kh2 coct <map>` prints a count
  per (material, flags >> 8 & 0x1f) pair.
- `kh2-audio::footstep`: `WorldSoundInfo`/`SoundInfoRecord` parse an entry of `12soundinfo.bar`
  and pick the bank by NeoStatus; `footstep_number` is FUN_0016baa0 (material 0 and missing bank
  are silent, the counter rules above).
- `kh2-app` (`audio.rs`, `footsteps` system in `Sim::Sounds`, and `World`'s `Ground::material`):
  for Sora's body, every trigger 2 crossed in a tick resolves to an SE number and goes out as a
  `PlaySe`. Sora's material is the material of the floor polygon in `motor.ground` (0 in the air).
  `KH2_SOUNDLOG=1` prints one `footstep` line per step with the kind, material, NeoStatus, SE
  number and the sample (WD id) it resolves to, and then the usual `se bank:index` line.

### A sound driver fix the footsteps needed (verified in LIBSSL.IRX)

Nearly every footstep entry starts with commands 0x78 and 0x79 (for example `78 00 08`, `78 01 14`,
`78 04 09`, later `79 00`). The SSL driver reimplementation did not know them, read their
arguments as commands (one of them was a tempo change to 0) and so most walk and run entries
rendered silent (peak 0.000 for entries 16 to 20 of `se697`). The handlers are at 0x10a66c (0x78:
two bytes, slot index and value, a value of 0 reads as 256) and 0x10a6e0 (0x79: one byte, slot
index, clears the slot) in the IOP module. They set per-track random ranges that note-on applies
through FUN_001091d8 (slot 0: pitch offset, `rand(R * 0x180000) - R * 0xc0000` in the 8.24 pitch
sum), FUN_00109228 (slot 1: volume factor `0x3fff - rand(V * 0x40 - 1)`), FUN_00109270 (slot 2:
pan offset `(rand(2P) - P) * 0x40`) and FUN_001092bc (slot 4: random addition to the attack rate
bits of ADSR1). `kh2-audio` (`ssl.rs`) now implements slots 0 to 2 and consumes the arguments of
the others: slot 3 (random delay time), slot 4 and slot 5 (random program) are not applied. After
the fix every entry of `se697` that the footsteps reach renders audible (peaks 0.10 to 0.29).
The fix also applies to BGM sequences that use the commands; no BGM was compared before and after.

Deviations and gaps:

- The floor under Sora is the highest floor polygon below his feet (`World::ground`), not the
  game's last collision-sweep polygon. They agree on ordinary terrain.
- Position and distance: the game passes the actor's position to the sound engine, which derives
  volume and pan against the camera. Footsteps are now placed at Sora's feet with that formula
  (`docs/audio.md`); the mix is recomputed every tick against the moving camera while the
  step plays (`audio::update_mixes`), the source position stays where the step started.
- Only Sora's footsteps are produced. The landing path of FUN_0017af10 (kind 8, with its
  conditions) is not ported (the WP-5D search of its conditions is in `docs/audio.md`, "Kind 8
  landing": it is the branch for an actor with no movement command, party flag, motion slot 1
  or 2, and the one-shot gate was not found); the footstep dust effect (trigger 19, FUN_001b1c90) is not ported.
- The `NeoStatus` 0x12 material override and the soundinfo bytes before +0xe are not understood.
- Nobody has listened to any of this. The rendered wav only shows that the entry has a sample and
  that it is not silent.
- `World::ground` accepts every floor polygon, whatever its flags. In `tt04` and `tt05` Sora's
  spawn floor has material 0, so he is silent there (logged: `material 0 ... -> None`); most
  floor polygons of those maps carry flags 0x82 (no `PARTY_STAND` bit) and material 0, and the
  polygons with bit 0 (`0x3f1`) carry the materials 1, 7 and 17. Whether the original lets Sora
  stand on 0x82 floors was not checked. `tt00` and `tt01` have material 0 everywhere (tt00 is a
  16-polygon test room).

## Checks

- Tests (`cargo test --workspace` with `KH2_ISO`): `audio::tests::soundinfo_records_resolve`
  (tt02 is 697/698/699/-1/729, Sora's NeoStatus 1 picks 697; tt00 picks 694, so the same step is
  694020 there and 697020 in tt02), `audio::tests::floors_pick_different_sounds_and_all_have_samples`
  (tt02 and tt04: at least two distinct non-zero floor materials, and each has a walk sample in
  the area's bank), and the synthetic tests in `footstep.rs` and `ssl.rs`.
- `kh2 coct map/us/tt02.map` lists material against flags: floors (0x3f1) carry 1, 3, 7, 16, 17.
- Walking in `tt02` (`KH2_SOUNDLOG=1 KH2_WALK=1 KH2_YAW=1.57 cargo run --release -p kh2-app -- tt02`),
  run steps (A002, kind 0) cycle the variant with the counter and the number changes when the
  floor changes:

      footstep A002 frame 12 kind 0 material 1 neo 1 -> Some(697016) wd 627 tracks 1
      footstep A002 frame 32 kind 0 material 1 neo 1 -> Some(697017) wd 627 tracks 1
      footstep A002 frame 12 kind 0 material 1 neo 1 -> Some(697018) wd 627 tracks 1
      footstep A002 frame 32 kind 0 material 1 neo 1 -> Some(697019) wd 627 tracks 1
      footstep A002 frame 12 kind 0 material 3 neo 1 -> Some(697048) wd 627 tracks 1
      footstep A002 frame 32 kind 0 material 3 neo 1 -> Some(697049) wd 627 tracks 1

  In `tt03` (a second record: bank 688 for Sora) the same walk gives 688016, 688017, 688018.
  Each is followed by the usual `se 697:16 (-) playing` line.
- Rendering: `kh2 se se/se697.seb 52 1.5` writes `local/se697_52.wav` (material 3, run step 0;
  peak 0.103). Nobody listened to it.
