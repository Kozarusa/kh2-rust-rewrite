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

PS2 USA corrections that we verified against the disc and that differ from OpenKH's Final Mix layouts are recorded in the `notes` column of `sheets/tables.json`.

## Research references (consulted, nothing copied)

- [govanify/kh2](https://github.com/govanify/kh2) "dasein", MIT: a matching decompilation of the KH2 Final Mix ELF. It is a reference for function names and the original file layout.
- [govanify.com: reverse engineering MDLX](https://govanify.com/post/re-3d-model-mdlx): background for the model format.

## Project structure inspiration

- [samwhosung/benilla](https://github.com/samwhosung/benilla), Apache-2.0/MIT: one crate per format, runtime reads from the user's install, "the reference is the spec".
- [kvalls/hl2-rs](https://github.com/kvalls/hl2-rs), MIT: the `docs/research.md` and validation-doc style.
- [muffinmxn/gang-beasts-rust](https://github.com/muffinmxn/gang-beasts-rust), MIT: the whitelist `.gitignore`.

Only structure and conventions were taken from these projects; no code was copied.
