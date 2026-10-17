# Abilities (work package 2D)

What the game does with abilities, what `crates/kh2-app/src/abilities.rs` implements, the hook points
for `sora.rs` and `drive.rs`, and the gaps. Marked **[V]** verified (read in the game's own code or
checked against the disc data), **[I]** inferred, **[GAP]** not found. Function names are addresses in
`SLUS_210.05`. No game values are in the repository; the numbers are read from the disc at run time.
Builds on `docs/player-data.md` (ability rows, the stat table) and `docs/moveset-selection.md` (the
gates of the attack table).

## What is implemented

`AbilitiesPlugin` (one line in `main.rs`) inserts the `Abilities` resource, built from the disc
(`item` type 19 rows, `item_stat`, `sstm`, `lvup`), logs with `KH2_ABILITYLOG=1`, and keeps the
controller's view (`sora::Abilities`, `sora::Growth`) in step with the block (`sync_controller`).
`attacker_bonuses` is the damage routine's use of the element, combo and reaction boosts.

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

## Hook points (wired by WP-2B2 unless marked)

1. Done: `sora::Abilities` is the controller's view (the bits and the three combo lengths), written by
   `sync_controller` (FixedUpdate, after `Sim::Weapon` and before `Sim::Sora`, on `resource_changed`) from
   `bits()`, `ground_combo()`, `air_combo()`, `finishers()`; `start_abilities()` and the `ABILITY_*`/`ID_*`
   constants of sora.rs are gone (`KH2_ABILITIES` goes through this module). `Sora::chain()` is untouched:
   it still reads `combo_block()` and `bits()`.
2. Done: `Growth` follows the block's growth levels through the same system; `KH2_GROWTH` is kept as the
   `GrowthOverride` resource and applies only while no form is active (in a form the form's growth ability
   is the one the block holds). `drive.rs` calls `set_form(Some(list))` on the swap with the `fmlv` grants
   up to the level plus `AbilityRules::growth_ability(form - 1, display level)` (the chain of 0x37c556 found
   through the growth bits of the kind 3 rows: 94..97, 98..101, 102..105, 106..109), and `set_form(None)` on
   revert; it no longer saves and restores `Growth`.
3. Done: the form timer drain reads `block().form_drain` (`drive::rules::drain_per_tick`), so a copy of
   Form Boost the player equips (a support ability) shortens the drain as well as the form's own grants.
4. Done where the damage routine was read (`FUN_001a0610`, WP-2B2): after the generic formula and Anti's
   multiplier, `FUN_001a0a18` scales a Fire, Blizzard or Thunder attack (`atkp.Element` 1, 2, 3) by the
   block's percent for that element (`+0x40`, `+0x42`, `+0x44`), and `FUN_001a0aa8` adds, with the
   first matching of: the Combo Boost bit (block word 3 bit 0x40 = ability bit 102) and `atkp.Kind` bit 0,
   `combo_boost` percent times the actor's combo counter (+0xb04) capped at the ground length
   (`FUN_00176be8`); the Air Combo Boost bit (103) and `Kind` bit 1, the same with the air length
   (`FUN_00176c40`); the Reaction Boost bit (104) and `Kind` bit 2, `reaction_boost` percent. Each step
   rounds up (`(x * p + 99) / 100`). Ported as `attacker_bonuses`, called by `enemy::receive_hits`; the
   bits 102..104 are checked against the rows in the tests. Berserk Charge is read in the same routine as
   an addition to the attack stat while MP-charging (`FUN_001a05d0`), not ported: the app has no MP charge.
5. Open: HUD or menu code: `ap_used()` / `ap_max()` for an ability menu.

## Ability readers (WP-A1)

Bits from the disc's rows (test `combat_ability_bits_are_the_rows_bits`): Leaf Bracer 402 bit 114 (block
word 3 mask 0x40000), Second Chance 415 bit 127 (word 3 0x80000000), Once More 416 bit 128 (word 4 0x1),
Auto Limit 417 bit 129 (word 4 0x2), Auto Change 418 bit 130 (word 4 0x4), Auto Healing 420 bit 132
(word 4 0x10). Readers were found by the callers of `FUN_001768a8` (the block) and by the offsets
`+0x1d8`/`+0x1dc` (`mips_scan.py --jal 0x1768a8`, `--load-off 0x1d8 --load-off 0x1dc`).

| ability | status | rule [V unless marked] |
|---|---|---|
| Second Chance | ported, wired | `FUN_00185a68` (HP step of the receive handler `FUN_00175340`): for a non-heal hit of damage `d` on status HP `h` with status minimum `m` (+8), when `h - d <= m` it asks the actor's vtable +0x58; for Sora (class vtable 0x35c908) that is `FUN_00176d78`: Second Chance and `h >= 2` answers yes; then `d = max(h - m - 1, 0)`, so HP ends at `m + 1`. Port `abilities::survives_lethal_hit`, `lethal_hit_damage`; called in `sora::receive_hits`. |
| Once More | ported, wired | the same routine's second case: Once More and `FUN_00185a08` (actor +0x140 set and motion id +0x154 in 10..=39, the damage motions). The app keeps the motion id it restarts in a hit reaction (`Sora::damage_motion`, cleared outside `Hurt`/`Blown`) [inferred: the app's reaction motion ids stand for +0x154]. The third case, the base class's `FUN_00185be0` (invincibility flags +0xabc/+0xac0/+0xac4, +0x588), is not modelled (answers no). |
| Leaf Bracer | rule ported, receiver wired, trigger needs `magic.rs` | not read by engine code: every `CURE_*.mag` script (test `ability_bit_tests_in_scripts`: the only 12 uses of host 1.343 = `FUN_00170c28` on the disc, all bit 114, word 0x1ed of Cure's event 20) tests the caster's bit 114, and when the cast's target is the caster (host 1.130 answers 0) calls host 1.91 (`FUN_00172fe8` -> `FUN_00175aa8`: actor +0xb00 = 60.0) and starts a thread (routine word 0x2b8 with 563, 64) [routine not read]. `FUN_00175ab0` lowers +0xb00 by the frame step; while it is above 0 `FUN_00175ad8` (vtable +0x60) answers untouchable and `FUN_00175340` drops every hit but heals (attack types 5, 6). Port: `abilities::leaf_bracer_frames`, `Sora::leaf_bracer` (lowered one per tick [inferred: tick = frame step]), `Sora::start_leaf_bracer`; `receive_hits` drops hits while it runs. The cast start is in `magic.rs` (Cure's event 20), outside this WP: it does not call `start_leaf_bracer` yet. docs/magic.md read this branch as "a target that is not the caster"; the IsZero of host 1.130 says the caster [V: script listing]. |
| Auto Healing | gap (rule read, nothing to wire) | `FUN_0017d4b8`: a timer `DAT_0035cc08` lowered by the frame step; at 0, for party slots 1..3 (`FUN_001a7200`) whose member is not one of the two field partners (`FUN_0017d400` over `FUN_0017cd08(0..1)` answers 0), with bit 132 in that member's status block (`+0x1dc & 0x10`), the HP method `FUN_001a0140(status, 1, 0, 0)` (+1 HP); then the timer is reset to `sstm +0x168`. So it heals members who are out of the field party. The app keeps no status for a member outside the field party: not ported. Called from `FUN_001b74e0` (0x1b77e4) [caller not read]. |
| Auto Limit | ported (WP-AL1; rule read by WP-A2) | `FUN_001d25e0(player, wanted command)`, called by a party member's reaction-command logic (`FUN_0019c670` -> `FUN_0019d430`, which hands the result to the reaction offer `FUN_0019c3c8`): only while `FUN_001d2460` holds, for every `limt` row (table `DAT_01d6a948`, 0x40 bytes each) whose member byte +9 is not 1 and that passes `FUN_001d1ce0` and `FUN_001d1ea0`, the member object `FUN_0017d400(+9)` must have bit 129 in its own block (word 4 0x2), stand within `DAT_0037c414` of the player (horizontal distance, y zeroed) and the row's command (+0x30, `FUN_001b6f48`) must have a status below 4 (`FUN_001ba2c0`); the wanted command wins, else one of the qualifying rows by the LCG `DAT_003716a0 * 0x10dcd`. So Auto Limit is the partner's ability that makes his limit appear as a reaction command. Ported by WP-AL1: `limit::auto_limit_candidates` / `auto_limit_pick` and `reaction_cmd::auto_limit_offer` offer the partner's limit as the reaction capsule, an accepted offer starts it through `limit.rs`; the gates and gaps are in docs/limits-reactions.md "Auto Limit (WP-AL1)". |
| Auto Change | rule read, not ported (WP-A2) | `FUN_0017ce60` is a party member's KO method (friend vtable +0x5c, docs/party.md): with bit 130 (block word 4 0x4) in the KO'd member's block, `FUN_001b42f0` (no limit lock `DAT_01d5d6b0`, `FUN_001a71e8(.., 3)` not 0x10, `0x35fec0` clear) and `FUN_0017d0f0` (the member is still in play, +0x7f8 bit 4 clear, and `FUN_001a7698` answers 0) it calls `FUN_001b4170`: a task (`FUN_001b41e8` -> `FUN_001b3f78`, end `FUN_001b4260`) that swaps the member for a reserve one, sets `0x35fec0` bit 2 and the member's +0x108 bit 0x100. Not ported: the app has the KO (`party.rs`) but no reserve members to swap in. |
| Scan | ported (gate), wired to a log; the HUD gate needs `hud_target.rs` | `FUN_0019b4a8(marker)`, run by the target marker's set `FUN_0019b3f0` and retarget `FUN_0019b5e8` after `FUN_0019b548` releases the old gauge: binds the target's HP gauge (`FUN_00154688` with the target's HP record `marker +0x24` of 12 bytes) only when the player (+0x28) has a status block, bit 19 (block word 0 0x80000), the target (+4) is in the actor list (`FUN_001b80e8`, chain from `DAT_01d5d7a0` through +0x8b4), its +0x108 has neither 0x80000 nor 0x10000000 (`FUN_0019a440`) nor 0x4000, and it has a status block (+0x48c). Port `abilities::scan_binds_gauge`, oracle `scan_gate_matches_the_game`. In the app the lock press logs the result; the gauge itself is drawn by `hud_target.rs` without the bit (outside this WP). |
| Aerial Recovery | ported (WP-5A), blend fixed (WP-A2) | `FUN_001758b0` is not a hit handler: it is the blown actor's button press, run by the command executor `FUN_00174cf8` before every command. In a blow mover (mover flags 0x40, not 0x800), after `plyr` BlowRecoveryTime ticks (+0xf8 against +0x130 +0x58) and with +0x7f8 bit 4 clear, command flag 2 (the jump button) with bit 20 (block word 0 0x100000) runs `FUN_001759b8`: motion 40 with blend 0 (`mtc1 r0, $f20` at 0x1759c8, $f12 = $f13 = 0 for both requests `FUN_0017e220` and `FUN_0017e378`) and the mover's method +8 with a vector from +0x10 and `plyr` +0x50/+0x54 [launch not decoded]. Already ported as `Moveset::recovery` (test `recovery_press_follows_fun_001758b0`) and `sora::blown_press`; the app played motion 40 with a blend of 4, now 0 (`BLEND_ATTACK`). |

Evidence: `cargo test -p kh2 --test ability_oracle` runs the game's `FUN_00185a68` with Sora's
`FUN_00176d78` in kh2-ee for 1400 cases (HP, minimum, damage, both bits, motion ids) and compares the HP
change with the port; `cargo test -p kh2 --test ability_scripts -- --nocapture` lists the host 1.343 uses
(`KH2_ABILITY_SCRIPT_LIST=1` prints the listing around the first). Harness: `KH2_NO_PARTY=1
KH2_ABILITIES=127 KH2_ABILITYLOG=1 KH2_MAGIC_HURT=8:3 KH2_WAVE_GROUP=b_44 kh2-game tt04` logs
`abilities: Second Chance keeps Sora at 1 HP: HP 2 damage 1 (atkp 17, damage motion None)`. Once More was
not seen in a run: the app's own 36-tick invulnerability after every hit (`receive_hits`, not from the
game) keeps a second hit from landing during most reactions.

## Gaps

- Where the real game gives Sora his early abilities (see above), and the selector of `plrp` `Id` 1.
- Dodge Roll: no row on this disc.
- Combat abilities that only set their bit (Guard, the slashes, Auto Limit,
  Auto Change, ...): the bit is provided (`has`, `has_bit`); readers located for some of them are in
  "Ability readers" below, the others were not ported. Second Chance, Once More and Leaf Bracer are
  ported (WP-A1).
- The consumers of the stat fields other than the form drain and the damage boosts (MP regeneration, drive
  gain while charging, Damage Drive, MP Rage, drop bonuses, experience multiplier, Defender's low-HP rule,
  Berserk Charge) were not traced or ported; the app computes the numbers exactly but nothing reads them yet.
- Resistance multipliers of equipment (`item_stat` fields at `+8..+0xe`) are not exposed.
- `ap_used()` as the sum of the ability AP costs is inferred (the menu code that checks it was not read);
  the level-up growth that adds abilities (`lvup` Sword/Shield/Staff) is not simulated: there is no
  experience system.
- The special `FUN_001c13b8` override (growth level 5 in world 0x12 area 0x17) is documented, not wired.
- Ability bits above 127 do not fit the `u128` the attack table uses; `Block::bit` has all 256.
- Nobody played the result: tests and logs only.
