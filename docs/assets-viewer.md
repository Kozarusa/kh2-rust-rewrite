# Asset viewer: how to use (WP-G10; the app's own developer tool, not in the original)

`crates/kh2-app/src/asset_viewer.rs` (with its catalogue and cursor in `asset_viewer_cat.rs`) lets you
step through the models, motions, textures, effects and sounds the app can read from your disc and
shows each one with the identifiers you can paste into a report. It draws with the game's own paths
(skinned models through `skel::spawn_skinned`, maps through `assets::spawn_model`, motions through the
`Animator`, effects through `fx::start_placed`, sounds through the sound driver); there is no second
renderer. The sound viewer (F10, `docs/audio.md` "Sound viewer") stays a separate tool.

## Start

```
target\debug\kh2-game.exe tt04          then press F11
$env:KH2_ASSETS='1'; target\debug\kh2-game.exe tt04      (starts open)
```

Optional: `KH2_ASSETS_KIND=model|motion|texture|effect|audio` opens on that kind;
`KH2_ASSETS_START=<file>` opens on that model (`obj/P_EX100.mdlx`, `map/us/tt04.map`) or effect file
(`magic/FIRE_1.mag`); `KH2_ASSETS_LIST=1` prints the catalogue's counts to the log at start.

The catalogue comes from the name list the app already reads for the world indexes
(`local/kh2idx.txt`, see the README): every listed name the disc has. Without that file only the
files the app itself uses are listed.

## Keys (while open; the game's own keys are swallowed, Sora does not move)

| key | does |
|---|---|
| F11 | open / close |
| Tab / Shift+Tab | next / previous kind: model, motion, texture, effect, audio |
| Left / Right | previous / next item (Shift: 100) |
| PageUp / PageDown | 10 items back / forward |
| Up / Down | the group or sub-item of the kind (Shift: 10), see below |
| Enter | play: restart the motion, replay the effect, play the sound |
| R | automatic orbit on / off |
| right mouse drag, wheel | orbit the camera, zoom |

| kind | Left / Right | Up / Down | shows |
|---|---|---|---|
| model | model in the group | group (`obj/P`, `obj/W`, `obj/N`, `obj/M`, `obj/B`, other `obj/<letter>`, then map folders) | the model on the stage, file, name, `00objentry` ids, meshes, bones, textures, motions, motion-set file |
| motion | motion of the shown model's motion set | the model (previous / next in its group) | tag (`A000` ..), slot, frames start..end, return frame, fps, trigger counts, the live frame; the model plays it looped |
| texture | image | source: the shown model's textures, the current effect file's textures, then every 2D image file (`.2dd`, `.2ld`, `.imd`, `.imz`) | the image (bottom left, small ones scaled up, large ones down), size, region or format |
| effect | element id of the effect file | effect file (`00effect.bar` first, then every effect `.bar`, `.pax`, `.mag`, object `.apdx`, map `.map`) | the element started 1 m above the stage (restarted when it ends), id, element index, DPX number, DPD, particle data, group, category, flags, bone, sound, start wait |
| audio | id in the bank | bank | the sound viewer's catalogue (SE banks, object banks, voice banks, BGM): bank, id, the `KH2_SOUNDLOG` form; Enter plays it the way the sound viewer does |

The shown model and the effects stand on a stage 3 km above the area's origin, so the area itself
is out of the camera's 1 km reach and the background is empty. The model kind hides nothing of the
game; closing the viewer gives the camera back.

## Log lines (paste these into reports)

Every selection prints one line, `asset <kind> <file> <id or name> ...`:

```
asset model obj/P_EX100.mdlx P_EX100 group obj/P 12/120 objentry 0x54 meshes 11 bones 228 textures 7 motions 1500 (obj/P_EX100.mset)
asset motion obj/P_EX100.mset A000 slot 0 1/1500 frames 0..60 return 0 fps 30 frame triggers 0 range triggers 1
asset texture model obj/P_EX100.mdlx #2 3/7 128x128 (sheet 256x256, region u 0+128 Repeat, v 0+128 Repeat)
asset effect 00effect.bar 1/400 id 18 (0x12) 5/40 element 7 dpx number 3 dpd 1 particle data 2 ...
asset audio se/se002.seb se 2:1 bank se002 3/120 entry 1 id 2001 wd 2 tracks 1 ...
```

(The numbers above show the form only.) `KH2_ASSETS_LIST=1` prints `assets list: models N in G
groups, motion sets M, effect files E, image files I, sound banks B with K ids`, one line per model
group and the effect and image files by extension.

## Known limits

- Motions are listed only for skeletal models that have a motion set (the `00objentry` row's set, else
  `obj/<name>.mset`); maps have none.
- A model texture's GS pixel format is not shown: `kh2_model::Texture` keeps the decoded pixels and
  the region only (the format lives in the crate-private GS info).
- An effect element bound to a bone is started at the fixed stage point (no owner); effects that
  need an owner's bones or a target show only their free parts.
- Audio: nobody has listened through the viewer; it uses the same request path as the sound viewer.
