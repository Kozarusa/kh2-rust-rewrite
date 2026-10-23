# Foundations: formats, data tables, rendering, oracle and research tools: contract

Ladder step L0 (foundations; Phase 0 and the file layers every system stands on). Ledger system name `foundations`. Crates: `kh2-iso`, `kh2-archive`,
`kh2-bar`, `kh2-ps2` (VIF1 unpack, GS swizzle), `kh2-tables` (generated from `sheets/tables.json`), `kh2-sheets` (the Spreadsheet Method generator and
preflight), `kh2-model` (MDLX, textures, UV scroll, MAP/SK0), `kh2-image` (imd/imz/tm2/2dd/2ld), `kh2-anim` (MSET, ANB, motion triggers, IK rig,
expressions), `kh2-ee` (the R5900 interpreter oracle), `kh2` (the command line: `validate`, `preflight`, `table`, `model`, `bdx`, `mset`, ...).
Sheet ids: `fs.iso`, `fs.archive`, `fs.bar`, `ps2.vif`, `ps2.gs`, `data.battle`, `data.system`, `data.objentry`, `render.model`, `render.texture`,
`render.uvscroll`, `ui.image`, `anim.motion`, `anim.triggers`, `anim.ik`, `anim.expressions`, `dev.ee`, `world.map`, `world.bob`, `ui.msg`,
`dev.assets`, `app`. Scenarios: `scenarios/foundations/` (none yet).

Status: every layer the game systems read from the disc exists and is checked against the user's ISO by `kh2 validate` (`systems.json` `validated`
column). The sheets list `kh2-msg` as the crate of `ui.msg` but no such crate exists (preflight reports it as a gap); message text is read by
`kh2_image::text` and `kh2_tables` `msg.rs`. The diff rig and the headless reference runtime (PLAN R10 and R17) are not built yet.

## What the system does
- Reads the disc: ISO9660, the `KH2.IDX`/`KH2.IMG` archive (hashes, LZ decompression, world sub-indexes, overlays), the BAR container, the
  `00battle.bin`, `03system.bin` and `00objentry` tables through typed rows generated from `sheets/tables.json` (values never in the sheets).
- Renders: VIF1 packets into VU1 memory, GS swizzle and CLUT addressing, skeletal MDLX models in rest pose with weights, model textures, the texture
  container footer (TEXA sprites, UVSC scroll), map geometry and sky placement, background objects (BOB/BOP).
- Animates: MSET slots with the state fallback, ANB interpolated and raw, motion triggers in global 60 Hz frames, the control rig (one and
  two bone IK chains, constraints), rig expressions.
- Proves: `kh2-ee` runs the game's own routines (user's `SLUS_210.05`) so ports are compared over many inputs (oracle tests); PCSX2 with PINE and
  the patched debugger server shows the original running (`pcsx2.md`, `dynamic-tracing.md`); `final-mix.md` records where `SLPM_666.75` differs
  (research only, RULES A3).

## State and owners
| state | where | owner in the port |
|---|---|---|
| table layouts and statuses | `sheets/tables.json`, `sheets/systems.json` (structure and status, never game values) | `kh2-sheets` preflight in `kh2-tables/build.rs` |
| tables the game finds at boot | `FUN_00193610` opens `03system.bin` and `00battle.bin`, looks tags up by `FUN_00168510` (strings `0x37aee0..0x37afc0`) | `kh2-tables` loaders; unfinished layouts in `tables-unfinished.md` |
| texture footer animations | `FUN_00132238` family (TEXA, UVSC objects) | `kh2-model`, `texanim.rs` in `kh2-app` |

## Entry points in the original
| address | what |
|---|---|
| `FUN_00193610`, `FUN_00168510`, `FUN_001af9e0` | boot table setup: open files, find an entry by tag and type |
| `FUN_0011e018`, `FUN_00132238`, `FUN_001324a0`, `FUN_001326e8`, `FUN_001333c8` | texture setup and footer objects (base, TEXA, UVSC) |
| `FUN_0011f560`, `FUN_00121a78` | map / model draw: region test, scroll, sprite upload |

## Open gaps
- Unfinished tables and placeholder field names (`tables-unfinished.md` "14. Proposed sheet edits", "Gaps"); `kh2-msg` missing as noted above.
- Rendering gaps are in `render.md` "Gaps" (tt04 roof rectangles, sky placement, wrapped texture entries); the visual audit of enemy models is a
  short section there.
- Rig, headless reference runtime, scenario folders, `sheets/functions.json` (the ledger) and the build-speed work belong to later Phase 0 packages.

## Where the details are
Section numbers in references ("section 5", "20.14") are the old chapter numbers that still stand in the headings of the files below.

| file | KB | covers (section numbers are the old chapter numbers) |
|---|---|---|
| [assets-viewer.md](assets-viewer.md) | 5 | Start; Keys; Log lines; Known limits |
| [dynamic-tracing.md](dynamic-tracing.md) | 18 | sections 1, 2, 3, 4, 5 |
| [final-mix.md](final-mix.md) | 20 | sections 1, 2, 3, 4, 5, 6 |
| [pcsx2.md](pcsx2.md) | 3 | Code-level debugging: PCSX2-MCP |
| [render.md](render.md) | 16 | tt04 roofs: the black rectangles; Texture container footer; Runtime; Debug aids; Decompile index; Gaps; +3 more |
| [research.md](research.md) | 5 | Verified on the USA disc; Motion rig; Player controller |
| [tables-unfinished.md](tables-unfinished.md) | 24 | sections 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14 |

Index of all systems: [../../README.md](../../README.md).
