# Drive forms (work package 2B)

What the drive-form system does in the game, what `crates/kh2-app/src/drive.rs` implements of it, and
what is still open. It builds on `docs/player-data.md` (read that first for the table layouts). Each
statement is marked **[V]** verified (read in the game's own code or checked against the disc data),
**[I]** inferred (fits code and data, no line states it) or **[GAP]** not found. Function names are
addresses in `SLUS_210.05`. No game values are in the repository: the numbers quoted here are examples
of what the code reads from the disc at run time.

## What is implemented

`drive.rs` is a plugin (`DrivePlugin`, one line in `main.rs`) with the game's rules as pure functions in
`drive::rules` and a runtime that drives Sora through his public fields (it does not touch `sora.rs`).

- Drive gauge: whole points, a 0..100 fraction, a maximum (`rules::Points`). Gain and spend follow
  `FUN_001a0290` and `FUN_001a0320` exactly (carry, borrow, a full gauge keeps fraction 100, paying from a
  full or short gauge clears the fraction).
- Entering a form (`DriveRequest::Enter`): the checks of the Drive command, the cost, the Anti and Final
  roll, the old body's change motion, the body swap, the form timer. Logs: `drive: enter ...`, `drive:
  loaded ...`, `drive: change motion ...`, `drive: <form> active ...`, `drive: arrival motion ...`.
- The form timer: started at the swap, drained every 60 Hz tick (`drive: <form> timer ...` once a second),
  reverts at 0 (`drive: revert from <form> (timer ran out)`), or by command (`DriveRequest::Revert`).
- Loading the form: the form's player object (model, motion set, attack table), its two keyblade motion
  sets and the keyblade model, all read from the disc on first use (`DriveRules::files`). World outfit
  variants (`_NM`, `_TR`, `_WI`) are selected by the game's own merge rule (below).
- The HUD hook: `DriveGauge` (`hud_drive.rs`), rewritten every tick, plus a one-line debug readout (F9 or
  `KH2_DRIVE`).
- Hits Sora lands add `DriveDrain` of the attack row to the gauge, or take it off the form timer.
- Growth ability levels in a form (High Jump, Quick Run, Aerial Dodge, Glide) are written to the `Growth`
  resource and restored on revert; the attack table (`Sora::moves`) and the keyblade set are swapped.

Harness (all optional): `KH2_DRIVE=<form id 1..5>` enters at `KH2_DRIVE_AT` seconds (default 4),
`KH2_DRIVE_REVERT_AT=<s>` reverts, `KH2_DRIVE_POINTS` (0..9; with `KH2_DRIVE` set the default is 9,
otherwise 0 as in a new game), `KH2_DRIVE_LEVEL` (form level, capped by the number of owned forms),
`KH2_DRIVE_OWNED=1,2,3` (default 1,2,3,4), `KH2_DRIVE_PARTY=both|donald|goofy|none` (default both),
`KH2_DRIVE_BATTLE=1` forces the battle flag, `KH2_DRIVE_ANTI_COUNTER`, `KH2_DRIVE_SEED`,
`KH2_DRIVE_WORLD=<id|wi|nm|tr>` (default 2). Keys: F5 chooses the form, F7 enters, F8 reverts, F10 adds a
point, F11 fills the gauge. Entering Anti (`KH2_DRIVE=5`) skips the roll; the game only reaches Anti
through it.

## Verified facts

### Costs, conditions and the form choice

- Cost [V, `FUN_001ba568`, `FUN_001a0320`]: the drive command's `Cost` must not exceed the whole points, and
  that number of points is spent (a full or short gauge also loses its fraction). On this disc the costs are
  3, 3, 4, 5 and 9 for Valor, Wisdom, Master, Final and Anti (`kh2 table drive_forms`).
- Availability [V, `FUN_001ba2c0` case 4, `FUN_001ba568`, `FUN_001c7970`, `FUN_001bc578`]: the Drive entry is
  shown only while no form is active (otherwise the menu shows Revert); no command lock is up (globals at
  0x35feb8, 0x35fec0 and bit 0x40000 of `DAT_0035d268`); the form's companions are in the party.
- Companions [V, `FUN_001c77a8`, `FUN_001c77f0`]: Valor needs Goofy, Wisdom needs Donald, Master, Final and Anti
  need both. A missing companion makes the command unavailable. (A global scale above 1.0, set only in
  special battles by `FUN_001b9d70`, skips the test; it is 1.0 normally.) Donald and Goofy are not in the
  app, so `DriveState` has two flags (default both present).
- Revert [V, `FUN_001bc578`]: in Anti form the Revert entry is marked unavailable while the battle flag
  (`FUN_001903d8`) is set. A timer that runs out reverts regardless (`FUN_001ceda8`).
- The form choice [V, `FUN_001d4180`, `FUN_001d4278`, `FUN_001d4348`]: choosing Final always gives Final (the Anti
  counter drops by `sstm[0xa4]`, 10 on this disc). Otherwise the Anti roll runs when Anti's companions are
  present: chance is the form 5 row of `fmlv` the counter has reached (counter 10 or more 25 percent, 5 or
  more 10 percent, else 0), compared with `LCG / 0x28f5c29` (0..99); the counter rises by one on every roll.
  On success, with the battle flag set, the Final roll runs when the Final item is not owned, the story
  flag 0x4842 is set and the points reach Final's cost: chance `min(sstm[0xa0], sstm[0x98] * sstm[0x9c]^counter)`
  percent (25, 50, 100 here), the counter rises on every roll, resets on success, and success gives the Final
  item and Final form. Otherwise Anti is entered and its counter drops by `sstm[0x90]` (5). The generator is
  `state * 0x10dcd` (u32 wrapping); its seed `DAT_003716a0` is runtime state, so the app seeds from the clock
  (`KH2_DRIVE_SEED` fixes it). Note that the cost spent is the rolled form's: an Anti roll with 3 points
  spends all 3 although Anti costs 9.
- Level cap [V, `FUN_001d3bd0`]: 2 plus the number of owned forms 1..4, and 7 instead of 6 (`player::form_level_cap`).

### The timer

- Start [V, `FUN_001a04b0`, `FUN_001a0480`]: when the body has been swapped, MP is refilled to its maximum
  (and MP charge ended), the gauge mode becomes 2 and the timer is `sstm[0x68] * (level + 3)` gauge units, one
  drive point being `sstm[0x68]` (600 here). For Anti the level is `sstm[0x8c] - 2` (3), whatever the form level.
- Drain [V, `FUN_00177788`]: per frame the frame unit `DAT_00356f40`, times `sstm[0x6c]` (0.5) while the battle
  flag is clear, times the ability block's form multiplier. The multiplier is 1 times `sstm[0x140]` (0.8) for each
  Form Boost the form has granted [V for the formula, I that a form that granted it twice applies it twice: the
  `fmlv` rows of Final grant Form Boost at levels 4 and 6]. The frame unit counts vertical syncs (1.0 per 60 Hz
  tick, 2.0 for a fixed 30 Hz step, `FUN_0014cdd0` and the constants at 0x379170 and 0x379174) [I]; the app
  uses 1.0 per tick. So a level 1 form lasts 2400 ticks (40 s) in battle and twice that outside, level 7
  6000 ticks (100 s). Checked in the app: `KH2_DRIVE_BATTLE=1` reverts after 40.1 s.
- Clamp [V, `FUN_001a0358`]: the timer stays in 0..total, and in Anti form a positive change is made negative.
- Whole points [V, `FUN_001a0420`]: `timer / sstm[0x68]`, one less while the timer is full. This is what the HUD
  shows as points left (`DriveGauge::time_points`).
- Hits [V for the code, I for which side]: `FUN_0018cf50` calls the owner's method +0x74 for a hit that lands,
  which for the party class is `FUN_00178748`: with the attack row's `DriveDrain` non-zero it adds it to the
  normal gauge (times the global scale, 1.0), or in a form takes `DriveDrain * sstm[0x74]` (1.2) off the timer.
  The victim flags 0x10 and 0x800 it tests are not modelled (every enemy counts as 0x10 only). Damage taken
  adds drive through `FUN_001792f0` only with the Damage Drive ability (not modelled). Orbs go through
  `FUN_001a39b0` (not modelled, no orbs in the app).
- The battle flag [V that it is a tri-state with hysteresis, `FUN_001903e8`, `FUN_00190550`; I for the
  meaning]: set by `FUN_001904e0` from the player and the enemies' awareness (`FUN_001a4e70`). The app uses
  `Sora::battle > 0` (the keyblade-out timer) in its place, or `KH2_DRIVE_BATTLE`.
- Anti form multiplies the damage Sora deals by `sstm[0x94]` (1.5) [V, `FUN_001a0610`]. Not applied yet: the
  damage call sits in `enemy.rs` (hook below).

### The change sequence

- `FUN_001b4c28` [V]: marks the current form at once (`DAT_00340c88` byte 4), starts the change task, plays
  motion id 0x34 (52) on the old body, zeroes velocity, sets player flag 0x100 and tells the companions the
  form absorbs. The task (`FUN_001b4740`) preloads the new form's files, waits for the old object's motion,
  creates the new player object (`FUN_001ce820`), then starts the timer (`FUN_001b4bd0`). Revert is the same
  task with form 0 and no motion on the old body.
- Motion 52 is `A120` in `P_EX100.mset` and every form's set has `A121` at id 53 (state 3 slot) [V from the
  files]. That A121 is what the new body plays on arrival is **[I]**: the call that starts it was not found
  (the new object starts with the start mode at +0xb6c, 1 for a normal entry, 2 for a revert, 3 when dead).
  The app plays A120, swaps, plays A121, then hands control back. In the app A120 takes 2.7 s.
- `FUN_0016e7e8`, which the task calls with the form, is **not** a transformation effect: it is a resource
  preload (`FUN_0016e430` queues the object's `.mdlx`, `.mset` and `.apdx` through the format strings at
  0x37a430 and 0x37a8c0, and the weapon files of the object's moveset, and the objects named by
  `SpawnObject1..4`). This closes the "change effect resource" gap of `player-data.md`: there is no separate
  per-form transformation effect resource; anything visible during the change comes from the triggers of
  motions A120 and A121 (effects and sounds through the objects' `.apdx` files).

### Files of a form

- The object [V, `FUN_001a6ff0`, `FUN_001a6f70`, `FUN_001cf2c8`]: record 0 of `memt` is not read raw. On each area
  load the game clears words 8..24 of its in-memory copy and overwrites them, in table order, with the
  non-zero words of every record that matches: key 0 or the current world id, first flag (if non-zero) set,
  second flag (if non-zero) clear. Words 12..16 are then the player objects of forms 1..5. The worlds with their
  own records are 13, 14 and 17, whose forms are the `_WI`, `_NM` and `_TR` objects (`P_EX100_TR_BTLF`,
  ...); the other records keyed 2 (Twilight Town) apply only after story flags, for the Roxas outfit. That the
  world ids 13, 14, 17 are Timeless River, Nightmare/Halloween Town and Space Paranoids is **[I]** from the model
  name suffixes; the app takes the world from `KH2_DRIVE_WORLD` (default 2, which merges no variant).
  This closes the "outfit variant" gap of `player-data.md`. Checked: worlds 17, 13 and 14 load
  `P_EX100_TR_BTLF`, `P_EX100_WI_ULTF` and `P_EX100_NM_MAGF` and the screenshots show the different outfits.
- Motion set and attack table [V]: the object's `AnimationName` (the `_TR`/`_NM`/`_WI` objects share the base
  form's `.mset`) and its `NeoMoveset` (an index into `ptya`; the variants have their own rows).
- Keyblades [V]: the `wmst` table of `03system.bin` (row = `NeoMoveset`, two 0x20-byte names; `FUN_00184640`)
  names the right-hand and left-hand weapon motion sets, for example `W_EX010_BTLF_R` and `W_EX010_BTLF_L`.
  Valor, Master and Final hold two keyblades, Wisdom one, Anti none. The `went` table (row = `NeoMoveset`, then
  one object id per weapon slot of the equipped keyblade item, `FUN_001845b0`) gives the weapon model. The
  weapon of hand 1 for the player is taken from the form record's first u16 (`FUN_0016ee98`).
- **Correction to `player-data.md`**: the `memt` "weapon" words 18..23 (`H_EX500_*`) are not form keyblades.
  They are 284-bone, 16-mesh models of Sora's own size, only read through the virtual object id 0x23c
  (`FUN_0016e208` case 0x23c, `FUN_00196ad0`): most likely the cutscene model of the current form [I]. The
  in-field keyblade is `W_EX010` (the equipped keyblade's object) with the form's own motion sets.
- The two virtual ids 0x236 (PLAYER, 566) and 0x23b resolve to the current form's object, or to the base object
  when no form is active (`FUN_0016e208`).

### Stats and abilities in a form

- No per-form Strength, Magic or Defense exists in the code paths read **[V negative]**. The battle state is built
  by `FUN_0019f920` from the member's save record and equipment and does not look at the form; the readers of
  the player object's form field (+0xb50) are, besides the drive code: command availability (`FUN_001ba2c0`,
  `FUN_001ba568`, `FUN_001b7290` DisableForm), Anti's damage multiplier and gauge sign (`FUN_001a0610`,
  `FUN_001a0358`), experience (`FUN_001d3f88`, `FUN_001d4078`, `FUN_001d40e0`, `FUN_001d4148`), the menu
  (`FUN_001bc4c8` swaps an entry in Wisdom, `FUN_001bc578`), `FUN_001a3848`, `FUN_0019c0f0` (Anti), `FUN_001cf348`
  (Anti skips a party routine). What changes with a form is therefore the object (model, motions, attack table
  and the attack rows its motions name), the ability list (support abilities of the player plus the form's own
  list, `FUN_001c0e50`) and the timer.
- Growth [V, `FUN_001c1340`, table at 0x37bda0]: the four groups have bits 2..5, 6..9, 10..13, 14..17 for
  levels 1..4 (a fifth entry per group is bit 0, which no ability sets). The form chain at 0x37c556 is the
  abilities 94..97, 98..101, 102..105, 106..109 for forms 1..4 (levels 1..4). So in a form the growth level is
  the form's `fmlv` display level (1..4) in the group `form - 1`, and no other growth ability applies. Note
  for `sora.rs`: its `Growth` uses 1..5 with 5 as MAX, but this path only ever reaches 4.

## Hook points in sora.rs (for the orchestrator)

`drive.rs` works through public fields; these are the places it would be cleaner, or necessary, to wire:

1. A `State::FormChange` (or equivalent) that holds the controller while the change motions play. Now
   `drive.rs` puts Sora in `State::Hurt` and keeps `invuln` up; `Hurt` ends when the body's motion finishes.
2. `setup()` should build the base body from the merged `memt` record (`rules::memt_merge`, words 8..11 by outfit)
   instead of the fixed `P_EX100`, and read the keyblade from `went`/`wmst` (`DriveRules::files` shows how). The
   base object per world is part of the same merge.
3. The right-hand socket (`HandSocket`, bone 178) and the weapon visibility rule are Sora's. The left hand
   (bone 86) is `drive.rs`'s own (`LeftSocket`, `follow_left`); `weapon_visibility` hides Anti's missing
   keyblade after `sora::tick`. A weapon list in `Sora` (a `Vec<Entity>`) would replace both.
4. The ability block of `FUN_001c0d88`: combo length, air combo length and finishers per chain
   (`MAX_HITS_GROUND`, `MAX_HITS_AIR`, `MAX_FINISHERS` are constants now). Forms grant Combo Plus and Air Combo
   Plus through `fmlv`; `rules::granted_abilities` lists what a form has granted.
5. `Sora::abilities` for a form: the form's attack tables have no ability gates (checked for Valor and Anti), so
   they work unchanged; the player's action abilities (Guard and the like) should be dropped in a form.
6. A battle flag: `Sora::battle > 0` stands in for `FUN_001903d8`. A shared resource would do for the drive
   code, the Anti revert rule and music.
7. Damage: Anti's multiplier (`Tuning::anti_damage`, 1.5) belongs where Sora's damage is computed
   (`enemy.rs` `receive hits`, `GameData::damage`); `drive.rs` exposes it as `DriveRules.tuning`.
8. `Sora.hp`/`mp` and the party: companion absorption (Donald or Goofy leaving the field while a form lasts)
   and MP-charge handling are the party code's; the drive code only reads `donald`/`goofy` flags.
9. Revive and death: the game does not revert on death (`FUN_001ceda8` tests the dead flag); `sora.rs` revives
   in place, which keeps the form.

Messages other systems can use: `FormChanged { from, to }` (written when the body is swapped),
`DriveRequest` through the `DriveQueue` resource (a command menu would push `Enter` and `Revert`).

## Gaps

- Transformation visuals and sounds: only what the triggers of A120 and A121 start (the existing effect and
  sound systems read them, and the forms' `.apdx` files are registered when a form loads). Not checked frame
  by frame.
- Which routine starts A121 on the new body (inferred from the motion sets).
- The left-hand keyblade: the game takes it from the form record's first item (`FUN_0016ee98`), whose
  initialisation was not found. The app gives both hands the equipped keyblade's model.
- Form experience, level-ups and the Master orb gain (`FUN_001d3f88` and friends) are not run: the level is
  fixed per run (`KH2_DRIVE_LEVEL`), and no abilities are granted to the controller.
- Orbs, Damage Drive and summon mode (gauge mode 3, `FUN_001cf210`) are not implemented.
- The camera during the change (`FUN_0016fb20`, `FUN_00170098`) and the HUD mode switch.
- The special form field values 6 and 0xb and the battle flag's `FUN_001a4e70`.
- Wisdom's menu swap (`FUN_001bc4c8`) and the Final-ready prompts (`FUN_001ce240`): the menu is not ported.
- The frame unit: 1.0 per 60 Hz tick is inferred (see above).
## Integration status (WP-I1)

Done in sora.rs, enemy.rs and drive.rs: (1) `State::FormChange` holds the controller during A120 and A121 (no input, hit tests off, keyblade shown) and drive.rs ends it; (4) the ability block is the `Abilities` resource (sora.rs): the player's bits (`KH2_ABILITIES`, until the ability system exists) plus the form's grants, which drive.rs writes at the swap and clears on revert; combo length, air combo length and finishers per chain come from `Abilities::combo_block` (Combo Plus, Air Combo Plus and Finishing Plus count per copy from a form); (5) the `Battle` resource replaces `Sora::battle` and is read by the drive code; (6) Anti's multiplier is applied in `enemy::receive_hits` through `drive::rules::outgoing_damage` (generic damage branch only, as FUN_001a0610 does).

Not done: (2) `setup()` still builds the base body and keyblade from the fixed P_EX100 and W_EX010 (the merged `memt` word 8 and `went`/`wmst` for the base are not wired; the outfit variants of the base body are therefore missing); (3) the weapon list in `Sora`: drive.rs still owns the left-hand socket (bone 86, `follow_left`, `sync_left`) and the visibility override that hides Anti's keyblade. Both need `Sora::weapon` replaced by a list in every system that reads it (sora.rs tick, receive_hits, receive_wall_hits, fx.rs weapon_trails, drive.rs).