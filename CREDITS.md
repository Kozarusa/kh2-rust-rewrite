# Credits

kh2-rs is written from scratch. Nothing below is vendored as code. Where we reimplement a documented format or algorithm, the source is named here, beside the code that uses it, and its license is kept under `third_party/`.

## Formats and algorithms

| What | Source | License | Used in |
|---|---|---|---|
| IDX/IMG layout, `hash32`/`hash16` name hashes, backwards-LZ decompression | [OpenKH](https://github.com/OpenKH/OpenKh): `docs/kh2/file/type/idx.md`, `OpenKh.Kh2/Idx.cs`, `OpenKh.Kh2/Img.cs` | Apache-2.0 ([third_party/OpenKH/LICENSE](third_party/OpenKH/LICENSE)) | `crates/kh2-archive` |
| BAR container | OpenKH `docs/kh2/file/type/bar.md` | Apache-2.0 | `crates/kh2-bar` |
| Gameplay table layouts (00battle, 03system, 00objentry) | OpenKH `OpenKh.Kh2/Battle/*.cs`, `SystemData/*.cs`, `Objentry.cs`, plus `docs/kh2/file/type/*.md` | Apache-2.0 | `sheets/tables.json` → `crates/kh2-tables` |
| KH2 file-name list | OpenKH `OpenKh.Kh2/resources/kh2idx.txt`, **downloaded at runtime** by `kh2 fetch-names` and never committed | Apache-2.0 | `local/kh2idx.txt` |
| Edition differences (USA vs JP/FM) | OpenKH `docs/kh2/editions.md` | Apache-2.0 | disc check in `kh2-archive` |
| Model layouts: map models, skeletal MDLX, DMA chains, VPU packet layout, strip flags, weight groups, rest-pose rule | OpenKH `docs/kh2/file/model.md`, `docs/kh2/file/type/mdlx.md`, `OpenKh.Kh2/Mdlx.Model.cs`, `OpenKh.Ps2/VpuPacket.cs`, `OpenKh.Engine/Parsers/MdlxParser.cs` and `Kkdf2MdlxParser.cs` (which credit kenjiuno's khkh_xldM) | Apache-2.0 | `crates/kh2-ps2`, `crates/kh2-model` |
| Model texture layout, CLUT addressing, GS swizzle tables (hardware block/column orders) | OpenKH `docs/kh2/file/raw-texture.md`, `OpenKh.Kh2/ModelTexture.cs`, `OpenKh.Kh2/Ps2.Reform{32,8,4}.cs` | Apache-2.0 | `crates/kh2-ps2/src/gs.rs`, `crates/kh2-model/src/texture.rs` |
| Motion sets (slot = state + id × 4, state fallback, motion ID table), ANB container, interpolated motion layout (F-curves, keys, IK helpers), motion trigger IDs | OpenKH `docs/kh2/file/anb/{mset,anb,motion}.md`, which build on [kenjiuno's msetDoc](https://gitlab.com/kenjiuno/msetDoc) | Apache-2.0 | `crates/kh2-anim` |
| Object collision shapes | OpenKH `docs/kh2/file/type/objectcollision.md` | Apache-2.0 | `crates/kh2-model/src/collision.rs` |
| `pref` physics tables (plyr/prty/sstm/fmab), `lvup` and `lvpm` layouts | OpenKH `docs/kh2/file/type/preferences.md`, `OpenKh.Kh2/Battle/{Lvup,Lvpm}.cs` | Apache-2.0 | `sheets/tables.json` |
| Enemy names for `enmp` ids (Shadow = 34, Soldier = 33) | OpenKH `OpenKh.Tools.Kh2BattleEditor/Services/EnemyNameProvider.cs` | Apache-2.0 | `crates/kh2-app/src/enemy.rs` |
| VIF unpack semantics for undefined lanes (V2 → xyxy) | as noted in OpenKH `OpenKh.Ps2/VifUnpacker.cs`, following PCSX2's behaviour | Apache-2.0 | `crates/kh2-ps2/src/vif.rs` |
| Rig tables (constraint kinds, limiter, expression node types, joint flags) | OpenKH `docs/kh2/file/anb/motion.md` | Apache-2.0 | `crates/kh2-anim/src/rig.rs` |
| The game's motion-evaluation routines (setup / curves / solve, called at fixed addresses on a RAM image) | OpenKH `OpenKh.Engine/Motion/Mlink.cs`, kenjiuno's approach; we call the USA build's equivalents in our own interpreter on the user's executable | Apache-2.0 | `crates/kh2/src/oracle.rs` |
| R5900 / VU0 instruction semantics; FPU and VU corner cases (no infinities, saturation, LWR sign extension) | Sony EE Core / VU user manuals; conventions documented by [PCSX2](https://github.com/PCSX2/pcsx2) | — (facts, no code) | `crates/kh2-ee` |

## Dependencies

[Bevy](https://bevyengine.org) (MIT OR Apache-2.0), [glam](https://github.com/bitshifter/glam-rs) (MIT OR Apache-2.0), [serde_json](https://github.com/serde-rs/json) (MIT OR Apache-2.0), [png](https://github.com/image-rs/image-png) (MIT OR Apache-2.0). Full license texts ship with each crate; `cargo tree` lists the rest.

PS2 USA corrections that we verified against the disc and that differ from OpenKH's Final Mix layouts are recorded in the `notes` column of `sheets/tables.json`.

## Behaviour studied in the game itself

Some rules are not documented anywhere and were read from the game's own code in a **local** decompilation of the user's disc, which is never committed. They were reimplemented from behaviour only, with no code copied:
- **Damage formula** (`crates/kh2-app/src/data.rs`): attacker Str or Mag chosen by `atkp`; physical damage = ⌈clamp(Str − Def, caps) × Power × resistance% × general% / 10⁶⌉; type 5 = % of max HP; type 6 = ignores Defense.
- **Enemy stat setup:** HP = ⌈lvpm.HpMultiplier × enmp.Health / 100⌉; damage caps are per-ten-thousand of HP.
- **Motion trigger timing:** triggers count global 60 Hz frames, while motions run at their own rate.
- **Motion rig evaluation** (`crates/kh2-anim/src/rig.rs`): joint order, constraint cursor, IK goal search, one/two-bone IK, look-at, limiters (including two of the game's quirks: the undamped sphere limit never clamps, the damped minimum side adds its soft term), Euler re-extraction, expression semantics, F-curve pre/post modes, 60 Hz key times. Verified joint by joint against the game's own routines run by `kh2-ee` (`kh2 rig`, `kh2 rig-sweep`).

## Research references (consulted, nothing copied)

- [govanify/kh2](https://github.com/govanify/kh2) "dasein", MIT: a matching decompilation of the KH2 Final Mix ELF. It is a reference for function names and the original file layout.
- [govanify.com: reverse engineering MDLX](https://govanify.com/post/re-3d-model-mdlx): background for the model format.

## Project structure inspiration

- [samwhosung/benilla](https://github.com/samwhosung/benilla), Apache-2.0/MIT: one crate per format, runtime reads from the user's install, "the reference is the spec".
- [kvalls/hl2-rs](https://github.com/kvalls/hl2-rs), MIT: the `docs/research.md` and validation-doc style.
- [muffinmxn/gang-beasts-rust](https://github.com/muffinmxn/gang-beasts-rust), MIT: the whitelist `.gitignore`.

Only structure and conventions were taken from these projects; no code was copied.
