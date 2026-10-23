# Script host and enemies: contract

Ladder step L7. Ledger system name `script-host`. Crates: `kh2-bdx` (disassembler, VM, runtime with threads and messages, host table reader,
`names.rs`, `signatures.rs`, `reaction.rs`, `MockHost`), `kh2-ard` (spawn groups and area scripts that name the enemies), `kh2-app`
(`enemy.rs`, `enemy_ai.rs`, `enemy_host.rs`, `enemy_obj.rs`, `npc.rs`, `friend_host.rs`, `summon.rs` hosts).
Sheet ids: `game.enemies`, `game.ai_bdx`. Scenarios: `scenarios/script-host/` (none yet).

Status: the BDX bytecode interpreter runs bit for bit against the game's own interpreter (kh2-ee oracle); about 54 enemy kinds are in
`enemy::KINDS` and run their own object scripts; 273 of the 812 distinct host functions that scripts on the disc call are named. The L7 gate (every
enemy object of ObjectType 4/21, bosses included, soaked headless with no `stub` or `missing` host arm) is far away: the area survey ranks the
absent kinds.

## What the system does
- BDX is the game's script bytecode: BAR entries of type 3 inside `obj/*.mdlx` (885 files), `*.bar` mission and limit files (283), `magic/*.mag`
  (79), `ard/*.ard` (43), and the standalone `00common.bdx` that the game loads at start (`FUN_00194d50`). The 16-bit instruction set and the
  calling convention are `FUN_001d7cb0`: arguments go into a 16-word buffer, the native function receives a pointer and writes word 0.
- The runtime: threads, messages and queues (`FUN_0016c8d8`, `FUN_0016c680`, `FUN_0016c3f0`, `FUN_00187170`), the mode manager
  (`FUN_001869b8` and the functions around it), events (6 damage received, 0xb/11 a reaction command accepted, 0xd/0xe event begin and end, 0x13 clash).
- The host: tables in the executable at `0x35bce8` (group pointers of `{function, flags}` entries; low 16 bits = argument count, bit 30 =
  returns a value). Groups 0, 1, 2, 4 to 8 and 10 are in the file; group 3 (Gummi ship) and 9 (Gummi menu) and entries 4.0 and 4.1 are empty in it.
  `kh2_bdx::Host::call(&Call)` is the single seam; vector math and the random generator (`seed * 0x10dcd`) live inside the crate.
- Enemies: choosing the next kind from the data (`ObjectType` 4 and 21 of `00objentry` that the area scripts spawn), waves, the app's object
  `Obj`, the host call mapping, hit reactions, clash, messages between objects (the Dusk's surprise), drops, death, one doc section per kind.

## State and owners
| state | where in the original | owner in the port |
|---|---|---|
| script object (fields as the wrappers touch them) | object `+0x..` (see "The object as the script sees it") | `enemy_obj.rs` `Obj`, one field per game field with offset and writer in the doc comment |
| host call dispatch (`imp`, `approx`, `stub`, `missing` arms by group) | tables at `0x35bce8` | `enemy_host.rs` (`group0`, `group1`, `group2`, `other`); `KH2_BDXLOG=1` prints the per-run table |
| thread tree, per-object queues | `FUN_0016c928` and friends | `kh2_bdx::Runtime` |
| kinds the app runs | enemy kinds with their `enmp` row | `enemy::KINDS`, `enemy::Kind` constants |
| global script memory | zero at start (the game's zeroing is unverified) | `kh2_bdx` |

## Entry points in the original
| address | what |
|---|---|
| `FUN_001d7cb0` | BDX instruction dispatch and host call convention |
| `FUN_0016ca28`, `FUN_0016ca10` | group 3 table, run-time registration of groups 3 and 9 |
| `FUN_0016e8c0` case 0x11 | create object (host 1.0) |
| `FUN_0016b738` -> `FUN_001a3cb0` | drop (host 1.92) |
| `FUN_001869b8`, `FUN_00186900`, `FUN_00186cf0` .. `FUN_001871d0` | the mode manager |
| `FUN_00194d50` | loads `00common.bdx` |

## Open gaps
- 540 host functions are unnamed (NPC library group 5, mission and area scripts group 4, boss scripts); several named ones only describe a field
  read or written by the wrapper; groups 3 and 9 cannot be run from the table (`bdx-scripts.md` "Gaps and open questions").
- Not ported in the runtime: per-object thread count `+0x488`, the order of the action thread against the script update, children of remote
  queue entries.
- The area survey (289 areas, 0 panics, 0 unimplemented host call) ranks 88 absent objects of ObjectType 4/21 (23 base kinds, 41 world variants,
  8 bosses, ...); 23 areas have no map file; area opcode 0x16 `load_file` is the only opcode read and not ported (`area-survey.md`).
- The motion controller (blend queue, airborne variants), spawn records from the event data, target records at the lock-on part, hit stop
  (`ModeFrozen` is always false) (`enemy-ai.md` "Gaps"); the L7 target "all 812 host functions" has no tracked list yet (the ledger will).

## Where the details are
Section numbers in references ("section 5", "20.14") are the old chapter numbers that still stand in the headings of the files below.

| file | KB | covers (section numbers are the old chapter numbers) |
|---|---|---|
| [area-survey.md](area-survey.md) | 40 | Method; Result; Fix made; Collision groups; Area opcode 0xb; Ranked gaps; +2 more |
| [bdx-format.md](bdx-format.md) | 18 | Where the files are; File format; Instruction set; How the game runs a script; Threads, messages and queues; Host interface |
| [bdx-host-functions.md](bdx-host-functions.md) | 59 | Named host functions |
| [bdx-scripts.md](bdx-scripts.md) | 21 | Host groups the executable's table lacks; What Shadow; What Soldier; Reaction commands in scripts; Verification; Gaps and open questions |
| [enemies-al-n.md](enemies-al-n.md) | 29 | The second enemy: N_EX570_BTL; The fourth enemy: N_EX500_BTL; The fifth enemy: N_EX600_BTL; The enemy M_AL020_FIRE; The enemy M_EX020_RAW; The enemy M_AL020_ICEE; +1 more |
| [enemies-core.md](enemies-core.md) | 42 | Choosing the next enemy from the data; Next kinds; The Dusk; Waves; Acceptance run; Follow-ups; +2 more |
| [enemies-m-ex0-4.md](enemies-m-ex0-4.md) | 30 | The sixth enemy: M_EX420; The enemy M_EX110; The enemy M_EX210; The enemy M_EX050; The enemy M_EX060; The enemy M_EX120; +1 more |
| [enemies-m-ex5.md](enemies-m-ex5.md) | 37 | The enemy M_EX520; The enemy M_EX510; The enemy M_EX560; The enemy M_EX580; The enemy M_EX590; The enemy M_EX570; +2 more |
| [enemies-m-ex6.md](enemies-m-ex6.md) | 37 | The enemy M_EX650; The enemy M_EX660; The enemy M_EX690; The enemy M_EX600; The enemy M_EX620; The enemy M_EX630; +3 more |
| [enemies-m-ex7.md](enemies-m-ex7.md) | 32 | The enemy M_EX700; The enemy M_EX770; The enemy M_EX730; The enemy M_EX750; The enemy M_EX780; The enemy M_EX740; +1 more |
| [enemies-m-ex8-9.md](enemies-m-ex8-9.md) | 39 | The third enemy: M_EX930, the Dancer; The enemy M_EX880; M_EX900; The enemy M_EX950; The enemy M_EX910; The enemy M_EX920; +2 more |
| [enemy-ai-host-calls.md](enemy-ai-host-calls.md) | 41 | Host call mapping |
| [enemy-ai.md](enemy-ai.md) | 20 | How it runs; The object as the script sees it; Event 11: reaction commands; Events, death, leaving; Movement; Verified and inferred; +5 more |

Index of all systems: [../../README.md](../../README.md).
