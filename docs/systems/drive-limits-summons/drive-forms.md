# Drive forms (work package 2B)

What the drive-form system does in the game, what `crates/kh2-player/src/drive.rs` implements of it, and
what is still open. It builds on `docs/systems/combat-effects/player-data.md` (read that first for the table layouts). Each
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
- Loading the form: the form's player object (model, motion set, attack table) and its weapons by hand
  (model from the moveset's `went` row for the item, motion set from `wmst`), all read from the disc on
  first use (`DriveRules::files`). World outfit variants (`_NM`, `_TR`, `_WI`) are selected by the game's
  own merge rule (below). Hand 0 holds the equipped keyblade; hand 1 the form record's keyblade, taken
  from the inventory when the form was obtained (`rules::take_form_keyblade`, below).
- The base body (WP-2B2): `sora::setup` builds base Sora from the same merge (`DriveRules::base_files`:
  word 8 + outfit of the merged record, hand 0 from `went`/`wmst`), so the world's outfit of the base body
  appears too; the plugin's `P_EX100`/`W_EX010` rigs are reused when the merge names them.
- The weapon list (WP-2B2): `Sora::weapons` holds the weapons in hand order, the body swap spawns them on
  their hand sockets (`sora::spawn_weapons`), and every controller system plays, syncs, shows and trails
  all of them; Anti has none, so nothing has to be hidden.
- The HUD hook: `DriveGauge` (`hud_drive.rs`), rewritten every tick, plus a one-line debug readout. The drawn gauge reads the state through `hud_gauge::feed_drive` (docs/systems/ui/contract.md, WP-G9): the readout's resource is not what the HUD draws. (F9 or
  `KH2_DRIVE`).
- Hits Sora lands add `DriveDrain` of the attack row to the gauge, or take it off the form timer.
- The form's ability list (WP-2B2): on the swap the drive code gives the ability system the form record's
  list (the `fmlv` grants up to the level plus the growth ability of the form's chain at the display
  level, `Abilities::set_form`) and clears it on revert; the ability system derives the stat block, the
  controller's bits and combo lengths and the `Growth` resource from it (docs/systems/combat-effects/abilities.md). The timer
  drain reads the block's form multiplier. The attack table (`Sora::moves`) is swapped with the body.
- HP and MP (WP-2B2): the timer start restores HP and MP to their maximum (below).

Harness (all optional): `KH2_DRIVE=<form id 1..5>` enters at `KH2_DRIVE_AT` seconds (default 4),
`KH2_DRIVE_REVERT_AT=<s>` reverts, `KH2_DRIVE_POINTS` (0..9; with `KH2_DRIVE` set the default is 9,
otherwise 0 as in a new game), `KH2_DRIVE_LEVEL` (form level, capped by the number of owned forms),
`KH2_DRIVE_OWNED=1,2,3` (default 1,2,3,4), `KH2_DRIVE_PARTY=both|donald|goofy|none` (default both),
`KH2_DRIVE_BATTLE=1` forces the battle flag, `KH2_DRIVE_ANTI_COUNTER`, `KH2_DRIVE_SEED`,
`KH2_DRIVE_WORLD=<id|wi|nm|tr>` (default 2), `KH2_DRIVE_OUTFIT=<0..3>` (the base body's outfit index,
default 0), `KH2_DRIVE_INVENTORY=<item ids>` (spare keyblades the forms take; default one Kingdom Key per
owned dual-wielding form, empty for the new-game state). Keys: F5 chooses the form, F7 enters, F8 reverts,
F10 adds a point, F11 fills the gauge. Entering Anti (`KH2_DRIVE=5`) skips the roll; the game only reaches
Anti through it.

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
- Anti form multiplies the damage Sora deals by `sstm[0x94]` (1.5) [V, `FUN_001a0610`]: applied in
  `enemy::receive_hits` through `rules::outgoing_damage`.
- No other form changes the outgoing damage [V negative, WP-2B2]: `FUN_001a0610` tests the attacker's form
  field only for 5 (with the player-class flag 0x80), and the only other per-form rule on its path is the
  attacker's ability block in its two last steps (`FUN_001a0a18`, `FUN_001a0aa8`: element, combo and
  reaction boosts, ported in `abilities::attacker_bonuses`). Final form has no damage multiplier; what
  changes its damage is its attack table and its abilities. The earlier steps `FUN_001a0898` (a global at
  `DAT_0033feb8` scales by 50, 150, 200 or 100 percent when the attacker or the victim has actor flag 4)
  and `FUN_001a0968` (`DAT_0035d268` bit 0x20000 with a counter from `FUN_001b8218`: a multiplier for
  enemies, a divisor for the player) are not form rules and not ported (the meaning of the two globals was
  not read).
- HP and MP at the timer start [V, `FUN_001a04b0`]: before the gauge mode is set, `FUN_001a0140(status,
  status+4, 0, 0)` calls the actor's method +0x78 with the maximum HP of part 0; for the player class
  (vtable at 0x35c9a8, read from the executable: +0x74 is `FUN_00178748`, the hit callback the doc already
  names, +0x78 is `FUN_001792f0`) that is the HP change with the Damage Drive hook, then `FUN_00176c98` and
  `FUN_00186238`, whose `FUN_001a0100` adds the amount and clamps at the maximum. So entering a form heals
  Sora fully. `FUN_001a0240(status, 0)` then sets MP to its maximum (`FUN_001a0210` -> `FUN_00186368` ->
  `FUN_001a0180` with the maximum at +0x184, clamped). The app sets both on the swap and logs them.

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
  `SpawnObject1..4`). This closes the "change effect resource" gap of `docs/systems/combat-effects/player-data.md`: there is no separate
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
  This closes the "outfit variant" gap of `docs/systems/combat-effects/player-data.md`. Checked: worlds 17, 13 and 14 load
  `P_EX100_TR_BTLF`, `P_EX100_WI_ULTF` and `P_EX100_NM_MAGF` and the screenshots show the different outfits.
- Motion set and attack table [V]: the object's `AnimationName` (the `_TR`/`_NM`/`_WI` objects share the base
  form's `.mset`) and its `NeoMoveset` (an index into `ptya`; the variants have their own rows).
- Keyblades [V]: the `wmst` table of `03system.bin` (row = `NeoMoveset`, two 0x20-byte names; `FUN_00184640`)
  names the right-hand and left-hand weapon motion sets, for example `W_EX010_BTLF_R` and `W_EX010_BTLF_L`.
  Valor, Master and Final hold two keyblades, Wisdom one, Anti none. Anti: [V, WP-5B, from the disc data via
  `explore_memt_went_wmst`] its moveset (6) has a filled `went` row (same row 66 as the other forms, giving
  `W_EX010` for the Kingdom Key) but BOTH `wmst` names are empty. `FUN_00184640` returns 0 for an empty name,
  so `FUN_00184190` never loads a motion set for that weapon; a weapon with no motion set is not drawn [I]
  (matches the game: Anti Sora shows no keyblade; seen in the app). `DriveRules::object_files` therefore gives a
  hand only when its `wmst` name is non-empty. Wisdom's hand 1 has an empty `wmst` name too, so it never shows. The `went` table (row = `NeoMoveset`, then
  one object id per weapon slot of the equipped keyblade item, `FUN_001845b0`) gives the weapon model.
- Which item each hand holds [V, WP-2B2]: the player object's constructor (`FUN_00176f88`, also
  `FUN_00176e00`) attaches hand 0 with `FUN_0016ef28(objentry, 0)` and, only while the form field is 1..5 or
  9, hand 1 with `FUN_0016ef28(objentry, 1)`. `FUN_0016ef28` resolves the hand's item with `FUN_0016ee98` and
  looks the weapon object up in `went` by the item's `Flag1 | Rank << 8` (`FUN_001845b0`, item +4).
  `FUN_0016ee98`: for an object whose `NeoStatus` is 1 or 0xe and hand 1, the item is word 0 of the form
  record of the object's `ObjectForm` byte (`FUN_00187f60(0x33fec0, form)` = record `0xc3c + 0x28 * form`);
  otherwise it is word `hand` of the character's party record (`FUN_00187d40`; word 0 is the equipped
  weapon). The attach (`FUN_001751b0` -> `FUN_00184190`) loads the hand's `wmst` motion set when the name is
  non-empty and otherwise leaves the weapon without one.
- The form record's keyblade [V, WP-2B2; closes the "initialisation not found" gap]: when a form item (type
  21) is added to the inventory, `FUN_001a1a18` calls `FUN_001a2010`, which resets the Anti counter and, for
  the form of the item's command (`cmd.Argument` 1, 3 or 4: Valor, Master, Final) while the record's word 0
  is 0, stores `FUN_001a20b0(2)`: the first item of type 2 (a keyblade) in item-table order whose inventory
  count is at least 1 (`FUN_001a1cc8`), one copy of which leaves the inventory (`FUN_001a1c30`). So a form's
  second keyblade is the lowest-id spare keyblade owned at the moment the form is obtained (in the story the
  keyblade given with the form), and 0 when none is spare: the hand then stays empty. Wisdom and Anti keep 0.
  The app has no inventory, so `KH2_DRIVE_INVENTORY` stands in (default: one Kingdom Key per owned
  dual-wielding form; the harness obtains the owned forms in id order), and `DriveState::form_weapon` is the
  record word per form. Checked against the disc: Valor with the Kingdom Key in both hands gives
  `W_EX010` with `W_EX010_BTLF_R` and `W_EX010_BTLF_L`; Anti's row gives no weapon.
- World variants of the keyblade [V negative, WP-5B]: the object table names `W_EX010_NM`, `_WI` and `_TR` (the
  `went` rows of the variant movesets point at them) but this disc has no `obj/W_EX010_{NM,WI,TR}.mdlx` (or `.a.us`):
  the names exist in the OpenKH list only. What the original does for the missing file was not traced. The app
  keeps the plain `W_EX010` model with the hand's motion set [I] (`drive::load_assets` and `sora::setup` both).
  Checked in the app for worlds 13, 14 and 17: the forms show both keyblades in the outfit.
- The weapon bones [I]: the attach passes `method +0x3c (0x16bc68) + hand + 1` to `FUN_00184190`, which
  stores it through `FUN_001a79a0`/`FUN_001a7808` as an index resolved by `FUN_0016fd18` in the resource
  manager (a handle counter, not a bone number); in a form hand 0 gets the constant 0x8fd instead. The
  bone the weapon follows was not reached on this path. The app keeps bone 178 (right, checked on screen)
  and 86 (left, the mirror, inferred). `objentry.WeaponJoint` (byte +7 of the row) is a candidate the
  attach path does not read directly.
- **Correction to `docs/systems/combat-effects/player-data.md`**: the `memt` "weapon" words 18..23 (`H_EX500_*`) are not form keyblades.
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
- A form level-up while the player is in that form (WP-7Bc; `FUN_001d3da8` upgrades the record's slots with
  `FUN_001d3f18` and then rebuilds the status block, `FUN_0019fb98`): `drive::refresh_form_list` rebuilds the form
  list at the new level (`drive::form_ability_list`, the same list the form entry builds) and hands it to the ability
  system, which recomputes the block at once. Out of the form nothing changes; the next entry builds the list from the
  saved level as before. Caller: `prize::apply_form_exp` (Master Form from the drive orbs, and the `FormExpGain`
  message for Valor, Wisdom and Final). Test `drive::tests::a_form_level_up_in_the_form_rebuilds_its_list`. The rules
  of the experience itself are in docs/systems/combat-effects/prizes.md ("Master Form experience").

## Hook points in sora.rs (for the orchestrator)

The nine hooks of WP-2B as they stand after WP-I1 and WP-2B2:

1. Done (I1): `State::FormChange` holds the controller while the change motions play.
2. Done (2B2): `sora::setup` builds the base body and keyblade from the merged `memt` record and the
   `went`/`wmst` rows (`DriveRules::base_files`), replacing the plugin's `P_EX100`/`W_EX010` rigs when the
   world names a variant.
3. Done (2B2): `Sora::weapons` is the weapon list; the sockets are `HandSocket(bone)` for every hand
   (`sora::spawn_weapons`, `follow_hand`), the controller plays the weapons through `WeaponCue`, and
   `sync_weapon`, the visibility rule and `fx::weapon_trails` run over the list. `drive.rs` has no socket or
   visibility code left.
4. Done (I1, reworked in 2B2): the controller's block is `sora::Abilities` (bits and combo lengths), written
   from the ability system's block (`abilities::sync_controller`), which the drive code feeds with the form's
   list (`Abilities::set_form`).
5. Done (2B2): with a form list the ability system applies only the player's support abilities
   (`FUN_001c0e50`), so Guard and the other action abilities drop out of the bits in a form.
6. Done (I1): the `Battle` resource.
7. Done (I1, extended in 2B2): Anti's multiplier and the block's damage boosts in `enemy::receive_hits`.
8. Open: companion absorption and MP charge are the party code's; the drive code only reads the
   `donald`/`goofy` flags. HP and MP restore at the timer start are done.
9. Open: the game does not revert on death (`FUN_001ceda8` tests the dead flag); `sora.rs` revives in place,
   which keeps the form.

Messages other systems can use: `FormChanged { from, to }` (written when the body is swapped),
`DriveRequest` through the `DriveQueue` resource (the field command menu pushes them, WP-DRV1).

Command menu (WP-DRV1, `items.rs` `drive_request`): choosing a form in the Drive list (list 3, `FUN_001bca70`: the forms whose item is
owned; a missing companion greys the entry, `FUN_001c77f0`) issues that form's command (`Execute` 4, no sub-menu, `Argument` the form id) and
the menu pushes `DriveRequest::Enter(form)`; Revert (`Execute` 5) pushes `Revert`. The original: Sora's command handler `FUN_001cf030`
(read from the instructions, no decompilation) takes `Execute` 4 and 5 itself, the other ids go to `FUN_00177b18` -> `FUN_00177c20`.
The entry's availability is `FUN_001ba568` (cost against whole points, no form running, companions), unchanged. Test
`drive_menu_selection_becomes_a_drive_request`. Harness (tt00, `KH2_DRIVE_POINTS=9`, keys `3:G,3.3:G,3.6:G,4:J,4.5:J`): log
`items: Valor Form chosen (Enter(1))`, `drive: enter Valor`, then Revert from the deck gives `items: Revert chosen (Revert)`, `drive: revert from Valor (command)`.
Gap: the body of `FUN_001cf030` was not decompiled here; whether it adds anything to the form start beyond the request is not checked [inferred equal].

## Gaps

- Transformation visuals and sounds: only what the triggers of A120 and A121 start (the existing effect and
  sound systems read them, and the forms' `.apdx` files are registered when a form loads). Not checked frame
  by frame.
- Which routine starts A121 on the new body (inferred from the motion sets).
- The bone each hand's weapon follows (the attach path ends in resource handles; 178 and 86 are kept).
- The inventory: the form keyblade rule is ported, but the app has no inventory, so the harness supplies
  the spare keyblades (`KH2_DRIVE_INVENTORY`).
- The outfit index of a world (byte 0 of the save block at `0x340c9c + 4 * world`): a new game's value is
  taken to be 0; its writer was not read. `KH2_DRIVE_OUTFIT` sets it.
- "Anti's partial MP drain" [closed by WP-6A: the original has none, see the third attempt below; searched twice before, not found, WP-5B]: no code ties MP to the form field. Searched:
  every reader of `sstm` (`DAT_00360130 + offset`, all 0x0c..0x16c): the Anti-related offsets are 0x8c (timer
  level, `FUN_001a04b0`), 0x90 and 0xa4 (counter drops), 0x94 (damage, `FUN_001a0610`), 0x98..0xa0 (Final roll
  chance, `FUN_001d4180`, `FUN_001d4348`); 0xa8 belongs to a movement routine (`FUN_00176050`). The MP writers:
  `FUN_001a0180` (adds to MP at +0x180; during an MP charge a positive add feeds the charge by `sstm[0x7c]`
  instead), `FUN_001a0210`, `FUN_00186368`, `FUN_001a0240`, `FUN_001a05a8`, and their callers (`FUN_001a04b0`
  refills HP and MP at the timer start; the player constructors `FUN_001cda78`/`FUN_001cdb98`;
  `FUN_00118730`; `FUN_001a12a0` item use; `FUN_001ac130`; `FUN_0017d4b8` the periodic MP tick; `FUN_001be500`).
  None tests the form. Not ported; no invention.
  Third attempt (WP-6A), a different method: a scan of the executable's instructions themselves (all
  executable segments of `SLUS_210.05`, so it also covers code the decompilation lacks, such as Sora's
  command handler `FUN_001cf030`): (a) every `jal` to the MP writers `FUN_001a0180`, `FUN_001a0210`,
  `FUN_001a0240`, `FUN_001a05a8` and the actor-level `FUN_00186368` (MP add with the "not enough" branch
  `FUN_00186440`): 15 call sites, all inside known functions: `FUN_00118730`, `FUN_001769e0` (cost of
  +0xaf4), `FUN_00176c98`, `FUN_00186368`, `FUN_00194c08`, `FUN_001a0240`, `FUN_001a04b0` (the refill at
  the form timer start), `FUN_001a12a0` (items), `FUN_001a39b0` (the pickup table at 0x37b540: HP, MP and
  drive per prize kind), `FUN_001ac130`, `FUN_001b28a0` (a command's MP cost, `FUN_001b3f00`),
  `FUN_001be500`, `FUN_001cda78`, `FUN_001cdb98`; none reads the form field +0xb50 or the gauge mode;
  (b) every store instruction with offset 0x180..0x183: the status-block ones are the initialisers
  `FUN_0019f4a8`, `FUN_0019f5d8`, `FUN_0019f920`, the writer `FUN_001a0180` and `FUN_001a04b0`; the rest
  store to other structures (stack frames, map and render code in 0x0013..0x0016 and 0x0020..0x0023).
  Sora's per-tick drive work (`FUN_00177788` at +0xc of his class table 0x360410: the timer drain
  through `FUN_001a0358`; `FUN_001ceda8` at +0x14: the revert when the timer is empty) writes no MP.
  Conclusion [V for the direct writers]: the original has no separate MP drain for Anti form; MP moves
  only by the costs, pickups, items, the periodic tick (`FUN_0017d4b8`) and the refill at the form
  start, in every form alike. (A writer through a computed address other than base + 0x180 would not
  be found by this scan.)
- The drive-gauge charge of commands 455 and 457: wired in WP-6A, but it is **not Sora's** [V]. The three
  player-class tables in the executable: 0x360288 (`FUN_001cd678` ...), 0x360360 (built by `FUN_001cda78` and
  `FUN_001cdb98`; per-tick `FUN_001ce240` at +0xc, command handler `FUN_001ce518` at +0x98) and 0x360410 (built
  by `FUN_001ce718` / `FUN_001ce820`, the default class of `FUN_00178e40`; it holds `FUN_001ceda8`, the form-timer
  revert, and the drive-form code: Sora's). Sora's command handler at +0x98 is `FUN_001cf030` (not in the
  decompilation; read from the instructions): it takes command 0x48 and 4/5 and passes every other id,
  455 and 457 included, to the generic `FUN_00177b18` -> `FUN_00177c20` (use counter, event 11 to the target).
  The 0x360360 class is the one `FUN_001cdf48` spawns with `FUN_001cdb98` from the merged `memt` word 17
  (`FUN_001ce080` reads +0x22): object 91, `P_EX200`, the model whose motion set offers 457 (test
  `charge_class_object_is_memt_word_17`), i.e. Mickey [inferred name]. `FUN_001ce518` also hangs an end callback on
  motion 0xc1 (player +0x2d4 = controller +0x194 = `LAB_001ce710`, two instructions: `jr ra; sw zero, 0xb68(a0)`);
  the controller calls and clears it when any new motion starts (`FUN_0017dd38` -> `FUN_0017fac8`), so a hit or
  any other motion ends the charge with no motion 5. Blend of 0xc1: `DAT_0037c33c` = 8.0; motion 5 is started
  with blend 0 and motion 0 is queued behind it (`FUN_0017e378`).
  Port (`drive.rs`): `rules::ChargeRun` (accept, tick, callback; test `charge_run_wiring_455_457_and_the_end_callback`)
  and the system `charge_tick` (reads `PlayerReactionCmd::Charge`). Since the app's only player is Sora, the
  command is logged and dropped (`charge command ... not run: Sora's command handler FUN_001cf030 has no
  charge`). `KH2_CHARGE_ON_SORA=1` runs Mickey's handler on Sora's body as a harness (NOT the original's
  behaviour: Sora's motion 0xc1 is his magic motion A342, Mickey's is A122). Harness run
  (`KH2_REACTION_FAKE=455 KH2_REACTIONLOG=1 KH2_CHARGE_ON_SORA=1 KH2_REACTION_PRESS=3,3.5,4,4.5,5`, tt00): `charge
  started ... timer 120, wait 0`, `charge motion 0xc1 (A342, slot Some(772)), blend 8`, `charge +1 gauge unit
  (FUN_001a0290): 0 points + 0% -> 0 + 1% (timer 60, wait 3)` then one every 4 ticks, `charge timer ran out:
  motion 5 (A005) with motion 0 queued behind it`; a hit during the charge: `charge cancelled: the charge motion
  was replaced (A202)`. For Sora the faithful outcome is the generic accept path, which is `reaction_cmd.rs`'s
  (not changed here). The rule as found by WP-5B: `FUN_001ce518` sets the
  timer +0xb68 (120.0 at 0x37c340 for 455, with motion 0xc1, and +0xb6c = 0; 30.0 at 0x37c344 for 457, no motion,
  +0xb6c kept). `FUN_001ce240` each tick, while +0xb68 > 0: when the player's animator is at the end of its
  motion (`FUN_0017f400`: `FUN_0017fab8` false i.e. a motion present, blend time +0x34 <= 0 and the motion's
  length `<=` the current frame +0x30) it takes the frame unit off +0xb6c and, at <= 0, adds 1 gauge unit
  (`FUN_001a0290`, the same carry as `Points::gain`) and re-arms +0xb6c by `sstm[0xd4]` (4 on this disc);
  +0xb68 always drops by the frame unit and at <= 0 is cleared and motion 5 is played. Ported as the pure
  `drive::rules::Charge` (tests `charge_*`, `sstm[0xd4]` read from the disc); one unit is 1/100 of a point.
  The app's "at its end" ignores the blend part of `FUN_0017f400` (the frame test alone).
- Master Form's orb experience now runs (`prize.rs` `master_form_exp`, WP-7Bb Part B); Valor, Wisdom and Final gains (`FUN_001d4078`, `FUN_001d40e0`) and the level-up side effects do not. Form experience, level-ups for the other forms are not run: the level is
  fixed per run (`KH2_DRIVE_LEVEL`); the form's ability list is rebuilt from the level on each entry.
- Orbs, Damage Drive and summon mode (gauge mode 3, `FUN_001cf210`) are not implemented.
- The camera during the change (`FUN_0016fb20`, `FUN_00170098`) and the HUD mode switch.
- The special form field values 6 and 0xb and the battle flag's `FUN_001a4e70`.
- Wisdom's menu swap (`FUN_001bc4c8`) and the Final-ready prompts (`FUN_001ce240`): the menu is not ported.
- The frame unit: 1.0 per 60 Hz tick is inferred (see above).

## Integration status (WP-I1, WP-2B2)

WP-I1 did in sora.rs, enemy.rs and drive.rs: (1) `State::FormChange` holds the controller during A120 and A121 (no input, hit tests off, keyblade shown) and drive.rs ends it; (5) the `Battle` resource replaces `Sora::battle` and is read by the drive code; (6) Anti's multiplier is applied in `enemy::receive_hits` through `drive::rules::outgoing_damage` (generic damage branch only, as FUN_001a0610 does).

WP-2B2 did: (2) the base-body merge in `sora::setup` (`DriveRules::base_files`); (3) the weapon list (`Sora::weapons`, `WeaponCue`, `HandSocket(bone)`, `spawn_weapons`; `fx::weapon_trails` over the list; drive.rs no longer owns a socket or a visibility rule); (4) the stat block is read: the drive code sets the form's list on the ability system, `abilities::sync_controller` writes `sora::Abilities` and `Growth` from the block, the drain reads `form_drain`, and `enemy::receive_hits` applies `abilities::attacker_bonuses` after Anti's multiplier; HP and MP are restored at the timer start; `reaction_cmd::offer` passes the object's form (`DriveState::object_form`) and the battle flag into `Ctx`.

Where the controller's attack-selection code is touched (WP-2G2's region in sora.rs): only the type of the `weapon` parameter of the `start_attack` and `request` closures (`&mut Animator` -> `&mut WeaponCue`, same `play`/`restart` calls) and the `wrig.mset.has(x)` reads, which now ask `WeaponRigs` (any weapon has the slot); every call inside the closures is unchanged.

### Verification status (WP-5B)

Run in the app (`tt00`, `KH2_DRIVE=1..5`, `KH2_DRIVE_AT=3`): the log shows the entered form, the chosen files, the
stat block values and the revert (`KH2_DRIVE_REVERT_AT`: `revert from Valor`, `form changed Valor -> base Sora`).
Screenshots LOOKED AT: Valor (two keyblades), Wisdom (one), Final (two, crossed), Anti (none), and Final in
worlds 13 and 17 (outfit changes, two keyblades after the fallback above). Master was checked from the log only
(`hand 0 W_EX010 (W_EX010_TRIF_R), hand 1 W_EX010 (W_EX010_TRIF_L)`). Base body per world (log): 2
`P_EX100`, 13 `P_EX100_WI`, 14 `P_EX100_NM`, 17 `P_EX100_TR`, each with the attack table of its own row.
Not verified: animation of the left hand's weapon in combat, the weapon bone choice (still [I]), audio.
