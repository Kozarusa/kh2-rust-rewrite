# Audio leftovers (WP-2F)

Status words: **verified** = read in the decompilation or the disassembly of the user's USA
executable; **inferred** = follows from it; **gap** = not done. Nobody has listened to any of this;
checks are by logs and unit tests only.

## Positional volume and pan for sound effects (verified, implemented)

Every sound an object plays goes through `FUN_00197008(actor, se_number)` (motion trigger 8, hit
sounds, the wall sound) or `FUN_0016baa0` (footsteps), which call `FUN_002c32b8(se_number,
position)` with the actor's position (`FUN_0016b960`: actor +0x70, or +0x540 when `FUN_001beb00` is
0). Effect sounds (`FUN_001de968`) do the same with the effect's position. Interface sounds use
`FUN_002c3318` (no position: volume 0x3fff, pan 0x40).

The sound engine (0x2c2000..0x2c3600) keeps a pool of 0x40-byte request slots: +0x00 the
position (4 floats, copied at request time), +0x14 the id, +0x18 bank, +0x10 index (`FUN_002c2258`:
`n / 1000`, `n % 1000`), +0x28 volume (default 0x3fff), +0x2c pan (default 0x40), +0x2a minimum
volume (0), +0x30 state, +0x31 kind (0 positional). Each game tick `FUN_002c30d0` runs
`FUN_002c2e90` over every slot; for kind 0 and state 1 or 2 it calls `FUN_002c27b0` and then sends
volume and pan to the IOP driver (`FUN_002be49c` at the start, command 0x13; `FUN_002be564` every
tick afterwards, command 0x15). The position is fixed, the listener is not, so volume and pan keep
changing while a sound plays as the camera moves.

`FUN_002c27b0` (the decompiler drops its VU0 code; read from the disassembly):

1. The position is transformed by the 4x4 matrix at `FUN_001068c0(0) + 0x10`. That is the camera's
   view matrix (`FUN_00106f78` builds it from eye, look-at point and up vector; `FUN_001407b8` feeds
   it), so the listener is the camera. Camera space: x to the right of the screen, z in front.
2. `d = |(x, y, z)|`; `steps = (int)(d * d / 100.0f)` (the word at 0x36f594 is 100).
3. Volume is `0x3fff - steps` when `steps < 0x4000`, else 0, then clamped to the minimum volume
   (+0x2a, 0) and 0x3fff. Full volume is only reached at distance 0; the sound is silent from about
   1280 units (squared distance 1,638,300), game units are centimetres.
4. The direction is normalised in 3D, then its y is set to 0 without renormalising. `angle =
   acos(dir.z)` (`FUN_0017bf10`: at or above 1 is 0, at or below -1 is pi), wrapped by
   `FUN_0017bbc0` as `fmod(angle + pi, 2 pi) - pi` (so exactly pi becomes -pi). `a = angle * 128 /
   pi`.
5. Pan: x < 0 and z > 0: `64 - a`; x < 0 and z < 0: `320 - a`; otherwise `64 + a`. Truncated to an
   integer. The result is a position on the driver's pan circle: 0 hard left, 0x40 centre front,
   0x80 hard right, 0xc0 behind.
6. In `FUN_002be49c` and `FUN_002be564`, a volume above 0x39ff (steps below 1536, closer than about
   392 units) sends pan 0x40 instead: close sounds are centred.

The driver (`LIBSSL.IRX`) indexes its pan law tables with the pan: indices 0..0x7f go from left to
right, 0x80..0xff have a negative left value (phase inversion for sources behind the listener;
Dolby Pro Logic style), and the player masks with 0x1ff. The SPU takes a 15-bit signed volume. The
old kh2-audio mixer masked a negative law value with 0x7fff without sign extension (a rear source
would have been louder than the front); it now sign-extends, which changes nothing for pan 0x40.

Port: `kh2_audio::positional::volume_pan` (pure, unit-tested), `audio::placement` (camera axes
from the app's camera `Transform`, metres to game units with `SCALE`), `PlaySe::pos` and
`PlaySe::at`. Footsteps, motion-trigger sounds and the wall sound are placed at their actor.
`KH2_SOUNDLOG` prints `volume` and `pan` per request.

Deviations and gaps:

- The port computes volume and pan once at the start of the sound. The game recomputes them every
  tick for the sound's whole life (command 0x15), so a long sound follows the camera there. Doing
  the same needs a handle from `Driver::play_se` and an update call; not done.
- Hit sounds (`combat.rs`), enemy hit sounds (`enemy.rs`) and effect sounds (`fx.rs`) are not
  placed yet: those files belong to another package. Hook: where they write `PlaySe`, add
  `.at(position_in_metres)` (`PlaySe::number(..).at(translation)`; for `PlaySe::hit` set `.pos`).
  Without it they stay at full volume and centre pan, as before.
- Kind 4 (map sound emitters, `FUN_0014eb50`, a sound that follows an actor or area shape) and
  kinds 2 and 3 (`FUN_002c33b8`, `FUN_002c3368`, used by the menu and the field BGM layer) are not
  ported.
- The minimum volume (+0x2a) is only written by kind 5 and 6 requests (streamed sounds).
- A source exactly behind the camera gives pan -64 in the game (the wrap quirk above); the port
  reproduces it.

## Gaps not worked (no code, no findings)

The package was cut short at the coordinator's request after item 1. Nothing was researched for:

- Battle BGM switching (the area's battle music number is already read from `arif` and logged by
  `area_bgm`, but no trigger or fade is implemented).
- Voice id 3 (target dependent), the game's random generator for voice pools, the Final Mix
  Japanese versus USA voice path.
- Footstep leftovers: the kind 8 landing path (`FUN_0017af10`), enemy footsteps, bit 0x20 of
  `*(actor + 0xc) + 8` forcing material 0x12.

Starting points: the BGM request code is reached from the area load path; the voice table lookups
are at 0x1ae4e8 and 0x1ae538; `docs/footsteps.md` lists the footstep addresses.
