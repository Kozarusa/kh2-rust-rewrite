# Player data: drive forms, limits, reaction commands, abilities, summons

Findings for work package 1D. This is a data and design note, not gameplay. Every statement is marked
**[V]** verified (read in the game's own code or checked against the disc data) or **[I]** inferred (fits the
code and data but no line of code states it) or **[GAP]** not found.

Function names are the addresses in the main executable (`SLUS_210.05`, written `FUN_xxxxxxxx`). The
decompiled code is a local study aid and is not in the repository. No game values are in this repository:
the numbers below are structure (which field, which unit); the values come from the disc at run time.

## How to look at the data

New decoders live in `crates/kh2-tables/src/player.rs` (typed views and loaders) and `msg.rs` (system
message text). `kh2 table NAME [LIMIT]` prints them from the user's disc:

| dump | what it shows |
|---|---|
| `drive_forms` | one row per drive form: form id, "X Form" item, drive command, cost, player object, keyblade object, top level |
| `form_levels` | the fmlv curve with the granted ability's name |
| `abilities` | every ability item: id, name, ability bit, AP cost, kind, command |
| `limits_ex` | limt joined with the command and ability names |
| `summons_ex` | sumn joined with the command name |
| `rcct` | the journal reaction-command lists (see below) |
| `memt` | party member records as raw words (record 0 is Sora) |
| `sstm_full` | the whole engine tuning record with the offsets that were traced to readers |

Tables that `sheets/tables.json` already generates (`fmlv`, `cmd`, `item`, `limt`, `sumn`, `magc`, `plrp`,
`pref_*`) are used through their generated structs. The sheet's `fmlv` columns `Unk0`/`Unk1` are the key
and the displayed level, see below. Suggested sheet changes are listed in the work package report.

## Vocabulary the game uses

- **Form id** [V]: 1 Valor, 2 Wisdom, 3 Master, 4 Final, 5 Anti. 0 is "no form". This is `cmd.Argument` of a
  drive command, `ObjectForm` of the form's player object, the key nibble of `fmlv`, and the value of the
  player object's form field (object +0xb50, with 6 and 0xb as special non-drive modes, see gaps). There is no
  Limit Form on this disc: that form is a Final Mix addition.
- **Party member id** [V]: the byte in `limt.Character` and `plrp.Character`: 1 Sora, 2 Donald, 3 Goofy,
  5 Auron, 6 Mulan, 7 Aladdin, 8 Sparrow, 9 Beast, 10 Jack, 11 Simba, 12 Tron, 13 Riku. An object's
  NeoStatus (`objentry` +0x4c, FUN_001699a8) is 1 for Sora, 2 Donald, 3 Goofy, and 1000 and up for enemies.
- **Drive points** [V]: the drive gauge is a whole-number count of points (status +0x1b1, maximum +0x1b2)
  plus a 0..99 fraction (+0x1b0), see "Drive gauge" below.

## Command table (`cmd`, 03system.bin)

48-byte records, looked up by `Id` (FUN_001b6f48). The sheet layout is correct against the game's reads. The
fields that matter for the player systems:

| field | use |
|---|---|
| `Execute` [V] | which handler runs the command. The player's executor FUN_001cf030 switches on it: 4 drive, 5 revert, 72 summon, 73 party member, 81 limit, 316 (0x13c) a map command; everything else goes to the generic executor FUN_00177b18. The availability check FUN_001ba2c0 switches on the same values (1 attack, 2 magic, 3 item are also handled there). Reaction commands use their own id here. |
| `Argument` [V] | drive: the form id. limit: the command id again. |
| `Cost` [V] | magic: MP. drive: drive points spent. summon: drive points spent. limit: MP cost code (255 means all MP, FUN_001b9ff0) or, for summon limits, the summon gauge cost. |
| `Time` | limit: duration used by the limit task (FUN_001b28a0 passes it to FUN_001b2c10) [V]. |
| `Require` | id of a command that must be present (chained reaction commands) [I]. |
| `CmdAction` [V] | how the player's own state is handled when the command runs (FUN_00177c20): 1 stops the player if moving, 2 clears velocity and calls the command target's handler; other values just hand the command id to the target. |
| `ReactionCount` [V] | slot in the saved per-command use counters (FUN_001b7168 reads it, FUN_00177c20 increments it, capped at 999). Only the Jiminy journal displays it. |
| `DisableForm` [V] | bit n set means the command is unavailable while the player's form id is n (FUN_001b7290). If `Id != Execute` the check is made on the command whose id equals `Execute`, so a drive command inherits the mask of the generic Drive command. On this disc the masks hide Magic in Valor and Anti, Items in Anti, Party in Master/Final/Anti, and Limit in all five forms. |
| `Flags` [V, partly] | bit 1: unavailable while FUN_0019aa28 is true for the player; bit 16: needs the global flag read by FUN_001903d8; bit 5: ignores the "commands locked" global bit 2 of DAT_0035d268 (FUN_001b70b8). The other bits are not decoded. |
| `Priority`, `Mark`, `Range`, `Dir`, `DirRange`, `DistRange`, `Score`, `Group`, `CmdIcon`, `SubMenu` | command-menu layout and reaction-command targeting. Not traced. [GAP] |

Magic commands (`Execute` 2, `Argument` = the `magc` slot, `Cost` = MP, 255 = all MP) and the `magc`, `magi` and
MP-charge rules are in `docs/magic.md` (WP-6B): a spell with too little MP can still be cast (FUN_001ba2c0 status 2),
paying more than the MP left starts the charge (FUN_00186368 -> FUN_00186440, `prty` MpDrive).

`item` records of type 20 (summon charms) and 21 (drive form items) carry the command id in `Command`
(offset +0x10) [V: FUN_001b7188 does `cmd(item(formItem).Command)`].

## Drive forms

### Data

| source | what |
|---|---|
| `item` type 21 | one "X Form" item per form (five rows). `Command` is the drive command shown in the menu. [V] |
| `cmd` Execute 4 | the drive commands. `Argument` = form id, `Cost` = drive points. A second set of four (Auto Valor/Wisdom/Master/Final commands, see abilities) uses the same Execute and Argument with a different id. [V] |
| `fmlv` | level curve, see below. [V] |
| `memt` record 0 | Sora's objects. `u16 word[12..=16]` are the player objects for forms 1..5 (FUN_001cf2c8: record byte offset 0x16 + 2 * form), `word[18..=23]` the keyblade object for forms 0..5 (FUN_00196ad0: offset 0x24 + 2 * form), `word[8..=11]` the normal player object per world outfit (offset 0x10 + 2 * outfit, outfit = low 5 bits of a per-world saved byte, FUN_001a71e8). [V] |
| `sstm` pref record | tuning: drive point size, drain scales, Anti and Final chances. Offsets in `sstm_full`. [V] |
| `pref_fmab` rows | growth ability physics, indexed by growth level - 1 (FUN_001cc1a0). [V] |

### `fmlv`: form level curve

8-byte records `{u8 key, u8 displayLevel, u16 ability, i32 exp}`, binary-searched on the first byte
(FUN_001d3b90 calls FUN_0030ff40 with key `form << 4 | level`). [V]

- Forms 1..4: internal levels 1..7. `exp` on level L is the form experience needed to leave L (level 7 has 0).
  `ability` is granted when the form reaches that level. `displayLevel` is the level shown to the player (1..4);
  when it rises the form's growth ability is upgraded (below).
- Form 0: the same shape for the summon level curve (FUN_001d3cf8, FUN_001d4ec8). [V]
- Form 5: not levels. Rows are the Anti-form roll table: `exp` is a counter threshold and `displayLevel` the
  chance in percent, see "Anti and Final". [V]
- Level cap [V, FUN_001d3bd0]: `2 + (number of the four forms 1..4 the player owns)`, and 7 when that is 6. The
  form cannot level past the cap even when the experience is there (FUN_001d3c38).
- Experience gain [V, FUN_001d3f88]: add the amount to the form's saved experience (`party record[form] +4`,
  FUN_00187f60 returns record `0x33fec0 + 0xc3c + form * 0x28`, record +2 is the level), then while the current
  row's exp is reached: subtract it, level up (FUN_001d3da8). It does nothing while the minigame flag of
  FUN_001b9bd8 is set.
- Experience sources [V for the call sites, I for the meaning]: Valor +1 for a landed hit by the player (hit
  callback FUN_001849d8 -> FUN_001d4078, needs attacker flag 0x80); Wisdom +1 and Final +1 when the player
  defeats an enemy whose NeoStatus is in 1000..2003 (Wisdom) or 3000..3952 (Final) (FUN_00184a30 ->
  FUN_001d40e0); Master gains the drive-orb value when an orb is picked up (FUN_001a39b0 -> FUN_001d4148).
- Level-up effect [V, FUN_001d3da8]: new internal level stored; if the previous row's displayLevel is lower, the
  growth ability is upgraded in place in the form's own 16-slot ability list and in the party ability list
  (FUN_001d3f18 replaces the slot holding the previous growth ability); if the row has an `ability`, it is
  added to the inventory (FUN_001a18d8) and equipped on the form (FUN_001d0828).
- Growth chain [V]: a table in the executable (0x37c556, 5 u16 per form, not on the disc) lists the growth ability
  ids per form: Valor High Jump, Wisdom Quick Run, Master Aerial Dodge, Final Glide, each LV1, LV2, LV3, MAX
  (consecutive item ids).

### Entering a form

Menu to execution [V]:

1. The Drive command (Execute 4) is available when `cmd.Cost <= drive points` (FUN_001ba568, which also needs
   `DAT_0035d268` bit 0x40000 clear and the player in a normal state; forms 6 and 0xb skip the cost test) and
   `DisableForm` allows the current form.
2. Executing it (FUN_001cf030): `form = FUN_001d4180(player, cmd.Argument)` decides the form actually entered
   (random Anti/Final replacement, below); then `FUN_001a0320(status, cost of that form's drive command)` spends
   the points (`FUN_001b7188(form)` finds the cost through the form item's command); the per-form use counter
   (`FUN_001d76c8`, capped at 999) is incremented; `FUN_001b4c28(player, form)` starts the change.
3. `FUN_001b4c28`: if the form differs from the current one (`DAT_00340c88` byte 4), it creates the change task
   (FUN_001b44c8, 0x20 bytes), and for a non-zero form resets the player's motion state (FUN_0016b698), starts the
   change motion (FUN_0017e220 with state 0x34), zeroes the velocity, sets player flag 0x100 and notifies the party members
   (FUN_001c76b0: for each of the two companions, FUN_001c77a8 says whether that character is affected by the form:
   form 1 Goofy, form 2 Donald, forms 3 to 5 both; the state is set with FUN_001c75d0. This matches the
   `DisableForm` masks, which keep the Party menu in Valor and Wisdom only, so the affected companion is most likely
   the one absorbed into Sora [I]).
4. The task (FUN_001b4740) loads the form's change effect (FUN_0016e7e8 -> FUN_0016e430 with the form's player
   object), waits for the old object's change motion, then creates the new player object (FUN_001ce820) from the
   objentry of the form object (FUN_0016e208) and swaps the camera/controller to it. Then FUN_001b4bd0 starts the
   timer (next section).
5. The new object's constructor (FUN_001ce820 / FUN_001ce718) sets the gauge mode (status +0x1af) to 1 normal,
   3 summon active, 4 when `DAT_0035d268 & 0x40000`; entering a form sets it to 2 (next section).

Spending: only `cost` points are removed; the points left stay [I: no code path in the entry sequence clears
them; MP is refilled and MP-charge is ended by FUN_001a0240].

### The form timer ("drive gauge" while in a form)

[V] unless marked.

- `FUN_001a04b0(status, form, level)` runs when the change finishes (from FUN_001b4bd0, `level` = the form's
  saved level, record byte +2). For form 5 `level = sstm[0x8c] - 2`. It refills MP to its maximum (FUN_001a0240)
  and calls `FUN_001a0480(status, 2, level)`: status +0x1af = 2, +0x1b3 = level + 3, and the timer
  +0x1b4 = +0x1b8 = `sstm[0x68] * (level + 3)`. `sstm[0x68]` is the size of one drive point in gauge units.
- Each frame (FUN_00177788): when mode is 2 or 3 and the game is not paused, the drain is the frame unit
  (`DAT_00356f40`), times `sstm[0x6c]` while the global flag of FUN_001903d8 is clear, times the ability-block
  multiplier (+0x3c in form mode, Form Boost; +0x68 in summon mode, Summon Boost). `FUN_001a0358(-drain,
  status)` subtracts it, clamped to `0..max`. In Anti form (object form field 5) a positive change is negated
  (FUN_001a0358 first lines).
- Extra gauge while in a form [I, FUN_00178748, FUN_001788d8]: hits that carry atkp `DriveDrain` (atkp +0x29)
  change the timer by `DriveDrain * sstm[0x70]` (gain) or `-DriveDrain * sstm[0x74]` (loss), scaled by
  `sstm[0x64]` when the attacker flag is 0x800. Which side (dealing or taking) uses which sign was not separated.
- Exit when empty [V, FUN_001ceda8]: the update checks the timer `+0x1b4 <= 0` and issues the Revert command
  (cmd 5, Execute 5): `FUN_001b4c28(player, 0)`, the same change task with form 0, marked "revert" (task +0x1c).
  The player can also pick Revert from the menu (FUN_001ba2c0 case 5 needs FUN_001c7a38).
- When the form ends, the gauge mode is set back to 1 by the new object's constructor [V].

### Normal-mode drive gauge

[V] unless marked. `status +0x1b1` points, `+0x1b2` maximum, `+0x1b0` fraction 0..99.

- Gain (FUN_001a0290(status, delta)): add `delta` to the fraction; 100 carries into a point up to the maximum;
  a negative delta borrows from a point. FUN_001788d8 calls it with `damage-dependent amount *
  FUN_001ba040()` (a global scale) for normal mode, multiplied by `sstm[0x78]` and the ability-block +0x38 when
  MP-charging (FUN_001a05d0: charge timer at status +0x1c0 > 0).
- Raising the maximum (FUN_001be500, from FUN_001be6b8): adds a value to +0x1b2 (the bonus table `bons`
  `DriveGaugeUpgrade` field) [I].
- Two player-object setup routines (FUN_001cda78, FUN_001cdb98) set points 0 and maximum 3 [V]; saved values
  restore the rest (FUN_0019f920 copies bytes +0x1b0..+0x1b3 and floats +0x1b4/+0x1b8 from the saved party record).
- The global flag read by FUN_001903d8 (`DAT_01c8a7d8`) gates three things: the form timer drains at the reduced
  scale only while it is clear, the Anti roll can only succeed while it is set, and limit prompts are only offered
  while it is set (FUN_001d2460). It is most likely "a battle is in progress" [I]; where it is written was not
  traced.
- Drive orbs pick up through FUN_001a39b0, which calls FUN_001788d8 with the orb's drive amount (table at
  0x37b540, 8 bytes per orb type, not on the disc) [V].

### Anti and Final selection

FUN_001d4180(player, chosen) [V]:

1. If `chosen` is Final (4): Anti counter -= `sstm[0xa4]` (floored at 0), return 4.
2. Otherwise run the Anti roll (FUN_001d4278): only if the player may have Anti (FUN_001c77f0(5) == 0); chance
   = the `fmlv` form-5 row whose threshold the counter has reached (counter >= row 0's exp -> row 0; else
   next row...) times a factor (FUN_001b9d70: 1, or a byte from the current special battle), compared with a
   random 0..99 from the game's LCG (`DAT_003716a0 * 0x10dcd`, bucketed by `/ (2^32 / 100)`). The counter
   `party record[5] +4` increases by one on every such roll. If the roll fails return `chosen`.
3. If the global flag of FUN_001903d8 is clear return `chosen`.
4. Run the Final roll (FUN_001d4348), only if the Final Form item is not owned (item 0x1d count 0), the form
   check FUN_001c77f0(4) passes, story flag 0x4842 is set and the points are enough for Final's cost: chance =
   min(`sstm[0xa0]`, `sstm[0x98] * pow(sstm[0x9c], counter4)`) percent (FUN_00300448 is taken to be `powf` [I]; the
   comparison value is the LCG scaled to 0..100); `counter4` (`party record[4] +4`)
   increases per roll. On success the Final Form item is added to the inventory, the Anti counter drops by
   `sstm[0xa4]` and Final (4) is entered.
5. Otherwise Anti (5) is entered and the counter drops by `sstm[0x90]`.

Anti form also multiplies the damage the player deals by `sstm[0x94]` (FUN_001a0610 end) [V], and its gauge
change sign is inverted (above).

### Stats and abilities in a form

- Abilities [V]: the form's own list (16 u16 slots at form record +8) is applied fully; the player's own list
  (64 slots at party record +0x50, top bit set = equipped) is applied only for abilities whose `StatEntry` low
  byte is 0 (support abilities) while a form record is active (FUN_001c0e50). Action abilities (Guard, Upper
  Slash and so on, kind 2) and growth abilities (kind 3) therefore come only from the form while in a form.
- Auto-form abilities [V, FUN_001cef10]: the four "Auto X" abilities are tested in the order Final, Master,
  Wisdom, Valor (list at 0x37c350). If the ability bit is set and the item's command is available
  (FUN_001ba2c0 < 4) the command is offered as a reaction-style prompt (FUN_001bb3a8, slot 1).
- Stat changes by form [V negative]: no per-form Strength/Magic/Defense table exists in the paths read (see
  docs/drive-forms.md, "Stats and abilities in a form"). What a form changes is the ability list (above), the
  object and its attack table, the Anti damage multiplier, and, at the timer start, HP and MP restored to
  their maximum (`FUN_001a04b0`, WP-2B2). The form keyblades (`H_EX500_<F>`) are visual objects (objentry type
  2) with no stat fields.
- The form record's weapon word [V, WP-2B2]: record `+0` is the form's second keyblade item, written by
  `FUN_001a2010` when the form item is obtained (the first spare keyblade in the item table, one copy taken
  from the inventory; Valor, Master and Final only). `FUN_0016ee98` reads it for hand 1 of the form's object.
- Level ability grants are the `fmlv` `ability` column [V] (for example Valor level 2 grants the Auto Valor
  ability; the form_levels dump names each grant).

### Files a form needs

Per form `F` in `BTLF` (Valor), `MAGF` (Wisdom), `TRIF` (Master), `ULTF` (Final), `HTLF` (Anti) [V from memt +
objentry + the file list]:

- `obj/P_EX100_F.mdlx`, `obj/P_EX100_F.mset` (motion set), `obj/P_EX100_F.apdx` (sound banks and extras).
  World outfit variants `P_EX100_TR_F`, `_WI_F`, `_NM_F` use their own mdlx/apdx and share the form's mset;
  their objentry rows (ObjectForm = the form id) are separate objects. How the game maps a world outfit to the
  variant object is not in `memt` for forms (memt gives one object per form) [GAP].
- The weapons: the equipped keyblade's object through the moveset's `went` row (hand 0) and the form
  record's keyblade (hand 1, dual-wielding forms), with the motion sets named by `wmst` for the form's
  moveset (`W_EX010_BTLF_R` / `_L` when the Kingdom Key is held) [V, docs/drive-forms.md; corrects the
  earlier guess at `H_EX500_F` and `SORA_WEAPON_BTLF`].
- The change effect resource loaded by FUN_0016e7e8 is keyed by the form's player object [V]; its file was not
  identified [GAP].
- `objentry` fields of the player object: `NeoStatus` 1, `NeoMoveset` a form-specific index [V from the dump].

## Limits

### Data (`limt`, 64-byte records, binary layout confirmed against the readers)

| field | meaning |
|---|---|
| `Id` | limit index. Several rows can share an Id (world variants). |
| `Character`, `Summon`, `Group` [V] | the participants. Bytes 1..3 of the record are each a party member id that must be present in the party (FUN_001d1ce0 loops the three bytes; FUN_00175ba0(id) finds the member). `Summon` values 200..204 mean a summon limit (FUN_001d1fe0): no second participant, it spends the summon gauge. The sheet's column names come from OpenKH; the code treats them as three slots. |
| `FileName` [V] | the limit's resource BAR: `limit/us/<name>` (format string `limit/%s/%s`, language directory, FUN_001d1ff8). |
| `SpawnId` | object spawned for the limit, 0 if none (for example Sparrow's ship, Tron's cycle, Riku). [I] |
| `Command` [V] | the limit command id (cmd Execute 81, Argument = the same id). FUN_001d2040 / FUN_001d2518 look up by it. |
| `Limit` [V] | an item id. For a normal limit it is the limit's ability item (type 19): the command is available only when the member `Character`'s ability bit for it is set (FUN_001d1ce0, tested only when the item type is 19). For a summon limit it is the summon's charm item (type 20). |
| `World` [V] | 0 = any world, otherwise only when the current world id (`DAT_0033e830`) equals it (the Hundred Acre Wood variants). The first usable row for a command wins. |

Availability (FUN_001b3e38, case Execute 81) [V]: the limit row must exist and be usable; the command must not be
blocked by FUN_001b3c00 / the global lock `0x35fec0`; normal limit: `MP >= cost` (FUN_001b3f00 -> FUN_001b9ff0,
255 = all MP) else status 2 (not enough MP); summon limit: the summon gauge float must be non-zero.

### Execution (FUN_001b28a0) [V]

1. Look up the limt row for the command (FUN_001d2040). If missing, send message 0x17 to the previous limit task.
2. Create the limit task (0xdc bytes, FUN_001b2c10 with `cmd.Time` as the duration) which resolves the resource BAR
   (FUN_001b2e08), marks the participants (`Character` != 1 sets flag at member +0xb4c, and the participants'
   object flag 0x10000), and loads the entries by BAR type: 3 (script), 18 (PAX effects), 20 (motion/model
   set), 34 (a second resource). Observed contents of `limit/us/auron.bar`: `limi` type 3, `LIMI` type 20,
   `limi` type 18, `00`/`10` type 22, `wave` type 32 and `se40` type 31 (sound), `comb`/`gage` type 24/25
   (HUD textures). [V for the BAR types the loader asks for; I for what each does]
3. Cost: normal limit, `FUN_00186368(player, -mpCost)`; summon limit, `FUN_001788d8(-cmd.Cost, player)` takes it from the summon gauge.
4. If `Character` != 1 call FUN_00175db0(player); put both companions into the limit state (FUN_001c75d0(0, member))
   unless already flagged.

### Triggering [GAP in part]

The Limit command itself appears in the command menu as Execute 81 / the generic "Limit" menu entry (cmd 81).
The commands listed in `limt` are what the sub-menu shows. How the menu picks which limit rows to list is in
FUN_001d21e0 / FUN_001d2320 (loops the limt rows, skips ones failing FUN_001d1ce0 and de-duplicates by `Id`)
[V]. The list is rebuilt by FUN_001d2320 (from FUN_001bcc60), which adds every usable limt command to the Limit
sub-menu through the command manager (FUN_001ba748) [V]. Limit prompts exist only while FUN_001d2460 holds: the player
is alive and unlocked, not in a form-change or locked state, the battle flag is set, and the player is not
MP-charging [V]. FUN_001d25e0(player, wantedCommand) picks a limt row whose partner member (`Character`) is a
party object with flag 0x2 within a fixed distance (`DAT_0037c414`) and whose command is available, returning the
wanted command if it qualifies, otherwise a random qualifying one (LCG) [V]; it is called from the party-member
command logic (FUN_0019d430, from FUN_0019c670) with the offered command's `ReactionCount` field as the wanted
command [V that it is called so; I that this is how Auto Limit prompts are produced: the consumer of the Auto
Limit ability bit was not located] [GAP].

## Reaction commands

### What is data

- The command rows themselves (`cmd`): reaction commands have `CmdAction` 1 or 2, `Mark` 2..8, and an `Execute`
  equal to their own id [V from the dump and FUN_00177c20].
- `ReactionCount` is the journal counter slot [V]. `rcct` is **not** the trigger table. `rcct` [V]: u32
  version, u32 count, then `{u16 key, u16 command[5]}`, binary-searched on the key (FUN_001d0258). The only
  reader is the Jiminy journal enemy page (FUN_002aadf0): `key` is the enemy object's NeoStatus (objentry
  +0x4c, found by FUN_0016e208 -> FUN_001d1470), the five commands are the reaction commands shown for that
  enemy together with their use count (FUN_001b7168, shown only when the count is non-zero or the command is
  known). The disc table has 32 rows with keys in the 1000 and 3000 ranges.

### What triggers a reaction command [V]

The offer is made by a **motion range trigger** of id 20 (0x14) in the animation of the object that gives the
reaction (an enemy attack, a map gimmick): FUN_00188030 (the handler for range triggers, a switch on the trigger
id; the same switch implements ids 0..3, 10, 22, 24, 27, 28, 33, 41 that `kh2-anim` already names) case 0x14 calls
`FUN_00188ad0(object, params[0], params[1])`. `params[0]` is the `cmd` id; `params[1]` (only if the trigger has
at least two params) is a position index passed on as the target offset. FUN_00188ad0 builds a target record
(FUN_0018f490), and offers the command in the player's command manager slot 1 with priority 100
(FUN_001bb3e0 when the offering object is the player, FUN_001bb3a8 otherwise; the wrapper refuses while the
player flag 0x2000000 is set). It also sets player flag 0x20000 and stores the id in player +0x50c.
Other ids in the same switch: 0x19 and 0x1a set object flags 0x100000 / 0x200000 and store `params[0]` in
+0x50c as well (a reaction-related window), 0x1d..0x20 and 0x22 set further object flags. [V; the meaning of
those flags is not traced]

Other offers come from code: the Auto-form and Auto-limit abilities (above), the Final-form-ready prompts
(commands 0x1c7 and 0x1c8 from FUN_001ce240), and map scripts (callers 0x2db670, 0x2e0348, 0x2f1988 of
FUN_001bb3a8). [V that they call the offer API; their conditions not traced]

### What runs [V in part]

The player executes the chosen command through the generic executor FUN_00177b18 -> FUN_00177c20:
1. increment the command's saved use counter (`ReactionCount`);
2. apply `CmdAction` (1 or 2 as in the command table section);
3. hand the command id to the command target object with FUN_0016b420 (the object that offered it). The target's
   script reacts to it; the player's own motion for the reaction is driven from that target's side. **The
   target-side handler (the enemy BDX event that receives the command) and the paired player motions were not
   traced** [GAP]; this is the part that needs the BDX VM (work package 1E).

## Abilities

### The ability list [V]

An ability is an `item` row with `Type` 19. 145 rows on this disc.

- `Id`: the item id; the same id is what `plrp` item lists, `fmlv.Ability`, `lvup.*Ability` and the
  party ability slots store (slot = id, bit 15 = equipped).
- Ability bit [V]: `Flag1 | Rank << 8` (item +4 as a u16). FUN_001c0f40 sets this bit in the character's ability set
  (status +0x1cc, a bit set of at least 6 u32) for every applied ability; every other system tests the bit.
  Several ids share a bit when they are the same ability on different characters or the "Dummy" duplicates.
- `StatEntry` (item +6): high byte is the AP cost [I: matches the known in-game costs for every ability checked],
  low byte a kind [V from FUN_001c0e50 and FUN_001c1340]: 0 support (applied also in forms), 1 party-only
  (Donald/Goofy/others), 2 action (Guard, Slash moves, Auto-form abilities), 3 growth (High Jump, Quick Run,
  Aerial Dodge, Glide).
- `Command` (item +0x10): the command the ability turns into. Set for the four Auto-form abilities (drive
  commands with Execute 4).
- `Name` / `Description`: ids into `msg/us/sys.bar` with bit 15 set (`kh2_tables::msg`).

### How abilities apply [V]

FUN_0019f920 rebuilds a character's battle state from the saved party record: base stats from the equipment items
(`item_stat` through FUN_001a0c08 / FUN_001a67b0, items of type 2..0xd use `StatEntry`; types 0xe..0xf use item +4),
then FUN_001c0d88 initialises the ability block, then FUN_001a0c08 adds each equipped item and
`FUN_001c0e50` applies abilities: the character's 64 ability slots (bit 15 set; see forms for the form rule) and
the 16 form slots. `FUN_001c0f40(block, id)` applies one ability; `FUN_001c1340` then derives the growth levels.

Ability block (status +0x1cc) fields, with the defaults `FUN_001c0d88` sets (a default of 1.0 is the float):

| offset | type | default | meaning (inferred from ability descriptions unless marked) |
|---|---|---|---|
| +0x00 | u32[8] | 0 | ability bit set [V] |
| +0x20..0x23 | u8[4] | 0 | growth levels 1..4 (0 = none) for High Jump, Quick Run, Aerial Dodge, Glide [V, FUN_001c1340; the bits per level are listed at 0x37bda0] |
| +0x28 | u8 | 3 | ground combo length |
| +0x29 | u8 | 3 | air combo length |
| +0x2a | u8 | 1 | finishing moves per combo |
| +0x2b | u8 | 1 | experience multiplier count [I] |
| +0x2c | i8 | 0 | defence bonus in a pinch [I] |
| +0x30 | f32 | 0 | Draw radius |
| +0x34 | f32 | 1.0 | MP regeneration multiplier after depletion |
| +0x38 | f32 | 1.0 | drive gauge gain while MP-charging |
| +0x3c | f32 | 1.0 | form timer drain multiplier [V use in FUN_00177788] |
| +0x40,+0x42,+0x44 | u16 | 100 | Fire, Blizzard, Thunder damage percent |
| +0x46,+0x48,+0x4a | u16 | 0 | Combo Boost, Air Combo Boost, Reaction Boost damage bonus |
| +0x4c | u16 | 0 | Item Boost (healing item bonus) |
| +0x4e | u16 | 0 | Berserk Charge bonus; also added to the strength used in damage during MP charge (FUN_001a0610) [V] |
| +0x50 | f32 | 0 | drive gauge restored per damage taken |
| +0x54 | f32 | 0 | MP restored per damage taken |
| +0x58 | f32 | 1.0 | revive heal multiplier (Hyper Healing) |
| +0x5c | f32 | 1.0 | limit duration multiplier |
| +0x60, +0x64 | f32 | 0 | munny/HP/MP orb drop bonus (Jackpot), item drop bonus (Lucky Lucky) |
| +0x68 | f32 | 1.0 | summon timer drain multiplier [V use in FUN_00177788] |

Abilities with a stat effect (`FUN_001c0f40`). Each also sets its ability bit. "step" is a field of the sstm
record (offset in brackets) [V]:

| id | name | effect |
|---|---|---|
| 162 | Combo Plus | +0x28 += 1 |
| 163 | Air Combo Plus | +0x29 += 1 |
| 393 | Finishing Plus | +0x2a += 1 |
| 394 | Negative Combo | +0x28 -= 1 and +0x29 -= 1 |
| 390 | Combo Boost | +0x46 += step [0x130] |
| 391 | Air Combo Boost | +0x48 += step [0x130] |
| 392 | Reaction Boost | +0x4a += step [0x134] |
| 395 | Berserk Charge | +0x4e += step [0x11c] |
| 396 | Damage Drive | +0x50 += step [0x138] |
| 397 | Drive Boost | +0x38 += step [0x13c] |
| 398 | Form Boost | +0x3c *= [0x140] |
| 399 | Summon Boost | +0x68 *= [0x16c] |
| 400 | Combination Boost | +0x5c *= [0x158] |
| 401 | Experience Boost | +0x2b += 1 |
| 405 | Draw | +0x30 += [0x12c] |
| 406 | Jackpot | +0x60 += [0x15c] |
| 407 | Lucky Lucky | +0x64 += [0x160] |
| 408, 409, 410 | Fire, Blizzard, Thunder Boost | +0x40 / +0x42 / +0x44 += [0x14c] |
| 411 | Item Boost | +0x4c += [0x164] |
| 412 | MP Rage | +0x54 += [0x150] |
| 413, 421, 422 | MP Haste, Hastera, Hastega | +0x34 += [0x120], [0x124], [0x128] |
| 414 | Defender | +0x2c += [0x148] |
| 419 | Hyper Healing | +0x58 *= [0x154] |

All other abilities only set their bit and are read where they act: for example Guard, the slash and
aerial moves, Scan, Aerial Recovery, Leaf Bracer, Magic Lock-On, No Experience, Second Chance, Once More, Auto Limit,
Auto Change, Auto Healing, the Auto-form abilities (offer commands, see forms), Donald/Goofy/other member
abilities. To implement one of them, find its bit consumers by the pattern
`bit(FUN_001768a8(object))` (FUN_001768a8 returns the status +0x1cc block). The many readers of that function are
listed in the decompile index (callers of FUN_001768a8).

### Growth abilities [V]

- Groups, bits per level at 0x37bda0: High Jump, Quick Run, Aerial Dodge, Glide, each four levels (LV1..MAX).
  FUN_001c1340 stores the highest set level in block +0x20 + group; FUN_001c13b8(block, group) reads it.
- `pref_fmab` row `level - 1` gives the physics for that level (FUN_001cc1a0). Group 0 jump uses the first float
  (FUN_0016bd98, motion 0xc9); group 2 air dodge uses fields 1 and 2 (AirDodgeHeight, AirDodgeSpeed) with motion 0xca
  (FUN_0017bad0). The sheet already names all 17 fmab fields.
- Obtained from form levels (`fmlv` displayLevel) and from `lvup` rows (`SwordAbility`/`ShieldAbility`/
  `StaffAbility` fields), see the existing lvup decoder.

### Other tables that feed abilities [V]

- `plrp` (per character base rows): `Items` lists the starting items and abilities per party member (ids with
  bit 15 set are equipped).
- `pref_sstm` first 21 floats are physics (already decoded); the rest of the record is the tuning in `sstm_full`.
- `vtbl`, `bons`, `przt`, `lvup`, `lvpm`, `enmp`: unchanged by this work package.

## Summons

`sumn` (64-byte records, layout confirmed: lookups in FUN_001d4c68..FUN_001d4cf8 step 0x40 from table +8) [V]:

| field | meaning |
|---|---|
| `Command` | the summon command (cmd Execute 72), looked up by FUN_001d4ca0 (returns the 1-based record index). |
| `Item` | the charm item (type 20) whose `Command` is the same id. |
| `Entity1` / `Entity2` | objentry ids spawned for the summon (a second entity for Peter Pan's companion; 0 if none). [I from the object names] |
| `LimitCommand` | the summon's own limit command. Three of the four equal a `limt` row's `Command` with a summon `Summon` byte; the fourth (Genie) names a command that exists but is not a `limt` row: Genie's limits are the four per-form rows [V] and the default id's use is not traced [GAP]. |

Execution [V, FUN_001cf210 via Execute 72]: play the summon-start motion state 0x8a, spawn the summon task object
(FUN_001db868), pay `cmd.Cost` drive points (FUN_001a0320), register the active summon (`DAT_00340c88` byte 5 =
1-based sumn index, read back by FUN_001d4cf8 and FUN_001d4d28). While a summon is active the gauge mode is 3 and
the timer drains with the Summon Boost multiplier (section "The form timer"). The summon level curve is `fmlv`
form 0, with experience `DAT_00340e4c` and level `DAT_00340c88` byte 6 (FUN_001d4ec8).

## Decompile index

| address | what |
|---|---|
| 0x1b6f48 | cmd lookup by id |
| 0x1cf030 | player command executor (Execute switch) |
| 0x1ba2c0, 0x1ba568, 0x1b70b8, 0x1b7290 | command availability, drive affordability, command flags, DisableForm |
| 0x1d3b90 | fmlv lookup (form, level) |
| 0x1d3f88, 0x1d3da8 | add form experience, level-up |
| 0x1d3bd0 | form level cap |
| 0x1d4180, 0x1d4278, 0x1d4348 | form choice with Anti and Final rolls |
| 0x1b4c28, 0x1b44c8, 0x1b4740, 0x1b4bd0 | form change entry, task constructor, task body, timer start |
| 0x1a04b0, 0x1a0480, 0x1a0358, 0x1a0290, 0x1a0320, 0x1a0420 | form timer start, mode set, timer add, drive fraction add, points spend, whole drive points of the timer |
| 0x177788, 0x178748, 0x1788d8, 0x1ceda8 | per-frame update, damage callback, drive gain, auto revert |
| 0x1cf2c8, 0x196ad0, 0x1a7118 | form object, form keyblade, memt record 0 |
| 0x1d2040, 0x1d1ce0, 0x1d1fe0, 0x1b28a0, 0x1b3e38, 0x1b2c10, 0x1b2e08 | limits: lookup, usability, summon test, execution, availability, task, resource load |
| 0x188030, 0x188ad0, 0x1bb3e0 | range trigger switch (reaction offer is case 0x14), offer, command manager slot write |
| 0x177b18, 0x177c20, 0x187f88 | generic command execution, counters |
| 0x1c0f40, 0x1c0e50, 0x1c0d88, 0x1c1340, 0x19f920, 0x1a0c08 | ability apply, apply all, block init, growth levels, build battle state, add equipment |
| 0x1d4c68..0x1d4e00, 0x1cf210 | summon table access and execution |
| 0x1cc040, 0x1cc140 | pref loader (plyr, fmab, prty, magi, sstm) |

## Gaps (not found)

- Per-form Strength/Magic/Defense changes and which stat tables a form uses.
- The reaction command's target-side handler and the player motions it starts (needs the BDX VM).
- World-outfit variants of form objects (`P_EX100_TR_F`, `_WI_F`, `_NM_F`): which code picks them.
- The change-effect resource for each form (FUN_0016e430 argument) and the transformation motion names.
- The special form-field values 6 and 0xb on the player object (used by the command and gauge code as
  "no cost / forced gauge" cases), and the global flag of FUN_001903d8.
- Which command-menu fields (`Priority`, `Mark`, `Range`, `Group`) order the menu.
- The ELF-embedded tables that have no disc source: form growth chain (0x37c556), growth bit groups (0x37bda0),
  auto-form ability order (0x37c350), form to item (0x37b420), drive orb table (0x37b540). Their values are
  recoverable from the executable at run time if a later work package needs them as data.
- `ptya`, `patn`, `vbrt` (party and voice tables) were not decoded here.
