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
| `atkp` field meanings used by the hit reaction rules (Refact values 0 Reflect, 1 Guard, 2 Nothing; BG Hit flag bit 0; Type values) | OpenKH `docs/kh2/file/type/00battle.md` | Apache-2.0 | `crates/kh2-app/src/reaction_rules.rs`, `docs/hit-reaction.md` |
| Motion trigger id names consulted while looking for a freeze trigger (range trigger 30, frame trigger 31); hint only | OpenKH `docs/kh2/file/anb/anb.md` | Apache-2.0 | `docs/hit-stop.md` |
| Enemy names for `enmp` ids (Shadow = 34, Soldier = 33) | OpenKH `OpenKh.Tools.Kh2BattleEditor/Services/EnemyNameProvider.cs` | Apache-2.0 | `crates/kh2-app/src/enemy.rs` |
| VIF unpack semantics for undefined lanes (V2 → xyxy) | as noted in OpenKH `OpenKh.Ps2/VifUnpacker.cs`, following PCSX2's behaviour | Apache-2.0 | `crates/kh2-ps2/src/vif.rs` |
| Effect containers: PAX elements, DPX effect list, DPD (particle data, textures, shapes, models, VSF), particle program file layout and program names | OpenKH `OpenKh.Kh2/Pax.cs`, `Dpx.cs`, `Dpd*.cs`, `DpdModel.cs`, `DpdPData.cs` (`PppProgramEnum`) | Apache-2.0 | `crates/kh2-fx` |
| Rig tables (constraint kinds, limiter, expression node types, joint flags) | OpenKH `docs/kh2/file/anb/motion.md` | Apache-2.0 | `crates/kh2-anim/src/rig.rs` |
| The game's motion-evaluation routines (setup / curves / solve, called at fixed addresses on a RAM image) | OpenKH `OpenKh.Engine/Motion/Mlink.cs`, kenjiuno's approach; we call the USA build's equivalents in our own interpreter on the user's executable | Apache-2.0 | `crates/kh2/src/oracle.rs` |
| 2D UI images and sequences: IMGD/IMGZ header and pixel formats, SEQD sprite / sprite part / sprite group / animation / animation group layouts (container roles of `.2dd` and `.2ld`) | OpenKH `docs/kh2/file/type/image.md`, `docs/kh2/file/type/2ld.md`, `OpenKh.Kh2/Sequence.cs` | Apache-2.0 | `crates/kh2-image` |
| R5900 / VU0 instruction semantics; FPU and VU corner cases (no infinities, saturation, LWR sign extension) | Sony EE Core / VU user manuals; conventions documented by [PCSX2](https://github.com/PCSX2/pcsx2) | — (facts, no code) | `crates/kh2-ee` |
| ARD area data: area script programs (opcode 0 loads a spawn group) and spawn-group layout (entity, event activator, walk path, return parameter, signal lists); sizes checked on the USA disc | OpenKH `OpenKh.Kh2/Ard/SpawnPoint.cs`, `OpenKh.Kh2/Ard/AreaDataScript.cs` | Apache-2.0 | `crates/kh2-ard` |

## Dependencies

[Bevy](https://bevyengine.org) (MIT OR Apache-2.0), [glam](https://github.com/bitshifter/glam-rs) (MIT OR Apache-2.0), [serde_json](https://github.com/serde-rs/json) (MIT OR Apache-2.0), [png](https://github.com/image-rs/image-png) (MIT OR Apache-2.0). Full license texts ship with each crate; `cargo tree` lists the rest.

PS2 USA corrections that we verified against the disc and that differ from OpenKH's Final Mix layouts are recorded in the `notes` column of `sheets/tables.json`.

## Behaviour studied in the game itself

Some rules are not documented anywhere and were read from the game's own code in a **local** decompilation of the user's disc, which is never committed. They were reimplemented from behaviour only, with no code copied:
- **Damage formula** (`crates/kh2-app/src/data.rs`): attacker Str or Mag chosen by `atkp`; physical damage = ⌈clamp(Str − Def, caps) × Power × resistance% × general% / 10⁶⌉; type 5 = % of max HP; type 6 = ignores Defense.
- **Enemy stat setup:** HP = ⌈lvpm.HpMultiplier × enmp.Health / 100⌉; damage caps are per-ten-thousand of HP.
- **Motion trigger timing:** triggers count global 60 Hz frames, while motions run at their own rate.
- **Effect runtime** (`crates/kh2-fx`): the effect manager's tick order, group birth, the per-program parameter cursor rule, loop restart with fast-forward, expiry; the value/matrix program handlers (integer angles over a 4096-entry sine table, Rz·Ry·Rx); sprites, models, random/loop programs, child spawners, sprite tails and the MegaBirth particle system; the hit-mark rule (attack effect bits + target type matched against PAX element categories); the weapon trail (bone-pair points at ratio × bone length along the bone's X, an 8-sample ring, natural cubic spline over the samples, first-pass alpha fade, UV spans).
- **Motion rig evaluation** (`crates/kh2-anim/src/rig.rs`): joint order, constraint cursor, IK goal search, one/two-bone IK, look-at, limiters (including two of the game's quirks: the undamped sphere limit never clamps, the damped minimum side adds its soft term), Euler re-extraction, expression semantics, F-curve pre/post modes, 60 Hz key times. Verified joint by joint against the game's own routines run by `kh2-ee` (`kh2 rig`, `kh2 rig-sweep`).
- **Player attack table** (`crates/kh2-anim/src/moveset.rs`, `crates/kh2-app/src/sora.rs`): the `ptya` subfile of `00battle.bin` is undocumented in OpenKH; its record layout and the selection rules (candidate groups, alternative keys, airborne and finisher tests, ability gate) were read from the game's player action routines. Which motion an attack plays, and whether it is an air or ground action, comes from that table at runtime.
- **Voice lines** (`crates/kh2-app/src/audio.rs`): motions name a voice ID; the `vtbl` table of `00battle.bin` (layout in OpenKH `docs/kh2/file/type/00battle.md`) maps it to the speaker's lines, with the game's chance-then-pool choice.
- **Particle programs, hit effects and death effects** (`crates/kh2-fx/src/prog_more.rs`, `crates/kh2-app/src/fx.rs`, `docs/effects-damage.md`): the handlers of `pppKeShpTail3X`, `pppKeMvYpEff` and `pppSnoMdlDttNorm`, the effect-start rules of a hit (attack effect bits and target type matched against the attacker's PAX categories, the ineffective-hit effect, the victim-side damage effect category, the placement overrides) and the Heartless `dead` script mode were read from a local disassembly and decompilation of the user's executable and reimplemented from behaviour. The three program handlers are checked against the game's own handlers run by `kh2-ee` (`crates/kh2-fx/src/oracle_tests.rs`).

- **Sound** (`crates/kh2-audio`): the SSL driver's timing (240 Hz frames, tempo accumulator, 300-per-tick), sequence commands, slide/LFO units, the pitch/volume/pan chain written to the SPU2, SE numbering (`bank * 1000 + index`, object-private banks inside `*.apdx`), hit sounds (the victim's own bank, entry 0-3 by the attack's `EnemyReaction`), and the SeBlock/VSB layouts were read from a local disassembly of the user's `LIBSSL.IRX` and executable. The driver's pitch/pan lookup tables are read from the user's own `LIBSSL.IRX` at runtime, never shipped.

- **Field camera** (`crates/kh2-app/src/camera.rs`, `camera_rules.rs`, `docs/camera.md`): the camera object's per-tick order, the radius-to-height parabola, the look-at offset, the spring smoothing, yaw swing limit, right-stick turn and zoom, the auto-camera turn rate, the lock-on framing and the wall pull-in rule (5 off the surface along its normal). The camera parameter table is read at run time from the user's `SLUS_210.05`, never shipped. The map's camera collision entry (BAR type 11) and the camera mode numbers are from OpenKH `docs/kh2/file/type/bar.md` and `areadata.md` (Apache-2.0); the area's indoor flag from `OpenKh.Kh2/SystemData/Arif.cs`.
- **HUD** (`crates/kh2-image`, `crates/kh2-app/src/hud*.rs`, docs/hud.md): the sequence player's frame stepping, interpolation and transform order, colour modulation, blend presets, parent/anchor placement, the party panel's HP ring/bar and MP bar rules, the target gauge's 200-per-segment rule and the munny popup were read from the game's own code in the local decompilation and reimplemented from behaviour.
- **BDX scripts** (`crates/kh2-bdx`, `docs/bdx.md`): the file layout, the 16-bit instruction encoding, the interpreter, the host function calling convention, the mode manager and the single-precision sin/cos and vector helpers were read from a local disassembly and decompilation of the user's executable and reimplemented from behaviour. OpenKH's `docs/kh2/file/type/ai.md` (Apache-2.0) was consulted: it only says that these files drive the AI and links a page that does not exist, so no facts came from it. The interpreter is checked against the game's own interpreter run by `kh2-ee` (`crates/kh2-bdx/tests/oracle.rs`). A few single-precision constants of the game's trig and rotation routines (about 20 floats, listed in `trig.rs` and `math.rs`) are written out because bit-exact results need them; the SunPro fdlibm-style kernels they belong to are public domain.

## Research references (consulted, nothing copied)

- [VGMTrans](https://github.com/vgmtrans/vgmtrans), zlib license: the layout of the Square PS2 WD instrument banks and BGM headers (`src/main/formats/SquarePS2`). The SPU ADPCM decoder and ADSR envelope follow the public SPU2 hardware description.

- [govanify/kh2](https://github.com/govanify/kh2) "dasein", MIT: a matching decompilation of the KH2 Final Mix ELF. It is a reference for function names and the original file layout.
- [govanify.com: reverse engineering MDLX](https://govanify.com/post/re-3d-model-mdlx): background for the model format.

## Project structure inspiration

- [samwhosung/benilla](https://github.com/samwhosung/benilla), Apache-2.0/MIT: one crate per format, runtime reads from the user's install, "the reference is the spec".
- [kvalls/hl2-rs](https://github.com/kvalls/hl2-rs), MIT: the `docs/research.md` and validation-doc style.
- [muffinmxn/gang-beasts-rust](https://github.com/muffinmxn/gang-beasts-rust), MIT: the whitelist `.gitignore`.

Only structure and conventions were taken from these projects; no code was copied.
