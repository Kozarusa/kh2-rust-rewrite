# Combat and effects: contract

Ladder step L3. Ledger system name `combat-effects`. Crates: `kh2-fx` (PAX/DPX effect programs, `prog_more.rs`), `kh2-tables` (`player.rs`
typed views), `kh2-app` (`combat.rs`, `fx.rs`, `fx_lens.rs`, `reaction_rules.rs`, `player_reaction.rs`, `clash.rs`, `prize.rs`,
`abilities.rs`, `party_battle.rs`).
Sheet ids: `game.combat`, `fx.effects`, `fx.lens`, `game.prizes`, `game.sora.abilities` (that row also names drive forms, limits, summons and
reaction commands: those are in `drive-limits-summons`). Scenarios: `scenarios/combat-effects/` (none yet).

Status: hit detection, the damage formula, hit reactions, the clash rule, prizes and the ability block are ported; three particle programs
and the hit, death and system effects run; the PAX runtime is far from complete (L3 exit gate: every effect program on the disc runs).

## What the system does
- Hit detection from the collision shapes of `objectcollision` plus motion triggers; the damage formula (`atkp` power, strength minus defense
  with caps, resistances) and the hit point (`FUN_0018ce18`).
- Reactions: the wall recoil is a generic rule of the actor class (`FUN_001a2cc0` -> `FUN_0018c8e0`, rows with `Flags` bit 0), the clash of
  two attacks, the victim reaction classes through `patn`. The original has no hit-stop rule (`hit-stop.md`): do not invent one.
- Effects: hit effects (`FUN_0018d090`), the Heartless death effect, owner-bound system effects, bound effects that follow the owner, sprite
  texture coordinates and the weapon trail, the lens/distortion draw.
- Prizes: HP, MP and drive orbs, munny, item boxes; the pool of `0x100` entries made by the `PRIZE` manager (`DAT_0035e7a0`), launch, update,
  pickup (`FUN_001a39b0`, table `0x37b540`), Master Form experience, dropped items, messages.
- Abilities: the 27 stat effects (`FUN_001c0f40`), the ability block at status `+0x1cc`, equipment-then-list-then-form-then-growth order
  (`FUN_0019f920`, `FUN_001c0e50`, `FUN_001c1340`), readers of Second Chance, Once More, Leaf Bracer, Berserk Charge, Combination Boost.
- Player data: drive-form, limit, reaction command, ability and summon table layouts, Sora's status block, stat growth at level-up (WP-ST3).

## State and owners
| state | where in the original | owner in the port |
|---|---|---|
| hit record, attack volume flags (8, 0x10) | volume flags set by `FUN_0018ca78` | `combat.rs`, `reaction_rules.rs` |
| actor reaction state (+0x588 flags, +0x160 bits) | actor | `enemy_obj.rs` `Obj`, `sora.rs` |
| ability block | status `+0x1cc` | `abilities::Block` |
| Sora's status block | built by `FUN_0019f920` from the party record | `Progress::member_vitals`, `abilities.rs` |
| prize pool | manager `+0x9f4`, `0x100` x `0x60` bytes | `prize.rs` |
| effect program table | `0x362b10` (47 `pppKeMvYpEff`, 157 `pppKeShpTail3X`, 238 `pppSnoMdlDttNorm`) | `kh2-fx` |

## Entry points in the original
| address | what |
|---|---|
| `FUN_001a2cc0`, `FUN_0018c8e0`, `FUN_0018ca78` | attack volume sweep against terrain, wall hit |
| `FUN_0018d090`, `FUN_0018ce18` | hit effects, hit point |
| `FUN_001a3cb0`, `FUN_001a3b28`, `FUN_001c5288`, `FUN_001a39b0` | prize drop, launch, update, pickup |
| `FUN_001b74e0` | per-actor time update (no hit-stop route reaches it) |
| `FUN_001c0f40`, `FUN_001c0d88` | ability block apply, defaults |
| `FUN_001b51e0`, `FUN_001b52f8` | experience add, level-up check |

## Open gaps
- Hit-stop: none found in the executable or the four overlays (`hit-stop.md` "What is not ruled out").
- Enemy wall recoil and clash rules for overlay classes; the meaning of hit record `+0x24`, stat block `+0x1c8` (`hit-reaction.md`).
- Effects: experience payout of `FUN_001b25d0`, camera-bound bind (flag `0x20000`), depth fade of sprites, packet budget (`effects-damage.md`).
- Prizes: `FUN_00143840` mover not ported, the drop point's line test, instant-collection flags (`prizes.md` "Gaps").
- Abilities: where the story gives Sora his early abilities, combat abilities that only set a bit, stat-field consumers not wired
  (`abilities.md` "Gaps"); form growth chain tables with no disc source (`player-data.md` "Gaps (not found)").
- Experience, levels and bonus rewards (max HP/MP growth) are in `player-data.md`; the full PAX opcode set has no doc yet.

## Where the details are
Section numbers in references ("section 5", "20.14") are the old chapter numbers that still stand in the headings of the files below.

| file | KB | covers (section numbers are the old chapter numbers) |
|---|---|---|
| [abilities.md](abilities.md) | 22 | What is implemented; Verified facts; Default equipment; API for sora.rs and drive.rs; Hook points; Ability readers; +2 more |
| [effects-damage.md](effects-damage.md) | 54 | The three ported programs; Hit effects; Heartless death; Message type and hook points; Owner-bound system effects and their end; An element's DPX entry is found by number; +11 more |
| [hit-reaction.md](hit-reaction.md) | 30 | Short answer: why Sora recoils from walls; Attack against attack; ReflectHitSound; Victim reaction; Hit-stop; Implementation status; +1 more |
| [hit-stop.md](hit-stop.md) | 16 | How an actor's time advances; The three freeze/slow mechanisms and who d; What was ruled out; Conclusion for the combat code; What is not ruled out |
| [player-data.md](player-data.md) | 45 | How to look at the data; Vocabulary the game uses; Command table; Drive forms; Limits; Reaction commands; +7 more |
| [prizes.md](prizes.md) | 35 | From the drop to the pool; The update; The pickup; The look; Tail calls of FUN_001a3cb0; Master Form experience; +8 more |

Index of all systems: [../../README.md](../../README.md).
