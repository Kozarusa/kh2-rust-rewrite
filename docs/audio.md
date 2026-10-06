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

- **Per-tick update (WP-5D, verified, implemented).** The game recomputes volume and pan every
  tick for the sound's whole life (command 0x15). Confirmed again: `FUN_002c30d0` walks the slot
  list and calls `FUN_002c2e90`, which dispatches by kind byte (+0x31): kind 5 to `FUN_002c2d00`
  (state 2: still playing per `FUN_002c26c8` -> `FUN_002c27b0`, `FUN_002c2a68`, `FUN_002bef1c`),
  kind 6 to `FUN_002c2dc8`, everything else to `FUN_002c2b70` (kind 0: state 2 ->
  `FUN_002c27b0` + `FUN_002be564`). The brief assumed the source moves; it does not. The
  position is copied into the slot once, by `FUN_002c2760` (called from the request functions
  `FUN_002c2380` kind 0, `FUN_002c2530` kind 5, `FUN_002c25a8`, `FUN_002c34c8`, all in the engine)
  and nothing rewrites it afterwards. Only kind 4 (`FUN_002c2b08`, map emitters, which ask
  `FUN_0014eb50` for a position) follows something, and that kind is not ported. So what changes
  per tick is the listener, the camera's view matrix. Port: `audio::LiveMixes` (every positional
  `PlaySe` and `PlayVoice` start pushes a `LiveSound` with the fixed start position, the handle
  from `Driver::play_se_handle` / `play_voice_at`, and the entry flags or the voice minimum
  volume), `audio::update_mixes` (FixedUpdate, `Sim::Sounds`, last in the chain) which drops
  finished sounds and calls `set_se_mix` / `set_voice_mix` with `audio::live_mix` ->
  `kh2_audio::positional::{se_mix, voice_mix}`. SE entry flags 0x10 (fixed volume) and 0x20
  (fixed pan) are re-applied each tick as the driver does at the start. `KH2_SOUNDLOG=1` prints
  `mix <kind> handle <n> volume <v> pan <p>` whenever the pair changes. Tests:
  `positional::a_source_passing_the_listener_sweeps_volume_and_pan`,
  `positional::entry_flags_and_minimum_volume_apply`, `audio::a_live_sound_follows_the_moving_listener`,
  and `audio::render_pan_sweep` (writes `local/wp5d-audio/pan_sweep.wav`; nobody has listened).
  Not verified: whether the IOP applies a command 0x15 to a sound already in flight in exactly
  the way `Driver::set_se_mix` does (it changes the volume and pan the sequence uses for its next
  notes and the running voices at the next 240 Hz frame).
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

## Battle music (WP-1B; verified in the decompilation and the IOP disassembly, implemented)

Code: `audio_bgm.rs` (trigger, fades), `kh2_audio::ssl::{BgmSlot, Driver::bgm_start, bgm_fade}`.

The area load (`FUN_00136ec0`, from `FUN_001048e0`) puts the area's field BGM into IOP slot 0 and
the battle BGM into slot 1. `FUN_001371d8` (numbers equal) clears and `FUN_00137300` (numbers
differ) sets the "battle music exists" flag at 0x354ebc; `arif` +0x10 and +0x12 give the two
numbers (`area_music`). A per-frame task `FUN_001373d0` (registered by `FUN_00137128`, priority
300000) compares the battle flag `FUN_001903d8` (word 0x1c8a7d8) with the state it last acted on
(0x354ebd). On a rise (and no battle fade running, 0x354eb8 = 0) it calls `FUN_00137458`; on a
fall while one runs, `FUN_001374c0`. Both state bytes are updated either way. Pausing the BGM
(`FUN_00137520`) clears the flag at 0x354ebc and `FUN_00137558` restores it.

| | slot 0 (field) | slot 1 (battle) |
|---|---|---|
| battle starts (`FUN_00137458`) | command 0x11: fade to 0 over 0x3c0 = 960 frames | command 0xf, mode 0: from 0 to 0x3000 over 0xf0 = 240 frames after a delay of 0xf0 = 240 |
| battle ends (`FUN_001374c0`) | command 0xf, mode 1 (resume): from 0 to 0x3000 over 0x1e0 = 480 frames after a delay of 0xf0 = 240 | command 0x11: fade to 0 over 0x2d0 = 720 frames |

Frames are the driver's 240 Hz frames (4 s, 1 s, 1 s, 2 s, 3 s). Command 0xf handler 0x104814
reads (slot, from, to, time, mode, delay) and calls `FUN_103cbc`; command 0x11 (handler 0x104a8c)
reads (slot, target, time) and calls `FUN_104964`. The ramp is stepped by `FUN_1081ac` (called
from the main loop at 0x101908): while the delay counter (+0x6a) is below the delay (+0x68) the
slot does nothing, then `volume += step; time -= 1` with `step = (target - volume) / time`
(integer division), and at 0 the volume snaps to the target; a target of 0 stops the sequence
(`FUN_108900(slot, 1)`) and keeps it for a resume. `kh2-audio` ports the ramps and slot handling
(unit test `bgm_slot_ramps_delays_and_resumes`) and the app drives them from `Battle`.

Inferred or not done:

- The slot volume (+0x44) is applied in place of one of the two fixed 0x3fff factors of the volume
  chain in `Sequence::update_voices`; which factor it replaces was not traced (inferred; at most
  one 14-bit step of difference). Full level is 0x3000, not 0x3fff, so the app's BGM is now a bit
  quieter than before (0.75 of the old level at the same master gain).
- Resume (mode 1) restores the game's last checkpoint (state at 0x1e35c0, saved by routine
  0x108068 at a counter event that was not traced). The port resumes where the field slot stopped.
- The game's battle flag has its own hysteresis (`FUN_001903e8`: after the desired state differs
  it needs 45 ticks to rise (the timer at 0x1c8a7e0 resets to 45 while the flag is 0 and to 120
  while it is 1, so a fall needs 120), except in the first 30 ticks after a reset (0x1c8a7dc)).
  What makes it desired is `FUN_001904e0`: the player alive and either a script flag (0x1b3c00)
  or an awake hostile in `FUN_001a4e70`'s list (flag +0x588 bit 0x4000 and not 0x400000). The app
  keeps its own `Battle` flag (ticks held after the last sign of a fight) and feeds it straight to
  the trigger. Result in tt: the music switches 240 ticks after the last fight sign, not 120.
- Area changes: slot 1 is stopped when a new area's field music starts.
- Streams (`bgm/music118.vsb`, event BGM) are not handled.

## Voices (WP-1B; verified, implemented in `kh2_audio::voice` and `audio.rs`)

Chain from a motion's voice trigger (13 or 14: the voice id): `FUN_00175d18(actor, id, prio)` ->
`FUN_001ae3c8` -> `FUN_001ae4e8` (find the `vtbl` record of speaker NeoStatus and id; the record
is 0xe bytes: speaker, id, priority, reserved, five (line, chance) byte pairs) -> `FUN_001ae538`
(pick) -> `FUN_001970b8` / `FUN_001ae798` (queue on the actor's slot) -> `FUN_001ae7d8` (stop the
old line and start the new one every tick).

- **Random generator.** One shared word, `DAT_003716a0`, reset to 1 by `FUN_002f5fc8`; the voice
  code steps it as `x = x * 0x10dcd` (mod 2^32). Other users of the word multiply by 0x1c587629 or
  -0x59004c2b. `FUN_00325b38` is a software 64-bit unsigned divide.
- **Pick** (`FUN_001ae538`): step, `roll = x / 0x028f5c29` (0..99); walk the five pairs in order;
  a line byte below 0 is skipped (-3 asks `FUN_001ae440`); a pair with chance 0 or less joins a
  pool; a chance pair subtracts its chance from the roll and wins when the roll goes below 0; with
  no winner the generator is stepped again and the pool entry `x / ((2^32 + n - 1) / n)` is
  taken; an empty pool is -1. Port: `voice::pick_line`. **Byte-identical check:**
  `crates/kh2-audio/tests/voice_oracle.rs` runs the game's `FUN_001ae538` in `kh2-ee` on the
  user's executable for seven records and 518 generator states each (3,626 picks) and compares
  line and new generator state. The old app code differed in the pool draw (`(x * n) >> 32`).
- **Voice id 3 / target dependence.** There is no special id: a record whose line byte is -3 takes
  its line from the other party (`FUN_001ae440(speaker, other)`): -1 when equal; -1 with Auron
  (NeoStatus 5) on either side while none of the story flags 0x184c, 0x180c, 0x182e is set (bits
  of the save table at 0x33f6b0); otherwise `other - 1` for Sora, Donald, Goofy (0, 1, 2), 4 for
  `pin` (15), 3 for NeoStatus 4..14, and -1 outside 1..15. The other party is the NeoStatus of
  the actor's object at +0x9f4 (0 if it is invalid, `FUN_001699a8`). Port: `voice::other_party_line`
  (oracle test over all 18 x 18 pairs). The app passes the lock-on target's NeoStatus (inferred
  to be +0x9f4; Shadow and Soldier map to their `objentry` rows M_EX020, M_EX010) and no story
  flags (they are not in the app, so Auron lines are refused).
- **Priority slot** (`FUN_001ae798`, per-actor object at +0xb08: +8 queued line, +0xc priority,
  +0x10 sound handle, +0x18 actor): with nothing queued or playing the line is taken; otherwise it
  replaces the queued line only when its priority (the record's byte 2) is at least the queued
  one's. Port: `voice::VoiceSlot`.
- **Bank path.** Format at 0x37b900: `voice/%s/battle/%s%d_%s.vsb` with the language folder
  (`DAT_00349d40`), the world code, the area's `arif` record short at +0x30 (`FUN_001933a0`
  record, so `<n>` is not always 0) and the speaker name (table 0x35ee10 indexed by NeoStatus:
  sora, donald, goofy, ...). Three banks stay loaded (0x1cb8290, `FUN_001ae180`).
- **Japanese versus USA.** The language table at 0x349d44 is `jp, us, uk, it, sp, gr, fr`, and
  start-up `FUN_00101d68` calls `FUN_00105cb0(2)`, which stores entry 2: this USA executable
  always uses `us`. No code path in it selects `jp`; the Final Mix executable (`SLPM_666.75`) is a
  different file that was not examined, so what it passes is a gap. Port: `voice::language`
  reads the table and the index from the executable.
- **Playback.** A voice line is a kind 5 sound (`FUN_002c2530`): positional at the actor like
  sound effects (`FUN_002c2d00` -> `FUN_002c27b0`) with a minimum volume of 0x1800 (`actor +
  0x2c7`, constructors 0x1746e0, 0x174800). The app plays it through the driver's pan law at the
  volume and pan computed at the start and re-mixed every tick against the camera
  (`audio::update_mixes`, see "Per-tick update" above).

## Footstep leftovers (WP-1B)

- **Bit 0x20 of `*(actor + 0xc) + 8`.** `FUN_00183730` returns 0x12 for it, else the ground
  material. The word is the class descriptor's flag word. `FUN_0016a2f0` (the per-tick actor
  update) tests bits 2 and 0x20 of the same word together before a block that clears the
  velocity fields and calls descriptor slot 4 (inferred: a jump or ground set-up; the purpose
  of that block was not established). Which descriptors carry bit 0x20 and what sets it was not found (no writer in the
  main executable's decompilation). Port: `footstep::actor_material` (tested); Sora and the two
  ported enemies pass 0.
- **Kind 8: the stop step (WP-6A, ported).** `FUN_0017af10` is slot 6 of the mover descriptor
  classes at 0x35cbb0 (mover 0, the ground mover, registered by `FUN_00179880` in the table at
  0x1c88eb0) and 0x371298 (mover 0x2b), called by `FUN_0016a2f0` every tick as
  `(*(desc->vtable + 0x18))(desc, actor)`. It has two branches on actor +0x1c, the commanded speed
  (the w of the input vector at +0x10: `FUN_001774d8` writes the walk speed +0x110 or the run speed
  +0x114 while the stick is out, 0 without input; the AI versions are `FUN_001775e8`,
  `FUN_00177710`):
  - commanded (+0x1c > 0): for a party actor (+0x588 bit 0x80, set only by the party constructors
    `FUN_00176e00`, `FUN_00176f88`) whose last requested motion id is 0 (idle), the dust effects 10
    and 11 (`FUN_001b1c90`, not for NeoStatus 0xe). No sound. The wanted motion is 1 (walk) or 2
    (run, when `(+0x110 + +0x114) * DAT_0037a4e8 <= +0x1c`).
  - not commanded: for a party actor whose last requested motion id is 1 or 2 and whose +0x164 bit
    0 is clear, footstep kind 8 (`FUN_0016baa0(actor, 8)`) and the dust effects 8 and 9 (not for
    NeoStatus 0xe). The wanted motion is 0.
  - Both end with `FUN_0017aeb8(desc, actor, wanted)`. It is a motion request, not a shake (the
    WP-1B/5D reading was wrong): when controller +0x20 (actor +0x160) has bit 4, or bit 0x10 and the
    wanted id is not 0, it calls `FUN_0017e2d8(DAT_0037a4e4, 0, controller, wanted)`, which starts the
    motion (`FUN_0017e220`) when `FUN_0017e278` allows it.
  - The fields: actor +0x154 is controller (+0x140) +0x14, the motion id last requested
    (`FUN_0017e000` stores it). Actor +0x160 / +0x164 are controller +0x20 / +0x24, the range-trigger
    flags of the playing motion (`FUN_00188030`: range 0 sets +0x20 bit 4, range 2 bit 0x10, range
    0x2a (42) sets +0x24 bit 0, range 0x2c sets bit 1; `FUN_0017f3d0` clears them and `FUN_0017dd38`
    zeroes +0x24 when a motion starts). On the disc, Sora's A000, A001 and A002 (idle, walk, run)
    carry range 0 over every frame, and A005 (landing, motion 5) carries range 42 over every frame
    and range 2 from frame 12.
  - The one-shot gate that WP-5D could not find: walk and run have range 0 open, so the same call
    asks for idle; +0x154 becomes 0 and the next tick fails the test. Range 42 keeps the step out
    while the landing motion plays (and the landing's id 5 fails the test anyway).
  - Port: `kh2_audio::footstep::stop_step` (test `stop_step_gate_of_fun_0017af10`) and
    `audio::footsteps`, which plays it for a grounded Sora in the ground state when the stick is let
    go while the previous tick's motion id (body slot / 4) was 1 or 2 and its range 42 was closed
    (the app's controller has asked for idle before the sound system runs). Log line with
    `KH2_SOUNDLOG=1`: `footstep A000 stop (FUN_0017af10) kind 8 material 0 neo 1 -> None` (tt00,
    stick let go after running; tt00's floor has material 0, so the number is None there: the sound
    itself was neither produced on a sounding floor in a run nor listened to). Not ported: the dust
    effects 8 to 11 (effects are not this package's).
- **Enemy footsteps.** Shadow and Soldier motions have no trigger 2, so nothing to play. Hook
  for another enemy: in `enemy.rs` where the enemy's motion triggers are walked, resolve
  `footstep_number(record, neo_status, actor_material(desc_flags, ground), kind, &mut counter)`
  for each trigger 2 and write `PlaySe::number(n, None).at(position)`; the counter is per actor.
  `audio::area_soundinfo` and `audio::neo_status` (make it `pub`) give the record and bank.
