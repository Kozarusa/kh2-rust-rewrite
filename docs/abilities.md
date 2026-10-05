# Abilities (work package 2D)

What the game does with abilities, what `crates/kh2-app/src/abilities.rs` implements, the hook points
for `sora.rs` and `drive.rs`, and the gaps. Marked **[V]** verified (read in the game's own code or
checked against the disc data), **[I]** inferred, **[GAP]** not found. Function names are addresses in
`SLUS_210.05`. No game values are in the repository; the numbers are read from the disc at run time.
Builds on `docs/player-data.md` (ability rows, the stat table) and `docs/moveset-selection.md` (the
gates of the attack table).

## What is implemented

`AbilitiesPlugin` (one line in `main.rs`) inserts the `Abilities` resource, built from the disc
(`item` type 19 rows, `item_stat`, `sstm`, `lvup`), and logs with `KH2_ABILITYLOG=1`.

- `Block` is the game's ability block (`status + 0x1cc`) with the defaults of `FUN_001c0d88`;
  `Block::apply` is `FUN_001c0f40` (all 27 stat effects of player-data.md, plus the ability bit).
- `Abilities` holds Sora's 64-slot ability list (learned and equipped), his equipped items (Kingdom Key
  at the start) and the optional form list, and rebuilds the block in the game's order
  (`FUN_0019f920`, `FUN_001c0e50`, `FUN_001c1340`): equipment abilities first, then the equipped list
  (only support abilities while a form is active), then the form's own list, then the growth levels.
- Queries: `has(id)`, `has_bit(bit)`, `bits()` (the low 128 bits, the format of `Sora::abilities` and
  `Chain::abilities`), `ground_combo()`, `air_combo()`, `finishers()`, `growth(group)`,
  `ap_max()`, `ap_used()`, `equipment_stats()`, `block()` (every stat field).

## Verified facts

- The ability list and the ability bit [V, player-data.md]. Item `Flag1 | Rank << 8` is the bit.
- Equipment grants abilities [V, `FUN_001a0c08`]: for each equipped item the `item_stat` row (found by
  `item.StatEntry`) adds Attack, Magic, Defense and AP to the status (`+0x188`, `+0x18a`, `+0x18c`,
  `+0x18e`), multiplies seven resistance bytes by `byte / 100`, and calls `FUN_001c0f40` with the row's
  `Ability`. The Kingdom Key row (item 41, stat entry 80) grants Defender on this disc.
  `FUN_0019f920` runs this for the party record's weapon and armor words, the form record's weapon,
  the accessory list (count byte at record `+0x10`, ids from `+0x14`) and the item list (count at
  `+0x11`, ids from `+0x24`).
- The ability list is 64 u16 slots starting at party record `+0x54` (player-data.md says `+0x50`;
  `FUN_001c0e50` reads `*(record + 0x50 + 4)`), bit 15 = equipped. `FUN_00187868` appends to the first free
  slot, so a second copy of an ability is a second slot and applies twice. `FUN_001a1d50` counts copies.
- `FUN_001c0e50`: the equipped list applies in full when no form record is active; with a form record
  only abilities whose `StatEntry` low byte is 0 (support). The form record's 16 slots always apply.
- Growth levels [V, `FUN_001c1340`, table at 0x37bda0 checked against the executable]: five bytes per
  group (bits 2..5, 6..9, 10..13, 14..17 and a fifth entry that is bit 0, which nothing sets). The
  stored level is 1..=4 (LV1, LV2, LV3, MAX); the loop looks from the highest entry down. The app derives the
  same bits from the kind 3 ability rows sorted by bit (checked equal). `FUN_001c13b8` (the reader the
  controller uses) returns 5 instead of the stored level while `FUN_00190868` is true: world 0x12 area 0x17
  or bit 0x2000 of `DAT_0035d268`. So `fmab` row 4 (level 5) is only a forced maximum and a learned
  ability never reaches it. `sora.rs`'s `Growth` allows 0..=5.
- Level-ups [V, `FUN_001b52f8`, `FUN_001b53d0`]: when `lvup` row `level - 1` has `Exp <=` the character's
  experience the level rises, and the new level's row grants one ability chosen by the record's weapon
  set byte (record `+0xe`: Sword, Shield or Staff column). The ability is added **unequipped**
  (`FUN_001a18d8(id, char, 0)`) and announced; the player equips it in the menu. `Ap` in `lvup` is the
  cumulative AP of the level (`FUN_0019f920` adds the equipment and the record's AP bonus byte).
- New game start [V, `FUN_001c0cb0(0)` from `FUN_00187d20`, `FUN_001c0b80`]: every `plrp` row with `Id` 0 is
  applied: HP, MP, then `Items[32]` through `FUN_001a18d8` (bit 15 = equipped). Sora's row has no items: HP
  20, MP 100, nothing learned. The `Id` 1 rows (Sora with HP 60, AP 80 and Guard, Upper Slash, Scan, Aerial
  Recovery, Trinity Limit and Finishing Plus equipped) are never applied by this caller (only the constant 0
  is passed); their selector was not found.
- `Dodge Roll` is not an ability on this disc (no row; the growth abilities are High Jump, Quick Run,
  Aerial Dodge and Glide only). It appears to be a later (Final Mix) addition.

## Default equipment

The data gives Sora nothing learned at the start of the story: only the Kingdom Key and the ability it
grants. The app, though, has always started Sora with Guard so the guard motion works (`start_abilities`
in `sora.rs`). To keep behaviour, the default adds Guard (id 82, bit 1) as equipped and says so: it is an
**app baseline, not a data fact**. `KH2_ABILITY_START=data` drops it. Where the real game gives Sora
Guard and the other early abilities (a story script, the unreached `Id` 1 rows, or a save default) was not
found [GAP].

`KH2_ABILITIES=bit,bit` keeps its meaning: each bit equips the ability row that owns it (21 adds a combo
hit, 100 equips Aerial Finish), a bit no row owns is set raw. `KH2_ABILITY_IDS=id,...` equips by item id.

## API for sora.rs and drive.rs

```rust
let ab: Res<Abilities>;
ab.bits()            // u128 for Sora::abilities and Chain::abilities
ab.ground_combo()    // Chain::max_ground, replaces BASE_HITS_GROUND + has(COMBO_PLUS) - has(NEGATIVE_COMBO)
ab.air_combo()       // Chain::max_air
ab.finishers()       // Chain::max_finishers
ab.growth(0..4)      // 0 High Jump, 1 Quick Run, 2 Aerial Dodge, 3 Glide: 0..=4
ab.has(id) / ab.has_bit(bit)
ab.block().combo_boost, .defense_bonus, .form_drain, .mp_regen, ...   // every stat field
```

The game counts equipped copies, so `ground_combo()` and the others are exact; `bits()` is only a set.

## Hook points needed (not done: sora.rs, drive.rs and the others are another worker's)

1. `sora.rs` `start_abilities()` and `Sora::abilities`: replace by `Abilities::bits()`, refreshed whenever
   `Abilities` changes (`resource_changed`). `Sora::chain()` takes `max_ground`, `max_air`, `max_finishers`
   from the three combo queries and drops the four `ABILITY_*` constants.
2. `sora.rs` `setup()` `Growth` from `KH2_GROWTH` / default: set from `Abilities::growth(0..4)` (keep
   `KH2_GROWTH` as an override). `drive.rs` writes `Growth` in a form from the form's display level; the
   equivalent here is `Abilities::set_form(Some(list))` with the form's granted abilities
   (`rules::granted_abilities`) and the growth ability of the form's chain (94..97 for Valor, 98..101 Wisdom,
   102..105 Master, 106..109 Final, levels by display level), and `set_form(None)` on revert. Then
   `Growth` follows `Abilities` and the drive code does not have to save and restore it.
3. `drive.rs` Form Boost: `granted_abilities` / `form_boost` could read `Abilities::block().form_drain`
   (exact: applied per copy, in a form also the player's support abilities).
4. Damage (`enemy.rs` / `data.rs::damage`): Combo Boost, Air Combo Boost, Reaction Boost, Berserk Charge,
   the element boosts and Anti's multiplier read `block()`; where they are used in the damage routine was
   not traced [GAP], so no formula is claimed here.
5. HUD or menu code: `ap_used()` / `ap_max()` for an ability menu.

## Gaps

- Where the real game gives Sora his early abilities (see above), and the selector of `plrp` `Id` 1.
- Dodge Roll: no row on this disc.
- Combat abilities that only set their bit (Guard, the slashes, Scan, Aerial Recovery, Leaf Bracer,
  Magic Lock-On, Second Chance, Once More, Auto Limit, Auto Change, Auto Healing, ...): the bit is
  provided (`has`, `has_bit`) but their consumers (`bit(FUN_001768a8(object))` readers) were not ported here.
  Only the attack table's own gates (docs/moveset-selection.md) read these bits in the app.
- The consumers of the stat fields (damage bonuses, MP regeneration, drive gain, drop bonuses, experience
  multiplier, Defender's low-HP rule) were not traced or ported; the app computes the numbers exactly but nothing
  reads them yet.
- Resistance multipliers of equipment (`item_stat` fields at `+8..+0xe`) are not exposed.
- `ap_used()` as the sum of the ability AP costs is inferred (the menu code that checks it was not read);
  the level-up growth that adds abilities (`lvup` Sword/Shield/Staff) is not simulated: there is no
  experience system.
- The special `FUN_001c13b8` override (growth level 5 in world 0x12 area 0x17) is documented, not wired.
- Ability bits above 127 do not fit the `u128` the attack table uses; `Block::bit` has all 256.
- Nobody played the result: tests and logs only.
