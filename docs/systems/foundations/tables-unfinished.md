# The unfinished tables of `tables.json` (WP-TB1)

Scope: the twelve tables that `kh2 preflight` lists as unfinished (`ptya`, `patn`, `vbrt` of `00battle.bin`; `memt`, `arif`,
`went`, `wmst`, `ftst`, `rcct`, `evtp`, `ipic`, `shop` of `03system.bin`) and the three fields of finished tables whose names are
placeholders (`atkp` Unknown, `fmlv` Unk0 and Unk1, `item_stat` Unknown08, Unknown0d and Unknown). Function names are addresses in the
main executable (`SLUS_210.05`). Marks: **[V]** read in the code, **[I]** inferred, **[GAP]** not found. No table contents are
written here, only layouts and meanings. Sizes are those of the USA disc.

## 0. How the game finds the tables

`FUN_00193610` runs at boot. It opens `03system.bin` and `00battle.bin` (`FUN_001af9e0`), looks each table up by its four letter tag
and type with `FUN_00168510` (the tag strings sit at 0x37aee0..0x37afc0), and hands the entry to a one-line setter that stores the pointer
in a global [V]. Readers go through that global, so the readers of a table are the code that touches its global. The setters:

| table | setter | global | accessor (the only code that reads the global) |
|---|---|---|---|
| ptya | `FUN_001d5e38` | 0x3608f8 | `FUN_001d5e48` (table by index), `FUN_001d5e90` (index in range and used) |
| patn | `FUN_001bfef0` | 0x1d5df10 | `FUN_001bff00` |
| vbrt | `FUN_001d3050` | 0x360768, 0x1d6a988 | `FUN_001d3088` |
| memt | `FUN_001a6f48` | 0x1c95600 (records), 0x1c95604 (presets) | `FUN_001a6ff0`, `FUN_001a7330` |
| arif | `FUN_00193370` | 0x1c8a970 | `FUN_001933a0` |
| went | `FUN_001845a0` | 0x1c89700 | `FUN_001845b0` |
| wmst | `FUN_00184630` | 0x1c89704 | `FUN_00184640` |
| ftst | `FUN_001a7dd8` | 0x1c95640 | `FUN_001a7df8` |
| rcct | `FUN_001d0248` | 0x1d6a924 | `FUN_001d0258` |
| evtp | `FUN_001d1118` | 0x1d6a92c | `FUN_001d1128` |
| ipic | `FUN_001b03a8` | 0x1d5d640 | `FUN_001b03c0` |
| shop | `FUN_00265ac0` | 0x36cfe4 | `FUN_00265660`, `FUN_00265688`, `FUN_00265ad0` |

Most keyed tables are searched with `FUN_0030ff40` (binary search: key, base, count, element size, comparator). `tools/mips_scan.py
--jal` does not see tail calls (`j`): `FUN_001845b0` is also reached by a `j` at 0x16ef50 (`FUN_0016ef28`) and `FUN_001d0120` (the
`item_stat` search) by a `j` at 0x1a67e4.

Subfile sizes on the disc, for checking a layout: ptya 12688, patn 488, vbrt 612, memt 1668, arif 20436, went 3604, wmst 4224, ftst 728,
rcct 392, evtp 152, ipic 6264, shop 4256 bytes.

## 1. `ptya`: attack tables of the player objects (`00battle.bin`)

**Layout [V].** u32 version (2), u32 table count (62), then one u32 offset per table (from the subfile start; 0 = the slot is unused),
then the tables. A table is a u32 record count and that many 0x44-byte records. A character's table is its `00objentry` row's
`NeoMoveset`; the actor keeps the table pointer at actor +0xaf0 (`FUN_001746e0`, `FUN_001d5e48`), `FUN_00175ff8` finds a record by its key
byte (`FUN_001d5ec0`: first record whose byte 0 equals the key).

| record offset | size | meaning | reader |
|---|---|---|---|
| +0 | u8 | action key | `FUN_001d5ec0`, `FUN_00177fa8` |
| +1 | u8 | candidate group (3 always, 1 locked target by range, 2 other threats by range, 0 only by key) | `FUN_001cbcf8` (tests `== 3`), `FUN_001cbf48` |
| +2 | s8 | alternative key | `FUN_001cbf48` |
| +3 | u8 | argument of the finisher test | `FUN_0019f1b0` (passed to `FUN_00176908`) |
| +4 | u32 | flags: bit 0 airborne only (`FUN_0019f1b0`), bit 1 adds the actor's jump height to the leap reach (`FUN_00176050`), bit 2 finisher, bits 3 and 4 sign of the target half height | `FUN_0019f1b0`, `FUN_00176050`, `FUN_001cbf48` |
| +8 | u16 | motion played (non-zero or the record is skipped) | `FUN_0019f1b0`, `FUN_00176050` |
| +0xa | u16 | motion queued after it | `FUN_00176050` |
| +0xc | f32 | leap height (a leap exists when above 0) | `FUN_00176050` |
| +0x10 | f32 | leap reach | `FUN_00176050` |
| +0x14 | f32 | leap minimum height | `FUN_00176050`, `FUN_00178108` |
| +0x18, +0x1c | f32 x2 | leap speed minimum and maximum | `FUN_00176050` |
| +0x20, +0x24 | f32 x2 | horizontal distance window | `FUN_001cbf48` |
| +0x28, +0x2c | f32 x2 | height window | `FUN_001cbf48` |
| +0x30, +0x34 | f32 x2 | window on the facing/direction dot product | `FUN_001cbf48` |
| +0x38 | f32 | blend of the queued motion (+0xa) | `FUN_00176050` passes it as the first argument of `FUN_0017e378`, which is `FUN_0017e170`, the motion queue whose first argument host 1.13 names "blend" (docs/systems/script-host/contract.md) [V that it is passed; I that it is the blend of the +0xa motion] |
| +0x3c | f32 | leap gap added to the horizontal distance | `FUN_00176050` |
| +0x40 | u16 | ability bit that must be set (0 none) | `FUN_0019f1b0` |
| +0x42 | u16 | not read [I: padding] | no load at +0x42 in the 0x174000..0x178fff, 0x19f000, 0x1cb000..0x1cc000 ranges (`mips_scan.py --load-off 0x42`) |

**App.** `crates/kh2-anim/src/moveset.rs` (`Moveset::parse`, record fields +0..+0xb, +0x20..+0x35, +0x40) and
`crates/kh2-app/src/sora.rs` (`AttackLeaps::parse`, `Leap::from_record`: +0xc..+0x1c and +0x3c; every table), `drive.rs` (a form's
table), `menu_abilities.rs`. The app does not read +0x38 (the queued motion's blend) or +0x42. **Unknown:** the meaning of flag bits
above 4; whether anything reads +0x42 through a computed base [GAP: the scan covers direct offsets only].

Docs: docs/systems/player-camera/moveset-selection.md, docs/systems/player-camera/contract.md "ptya leap fields".

## 2. `patn`: reaction conversion by victim state (`00battle.bin`)

**Layout [V].** u32 version (2), u32 count (15), then 32-byte records: +0 u8 status bit number, +1.. a replacement reaction per original
reaction (0xff keeps it). The reader indexes `record[1 + original reaction]` where the original reaction is the hit record's byte +0x27
(`FUN_001bff00`, loop step 0x20); the records end 31 bytes after the status byte, so the map has room for 31 reactions. Reaction numbers
seen by the app are 0..18. **Unknown:** the bytes at record +0x14..+0x1f are never indexed by a reaction number the app has met [I:
unused for reactions above 18].

`FUN_001bff00` (callers `FUN_00175340` for Sora's receiver, `FUN_001b21f0` for the enemy classes): skipped when the hit is already
ineffective (hit record +0x18 bit 4); for each record whose status bit is set in the victim's status mask `(+0xabc | +0xac0) & ~+0xac4`
the effective reaction (hit record +0x26) is replaced, later records winning. Bit 7 applies only when the victim is not airborne
(`FUN_0019aa28`), bit 8 only when airborne, bit 2 only when the attacker's +0x588 has 0x4000. **What sets the status bits.** Range
triggers: `FUN_00188030` case 0x24 sets bit `n` in actor +0xac0 and case 0x25 sets it in +0xac4 (`n` is the trigger's first
parameter, only when the actor's +0x588 has bit 2). The writer of +0xabc (the third
word of the mask) was not looked for here [GAP]. Docs/hit-reaction.md says "what sets each status bit is unknown"; the two trigger
cases are the first writers found.

**App.** `reaction_rules::parse_patn` and `effective_reaction` (`combat.rs` loads it). Status bit 7/8/2 conditions and the 19-byte map
are used; the rest of the record is not.

## 3. `vbrt`: controller vibration (`00battle.bin`)

**Layout [V].** Not a flat table: a BAR (magic `BAR\x01`, 10 entries). Entry 0 (type 2, tag `vibr`) is the key table: u32 version (1),
u32 count (9), then 2-byte rows `{u8 key, u8 index}` sorted by key. `FUN_001d3050` keeps the BAR (0x360768) and entry 0 (0x1d6a988).
`FUN_001d3088(key)` searches the rows (`FUN_0030ff40`, element size 2, comparator 0x334100 compares the key byte), takes the n-th entry of
type 0x2f (`FUN_001684c0(bar, 0x2f, index)`) and starts it with `FUN_00239950`. Entries 1..9 (type 0x2f) are the waveforms.

Waveform entry [V for the header, I for the meaning]: magic `VIBD`, u32 at +4 (a version), then `+8` u32 count A, `+0xc` u32 offset A,
`+0x10` u32 count B, `+0x14` u32 offset B, and the lists: 8-byte `{u32 frame, u32 strength}` steps (the strength byte is 0..0xff).
`FUN_00239950` copies the two (count, list) pairs into the vibration player's state at 0x36ca08..0x36ca20 and keeps the largest frame as the length [V]; that A and B are the two
motors of the pad (small and large) is [I].

**Who plays it [V].** The motion frame trigger 31 (case 31 of the switch at 0x1884fc in the frame-trigger handler `FUN_00188498`; its
jump table is at 0x37aa78, index 31 is 0x1889c0): `jal 0x1d3088` with the trigger's first parameter (u16 at trigger +4) as the key.
OpenKH's "frame trigger 31 is vibration" is confirmed. The Config menu's Vibration row (`menu_config.rs`) holds an option bit; that it gates this player was not traced [GAP].

**App.** None: nothing in `crates/` reads `vbrt` and the app has no pad rumble (docs/systems/combat-effects/hit-reaction.md "Hit-stop": "vbrt is not modelled
yet"; docs/systems/party/contract.md: nothing party-specific reads it). It has no visible or audible effect, so leaving it out is consistent with
rule A1. **Unknown:** what the two lists drive on the pad (motors, units of the frame).

## 4. `memt`: party member objects per world (`03system.bin`)

Docs/party.md "memt" has the full account (the heading there says `00battle.bin`; the table is in `03system.bin`). Summary with the
checks done here.

**Layout [V].** u32 version (5), u32 count (34), 34 records of 0x30 bytes, then seven 4-byte party presets (28 bytes, `DAT_01c95604` =
base + 8 + count * 0x30). Record: +0 u16 world (0 = any, else the world id in 0x33e830), +2 u16 story flag that must be set (0 none),
+4 u16 story flag that must be clear (0 none) (`FUN_001a6f70` with `FUN_001802e0`), +6 u16 **not read** by the merge or the test, +8
s32 and +0xc s32 (the last positive value over the matching records is kept and passed to `FUN_0016f278`; [I] an object memory budget),
+0x10 u16 x 16 member object ids (0 player, 1 Donald, 2 Goofy, 3 partner, 4..8 forms Valor..Anti, 9 Mickey, 10..15 keyblades). A
non-zero member word of a matching record replaces the merged word (`FUN_001a6ff0`, called from `FUN_001689c0` on area load). The
presets are applied by `FUN_001a7330` (callers `FUN_00180b48` area-script command 0xf, `FUN_0019d9d8`).

**App.** `kh2_tables::player::Memt` (all 24 words and the presets), `party.rs` (merge, presets), `drive.rs`, oracle test
`crates/kh2/tests/party_oracle.rs`. **Unknown:** record +6 (never read: [I] padding); the names of +8 and +0xc beyond [I].

## 5. `arif`: area information (`03system.bin`)

**Layout [V].** u32 version (1), u32 world count (19), then per world a u16 record count (+8 + 4 * world) and a u16 offset (+10 + 4 *
world) from the subfile start; area records are 0x40 bytes at `offset + 0x40 * area` (`FUN_001933a0`, world and area -1 mean the
current ones, 0x33e830 byte 1 and byte 0). The disc holds 318 records in total.

| record offset | size | meaning | reader |
|---|---|---|---|
| +0 | u32 | flag bits | `FUN_00193410(world, area, bit)` |
| +4..+0xf | 12 bytes | not read [GAP] | no reader in the five callers of `FUN_001933a0` |
| +0x10..+0x2f | 8 x (u16 field music, u16 battle music) | music sets; set `n` is a packed u32 at +0x10 + 4n, stored by `FUN_00193510` into 0x1c95638 (`FUN_001a7700`) with `n` = the world's saved byte at 0x33fe0c + world | `FUN_00193510`; the app reads set 0 only (`audio.rs`, `crates/kh2-ard/tests/disc.rs`) |
| +0x30 | u16 | the battle voice bank number in `voice/<lang>/battle/<world><n>_<speaker>.vsb` (docs/systems/audio/audio.md); `FUN_001ae180` also matches it against the three loaded banks at 0x1cb8290 | `FUN_001ae180` |
| +0x32 | u16 | an item id: `FUN_00193560` sets the flag at 0x1c8a974 when the player owns it (`FUN_001a1cc8`); also run at the end of an event (`FUN_00196170`) | `FUN_00193560` |
| +0x34 | u8 | digit of the command-menu layout file `field2d/<world>/<world><digit>command.2dd` (format at 0x378d58; `FUN_0013a6d0` via `FUN_001935e8`; the file is reloaded when world or digit changes) | `FUN_001935e8` |
| +0x35..+0x3f | 11 bytes | not read [GAP] | none found |

Flag bits [V for the callers, I for meaning]: bit 0 (`FUN_00193458`, caller `FUN_0019d818`), bit 1 indoor camera (`FUN_00193478`,
`FUN_00177128`; docs/systems/player-camera/contract.md), bit 2 (`FUN_001934a0`, caller `FUN_0013ab08`), bit 3 (`FUN_001934c0`, caller `FUN_00168d30`), bit 4
(`FUN_001934e8`, caller `FUN_0015da80`). The meanings of bits 0, 2, 3 and 4 were not traced [GAP].

**App.** `audio.rs` `area_arif` (voice bank +0x30, music set 0 as a stand-in for the area script's music), `camera.rs` `area_flags`
(bit 1), `crates/kh2-ard/tests/disc.rs` (prints the pairs). Not used: music sets 1..7, +0x32, +0x34 (the app picks its own command
menu layout), flag bits other than 1.

## 6. `went`: weapon objects per moveset (`03system.bin`)

**Layout [V].** No header. 66 u32 values first (index = the object's `NeoMoveset`; 0 = no weapon list), each the position in u32 words
from the subfile start of that moveset's list; then the lists, each a run of u32 object ids indexed by an item's weapon slot, which is
the u16 at item +4 (`Flag1 | Rank << 8`) (`FUN_001845b0`: `base + slot * 4 + offset * 4`, `FUN_001a66e8` returns the item row). The
offsets are not all distinct: several movesets share a list (the disc has 24 distinct values over 66 entries). The list lengths are
not stored; they run to the next offset or the end.

Callers: `FUN_00174fb0` (the weapon attach in the player constructor) and `FUN_0016ef28` (tail call at 0x16ef50, via `FUN_0016ee98`,
which picks the item for the hand; docs/systems/drive-limits-summons/drive-forms.md).

**App.** `drive.rs` `DriveRules::from_archive` / `rules::weapon_object`, `party.rs` (`went` blob). Fully used. **Unknown:** list lengths
(the game relies on the item slot staying inside the list).

## 7. `wmst`: weapon motion sets per moveset (`03system.bin`)

**Layout [V].** No header, 66 records of 0x40 bytes, indexed by `NeoMoveset`: two NUL padded 32-byte names, +0 the right hand
(hand 0) and +0x20 the left hand (hand 1). `FUN_00184640(moveset, hand, buffer)` formats the name with `obj/%s.mset` (0x37a8c0) and returns
0 when the name is empty. Callers: `FUN_0016e818` (preload), `FUN_00184058` and `FUN_00184190` (weapon attach). An empty name means the
weapon gets no motion set and is not drawn [V for the return, I for "not drawn"; docs/systems/drive-limits-summons/drive-forms.md].

**App.** `drive.rs` `rules::parse_wmst`, `party.rs`. Fully used. Nothing unknown.

## 8. `ftst`: text colours (`03system.bin`)

**Layout [V].** u32 version (1), u32 count (9), then 0x50-byte records: u32 id (1..8 and 10) and 19 RGBA words, one per world index (the
order of `text::WORLDS`, docs/systems/ui/hud-panels.md "Colours"). `FUN_001a7df8` is the lookup; callers `FUN_001a8210` and `FUN_001aa928` (text command 4,
the colour command of the text engine).

**App.** `kh2_image::text::ColorTable::parse`, loaded by `hud_text.rs` and `menu_text.rs`. Fully used. Nothing unknown.

## 9. `rcct`: reaction command lists of the journal (`03system.bin`)

**Layout [V].** u32 version (1), u32 count (32), then 12-byte records `{u16 key, u16 command[5]}`, sorted, binary searched on the key
(`FUN_001d0258`, comparator 0x334008). Key = an enemy object's NeoStatus (objentry +0x4c); the five commands are the reaction commands
the Jiminy journal's enemy page lists (0 = empty). The only reader is `FUN_002aadf0`.

**App.** `kh2_tables::player::parse_rcct`, `menu_journal/enemies.rs` `reaction_rows`, `kills.rs` `read_rcct`. Fully used.
docs/systems/combat-effects/player-data.md "Reaction commands", docs/systems/ui/contract.md. Nothing unknown.

## 10. `evtp`: event kinds (`03system.bin`)

**Layout [V].** u32 version (1), u32 count (18), then 8-byte records: +0 u8 kind, +1 u8 start screen effect (1 and 2: `FUN_0014a918` +
`FUN_0014a7c8`, a fade out to the colour; 3: the key-hole layout `FUN_0015d058`, not driven by the app), +2 u16 flags, +4 f32 fade length
(frames before multiplication by the constant at 0x37b050). Flags [V, `FUN_00195d28`, `FUN_00195ae0`, `FUN_00196170`]: bit 0 chooses
the object's start state (`+0x2c` 1 with the bit, else 4), bit 1 runs `FUN_002f5fe8`/`FUN_002c38b8` (the event's sound/stream set-up; with
kind 0x10 also sets object flag 4), bit 2 clear hides the field HUD tags 3..6, bit 3 clear takes control from the players. The row is
looked up by `arg & 0x3f` in `FUN_00195898` and `FUN_00195778` (the second only reads the kind to set `DAT_0035e3a0/1`: kinds 2 and 3).

**App.** `event_player::kind_rows` (kind, fade mode, flags, fade length) and `struggle::event_kinds`. Used: flags bits 2 and 3, the fade mode
1 and 2 and its length. **Unknown/not ported:** flag bits 0 and 1, fade mode 3.

## 11. `ipic`: item picture sprite sequence (`03system.bin`, entry type 25)

**Layout [V for the container].** A SEQD sprite-sequence file (magic `SEQD`, version word 0x160, 3 sprites at +0x30, 3 parts at +0xb4,
3 sprite groups at +0xf0, the animation tables after; the repo's `kh2_image::Seqd` reads this format). It is the sequence the item
window uses to draw a picture: `FUN_00137b38` (a HUD object's picture setup) and `FUN_002830f8` (the item window) fetch it with
`FUN_001b03c0` and pass it, with a sequence id, to the sprite drawer `FUN_00138948`; the picture itself is the image file
`itempic/item-%03d.imd` (format at 0x37b968, loaded by `FUN_001b0438` through `FUN_001b03d0`, which also loads it for the field HUD, the
id being the item row's picture field).

**App.** None reads `ipic`; `menu_pic.rs` draws the picture as one quad scaled to the window. **Unknown:** what the three sprite groups
frame or colour (a frame, a drop shadow or only the placement) and whether the quad should be built from them [GAP: not read from the
sequence data here]. This is a possible small visual difference in the item windows.

## 12. `shop`: shop file (`03system.bin`, entry type 0x29)

**Layout [V].** Not a flat table. Magic `TZSH`, u16 version 7 at +4, i16 record count at +6 (21), u32 offset of the "new" item list at +0xc, u32 offset of
the records at +0x10 (0x18 bytes each: +0 event, +2 record bit, +4 header message, +6..+0xc four words, +0xe flags, +0x10 group count, +0x14
group offset); groups of 8 bytes (i16 record bit, u16 count, u32 offset of item ids). Full account in docs/systems/ui/menus-shop.md 22.2. Readers:
`FUN_00265660` (header check), `FUN_00265688` (the stock for an event), `FUN_00265ad0` (the item's place in the new list),
`FUN_00265970`, `FUN_00265a50`.

**App.** `menu_shop.rs` (`Shops`, `stock`; test asserts the tag). Fully used except: **Unknown** the four words at record +6..+0xc
(`docs/systems/ui/menus-shop.md`: "their readers" are a gap).

## 13. The three tables with placeholder field names

**`atkp` `Unknown` (s16 at record +0x10) [V].** The swirl of a Magnet-style pull. Only the class 5 reaction handler reads it
(`FUN_001bf920`, via `FUN_00185668` from `FUN_00184e80`): it keeps the value at the victim's +0xab4, and each frame `FUN_001bfc10`
adds the pull direction turned a right angle (`FUN_0017c2e8`) times this value to the victim's velocity. The two knockback fields
before it give the hold distance range. The app uses it (`enemy.rs` `rules::Pull::new(.., atk.unknown, ..)`; docs/systems/magic/magic-turn.md "Magnet",
docs/systems/combat-effects/hit-reaction.md class 5). Proposed name `PullSwirl`. The 277 bytes after the records (`tables.json` note) were not examined.

**`fmlv` `Unk0` and `Unk1` [V].** `Unk0` is the key byte `form << 4 | level` the table is searched by (`FUN_001d3b90` through
`FUN_0030ff40`, element size 8, first byte compared; form 0 is the summon level curve, forms 1..4 the drive forms, form 5 the Anti roll
table). `Unk1` is the level shown to the player (1..4), whose rise upgrades the form's growth ability (`FUN_001d3da8`); for form 5 it is the
Anti-form chance in percent (`FUN_001d4278`). The app already splits them (`kh2_tables::player::FormLevels::from_rows`). Proposed names
`Key` (or `FormLevelKey`) and `DisplayLevel`. docs/systems/combat-effects/player-data.md "fmlv".

**`item_stat` `Unknown08`, `Unknown0d`, `Unknown` [V for 08/0d, I for the last].** The seven bytes at +8..+0xe are resistance
multipliers in percent, one per element number: +8 physical, +9 fire, +0xa ice, +0xb thunder, +0xc dark, +0xd light, +0xe general.
`FUN_001a0c08` (called for every equipped item) multiplies the status bytes at +0x1a8..+0x1ae by them, rounded up
(`(a * b + 99) / 100`). The damage code reads status `+0x1a8 + element` (physical, element 0 of the hit row) and `+0x1ae` (general)
(`FUN_0018b618`, `FUN_001a0610`); the menus show fire, ice, thunder and dark as `100 - value` (docs/systems/ui/menus-status.md section 10). So
`Unknown08` = `PhysicalResistance` and `Unknown0d` = `LightResistance` [V for the position in the element order from the loop and the
damage read; the name "light" is I from the order of the `enmp` weakness fields: physical, fire, ice, thunder, dark, light, general].
The final byte (+0xf) is read by nothing found: `FUN_001a67b0` is the only accessor (10 callers: `FUN_001a0c08`, `FUN_0026a2a0`,
`FUN_0026bd08`, `FUN_0026ca28`, `FUN_0027adc0`, `FUN_002a01b8` read +2, +7 or the item row; none loads +0xf) [I: padding]. The app
exposes fire, ice, thunder and dark resistance to the status page (`menu_status.rs`); the physical, light and general multipliers
are not applied to the player's damage (docs/systems/combat-effects/abilities.md "Resistance multipliers of equipment ... are not exposed").

## 14. Proposed sheet edits

See the WP report (JSON patch). In short: `rcct`, `evtp`, `ftst`, `wmst`, `patn` and `memt` fit the existing `table` and `list`
headers; `ptya`, `went`, `arif`, `vbrt`, `shop`, `ipic` are containers (offset directories, a BAR, a magic file) that need either a new
header kind or a `notes` entry; `atkp`, `fmlv`, `item_stat` need only field renames.

## Decompile index

0x193610 (loader), 0x1d5e38, 0x1d5e48, 0x1d5e90, 0x1d5ec0, 0x175ff8, 0x1746e0, 0x174800, 0x1cbcf8, 0x1cbf48, 0x19f1b0, 0x176050,
0x178108, 0x17e000, 0x17e170, 0x17e378, 0x1bfef0, 0x1bff00, 0x175340, 0x1b21f0, 0x188030, 0x188498, 0x1889c0, 0x1d3050, 0x1d3088,
0x239950, 0x30ff40, 0x334100, 0x1a6f48, 0x1a6f70, 0x1a6ff0, 0x1a7330, 0x1802e0, 0x16f278, 0x193370, 0x1933a0, 0x193410, 0x193458,
0x193478, 0x1934a0, 0x1934c0, 0x1934e8, 0x193510, 0x193560, 0x1935e8, 0x1a7700, 0x1ae180, 0x13a6d0, 0x1845a0, 0x1845b0, 0x184630,
0x184640, 0x16e818, 0x184058, 0x184190, 0x16ef28, 0x1a7dd8, 0x1a7df8, 0x1d0248, 0x1d0258, 0x2aadf0, 0x1d1118, 0x1d1128, 0x195778,
0x195898, 0x195ae0, 0x195d28, 0x196170, 0x1b03a8, 0x1b03c0, 0x1b03d0, 0x1b0438, 0x137b38, 0x2830f8, 0x265660, 0x265688, 0x265ac0,
0x265ad0, 0x1bf920, 0x185668, 0x184e80, 0x1bfc10, 0x1d3b80, 0x1d3b90, 0x1d3da8, 0x1d4278, 0x1a66b8, 0x1a66e8, 0x1a67b0, 0x1d0110,
0x1d0120, 0x1a0c08, 0x18b618, 0x1a0610, 0x26ca28.

## Gaps

* `ptya` +0x42 and the flag bits above 4; `patn` bytes +0x14..+0x1f and the writer of the status word at +0xabc (searched: `mips_scan.py
  --store-off 0xabc` was not run; only the trigger cases 0x24 and 0x25 were read).
* `vbrt` waveform meaning (two lists, motors, units); no app consumer.
* `arif` flag bits 0, 2, 3, 4 and the unread bytes +4..+0xf, +0x35..+0x3f (searched: the five callers of `FUN_001933a0` and the `j`
  tail calls).
* `ipic` role of the three sprite groups; `evtp` flag bits 0 and 1 and fade mode 3; `shop` record words +6..+0xc.
* `atkp` 277 trailing bytes.
