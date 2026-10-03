# Research: what exists for Kingdom Hearts II (surveyed 2026-10-03)

| Source | License | Content | Use for kh2-rs |
|---|---|---|---|
| [OpenKH/OpenKh](https://github.com/OpenKH/OpenKh) | Apache-2.0 | 57 KH2 format docs (`docs/kh2/`; web version at openkh.dev); C# libraries for all formats; a partial MonoGame engine (`OpenKh.Game`); PS2 graphics-data decoding (`OpenKh.Ps2`); a file-name list | Main format reference; it targets Final Mix and the PC release first, so verify against the USA disc |
| [govanify/kh2](https://github.com/govanify/kh2) | MIT | Matching decomp of the KH2 **Final Mix** ELF (`SLPM_666.75`), about 41% matched; ~6.5k symbols; class and file layout recovered from debug symbols in a PS3 E3 kiosk build | Names and structure of game code; the code is close to the USA ELF |
| [OpenKH editions.md](https://github.com/OpenKH/OpenKh/blob/master/docs/kh2/editions.md) | Apache-2.0 | Edition id per release: USA `SLUS_21005` at `0x349D44`; confirm button, space width and save header per edition | Edition behaviour |
| [Yokimitsuro/khdays-decomp](https://github.com/Yokimitsuro/khdays-decomp) | CC0 | Complete matching decomp of 358/2 Days (DS) | Shared conventions, loosely |
| [govanify.com: RE of MDLX](https://govanify.com/post/re-3d-model-mdlx) | – | Model-format reverse-engineering notes | Background for `kh2-mdlx` |

No Rust KH2 code exists, so every reader here is new.

## Verified on the USA disc (by `kh2 validate`)

- `KH2.IDX` has 4,925 entries. With the 19 world sub-indexes (`000xx.idx`) there are 26,461; 26,456 are named by the OpenKH list.
- `OVL.IDX`/`OVL.IMG`: 4 LZ-compressed code overlays, using the same IDX format.
- 18 tables decode with exact sizes. PS2 differences from OpenKH: `stop` entries are 4 bytes (u16, u16). `atkp` has 277 trailing bytes that are not modelled yet.
