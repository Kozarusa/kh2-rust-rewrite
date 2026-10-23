# Party: contract

Ladder step L6. Ledger system name `party`. Crates: `kh2-party` (`party.rs`, `party_battle.rs`, `friend_ai.rs`, `friend_host.rs`, `mickey.rs`), `kh2-game-core` (`party_rules.rs`), `kh2-menu` (`party_switch.rs`), `kh2-hud` (`hud_party.rs`), `kh2-tables` (`memt` in `player.rs`), `kh2-bdx` (the members run their own BDX scripts).
Sheet ids: `game.party`, `game.friend_ai`. Scenarios: `scenarios/party/` (none yet).

Status: Donald and Goofy spawn from the area script, follow, warp, run their friend AI target styles 0 to 5 (the BDX scripts attached since WP-7P1),
use their specials, are rebuilt when the slot set changes, swap at a knock-out (Auto Change) and heal (Auto Healing). The world partners (Beast,
Auron, Mulan, Aladdin, Jack Sparrow, Simba, Jack Skellington, Tron, Riku) are not done; Mickey's rescue is decided in the game over
(`mickey.rs`) but stays switched off (`RESCUE_PORTED` is false).

## What the system does
- Data: `memt` (party member table merged per world, `FUN_001a6ff0`), the seven party presets (`FUN_001a7330`), the stand-in objects (`PLAYER`,
  `FRIEND_1`, `FRIEND_2`, `FUN_0017cd20`), the friends' motion sets, effects and BDX scripts.
- Spawn and despawn: slot bytes per world, refusal in forms and summons, load of model, motion set and weapons, rebuild when the set changes,
  slot 3 as a reserve member, no members in the Struggle, the jump record's party arrangement.
- The friend AI: 19 AI classes (`FUN_0010b780` picks one by NeoStatus; Donald 0/12, Goofy 1/13, ten partners 2 to 11, summons 14 to 18), action
  records, follow points and hysteresis thresholds (`FUN_00114078`, `FUN_001140c0`), the target styles, emergency and healing rules
  (`FUN_0010f490`, `FUN_0010f4d0`, `FUN_001a0be0`).
- Host group 10 (table `0x371810`, 60 entries) that the member scripts call; the specials (Goofy's turbo and blade, Donald's blizzard), the target
  search, the class 16 chooser, Auto Change, Auto Healing, Auto Limit.

## State and owners
| state | where in the original | owner in the port |
|---|---|---|
| member records (13 x `0xf4` bytes) | party block `0x33fec0`, `FUN_00187d40` | `Progress::member` |
| member vitals (HP, MP) | record `+4..+7`; `FUN_0019f920` builds, `FUN_001a0030` writes back | `Progress::member_vitals` |
| party slots per world and the hidden list | slot bytes (kept across area loads) | `party.rs` `PartySlots` |
| AI object (0x14 bytes, Goofy 0x30): owner, parameters, action count, records | AI classes at `0x34ea00`, base vtable `0x352e78` | `friend_ai.rs` |
| follow mode `+0xb78`, far counter, steer | member object | `party.rs` `PartyMember` |
| member walk and run speeds `+0x110/+0x114`, record `+0x12c` (the `prty` row of the NeoMoveset) | `FUN_00194438` | `party_rules.rs` |

## Entry points in the original
| address | what |
|---|---|
| `FUN_0010e140`, `FUN_00110468`, `FUN_0010b270`, `FUN_00111670` | party spawn and AI entry points named by the sheet |
| `FUN_001a6ff0`, `FUN_001a7330`, `FUN_0017cd20`, `FUN_001c77a8` | `memt` merge, preset, slot to object, members a form takes |
| `FUN_00114550` / `FUN_00114668` | Donald's / Goofy's AI constructors |
| `FUN_00112b28`, `FUN_00112b88`, `FUN_0010d0c0`, `FUN_0010d848`, `FUN_0010f118` | target choice and range tests (target choice and range tests of the friend styles) |

## Open gaps
- The world partners' friend AI classes 2 to 11. The "10. Unknowns" list in `party-friend-ai.md` is older than the status sections (style 0's
  target choice `FUN_0010d0c0` was read in WP-P3b, `party-built.md`): read both.
- Frame-trigger frames of the members' motions count from the motion's first frame, the app's readers count from frame 0 (`party-friend-ai.md`).
- A stored HP of 0 at area entry, the reserve swap's 8-frame fade, weapons, motion remap, battle gating, push-apart (`party-built.md`).
- The accepted Auto Limit offer runs the limit script but the partner objects and scene hosts are missing (`drive-limits-summons/contract.md`).
- Generic reaction commands and the member swap are shared with `drive-limits-summons`; the class 16 chooser is modelled partly and no member
  of the app has class 16 (`party-built.md` "20. Status: the class 16 chooser").

## Where the details are
Section numbers in references ("section 5", "20.14") are the old chapter numbers that still stand in the headings of the files below.

| file | KB | covers (section numbers are the old chapter numbers) |
|---|---|---|
| [party-built.md](party-built.md) | 46 | sections 11, 12, 13, 14, 18, 19, 20, 21, 20 |
| [party-data.md](party-data.md) | 27 | sections 1, 2, 3 |
| [party-friend-ai.md](party-friend-ai.md) | 40 | sections 4, 5, 6, 7, 8, 9, 10 |
| [party-specials.md](party-specials.md) | 32 | sections 15, 16, 17 |

Index of all systems: [../../README.md](../../README.md).
