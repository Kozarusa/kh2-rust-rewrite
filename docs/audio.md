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
(unit test `bgm_slot_ramps_delays_and_resumes`) and the app drives them from the battle flag
below (`audio_bgm::BattleFlag`).

### The battle flag (WP-FIX-11; verified in the decompilation, implemented)

The music follows the word at 0x1c8a7d8 (`FUN_001903d8` reads "not 0"). Before WP-FIX-11 the app fed
the trigger with `sora::Battle`, the keyblade-out timer that Sora's attacks, guards and spells refill
for 240 ticks: so every swing switched the music, with or without an enemy. The original's flag does
not look at the player's actions at all.

- **Desired state** (`FUN_001904e0`): false when the player object is not valid (`FUN_0019a3f8` on
  `DAT_0035c998`; the hold timer below is also zeroed then). Otherwise true when any of three holds:
  1. a limit task exists (`FUN_001b3c00`: `DAT_01d5d6b0`, created by the limit execution
     `FUN_001b28a0`, cleared by `FUN_001b3178`, `FUN_001b32d8` and the area reset `FUN_001b2770`);
  2. bit 0x10000 of `DAT_0035d268`, which `FUN_00190900` rebuilds every tick: cleared, then the
     object list is walked (`FUN_00184ac8` over `FUN_001b7c88`: list `DAT_01d5d7a0` linked through
     +0x8b4, skipping objects whose +0x108 has 0x80000 or 0x10000000 (`FUN_0019a440`), and keeping
     only objects with +0x588 bit 0x10). The bit is set when such an object has +0x588 bit 0x400,
     wherever it is, or when its team word +0x3b0 is 2 and its **horizontal** distance to the
     player is **under 1200 units (12 m)**: the y component of the difference of the two positions
     (`FUN_0016b960`: +0x70, or +0x540) is zeroed before the length; the threshold is the float at
     0x37ad20 (1200.0 on the user's disc, read at run time).
  3. `FUN_001a4e70`: a detached script thread (the list at 0x1c95518 that host 2.20 and the spell
     start `FUN_001a4798` fill, docs/bdx.md) is running (its +0x1c lacks bit 4) for an owner that
     is a valid friend (+0x588 bit 0x4000, set by the friend constructor `FUN_0017c718`) and not a
     summon (bit 0x400000).
- Who has those flags: object type 4 of `00objentry` (normal enemy) is built by `FUN_001b02b0` ->
  `FUN_00184820` (+0x588 |= 0x10) -> `FUN_001d5c70` (+0x3b0 = 2, +0x588 |= 0x20000); types 3 and
  0xc (bosses) by `FUN_001acf80` / `FUN_001ad0c0` -> `FUN_001acf20`, which add +0x588 bit 0x400
  (dispatch in `FUN_0016e8c0` on the objentry type byte +4). Party members get team 1
  (`FUN_001746e0`). So the music is about enemies within 12 m of Sora (any boss in the area), not
  about lock-on range or attacks.
- **Hysteresis** (`FUN_001903e8`, once per tick from the object update at 0x1b775c, unless the flag
  is 2): `FUN_00190900` runs first; a grace timer (0x1c8a7dc) counts down by the tick scale
  (`DAT_00356f40`, 1.0); when the desired state differs from the flag it is taken at once while
  the grace timer is still above 0, else the hold timer (0x1c8a7e0) counts down and the flag
  changes when it reaches 0. Whenever flag and desire agree the hold timer is reloaded: 120
  (0x37ad18) while the flag is 1, 45 (0x37ad1c) while it is 0. So battle starts after 45 ticks of
  an enemy in range and ends 120 ticks after the last one left. The flag setter `FUN_00190550` also
  calls `FUN_001befc0(6)` / `FUN_001bef48(6)` (not traced) and, while the flag is 0, the update ends
  with `FUN_001a1e68` (stores the party records); neither touches the music.
- **Reset** (`FUN_00190370`, from the area setup `FUN_001689c0`): flag 0, hold timer 0, grace timer
  30 (0x37ad14). Scripts force the flag to 2 (`FUN_001905a8`, from `FUN_00173478` and
  `FUN_001b8d80`) and back to 0 (`FUN_001905c0`, from `FUN_001b99a0`); the per-tick update leaves a
  2 alone.
- **App** (`audio_bgm::BattleFlag`, `battle_flag_desire`; tests `battle_flag_rises_after_45_and_falls_after_120`,
  `battle_flag_grace_after_reset_switches_at_once`, `battle_flag_desire_is_horizontal_distance_under_1200`,
  `battle_flag_constants_from_exe`): the per-tick update and the reset are ported and the four
  constants are read from `SLUS_210.05` at run time; the reset runs when an area finishes loading.
  Term 2 uses the living enemies (`Enemy::alive`): every enemy the app spawns is an objentry type 4
  object (`enemy::area_groups` takes only `KINDS`, all normal enemies; the app has no boss). Terms 1
  and 3 are always false in the app: it has no limit task and no friends (`limit.rs` stops before
  the task; no companion objects). The scripted state 2 is not driven (no script calls it yet). The
  flag is updated even with `KH2_NO_MUSIC`. Log: `battle flag: 0 -> 1 (...)` / `battle flag: 1 -> 0`
  on every change.
- **[inferred]**: the app drops an enemy from the test when its HP reaches 0; the original keeps a
  dead enemy object in the list until its script deletes it or the +0x108 bits above are set. The
  writers of those +0x108 bits were not traced.
- `sora::Battle` stays the keyblade-out timer of the player controller (not this flag); the drive
  and reaction code still read that one.

Inferred or not done:

- The slot volume (+0x44) is applied in place of one of the two fixed 0x3fff factors of the volume
  chain in `Sequence::update_voices`; which factor it replaces was not traced (inferred; at most
  one 14-bit step of difference). Full level is 0x3000, not 0x3fff, so the app's BGM is now a bit
  quieter than before (0.75 of the old level at the same master gain).
- Resume (mode 1) restores the game's last checkpoint (state at 0x1e35c0, saved by routine
  0x108068 at a counter event that was not traced). The port resumes where the field slot stopped.
- The battle flag: see the section above (before WP-FIX-11 this bullet described the app feeding
  its own `Battle` timer to the trigger, which is what made attacks switch the music).
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

## Enemy voices, enemy footsteps and footstep dust (WP-7L)

Status words as above; nobody has listened to any of this.

- **Enemy voice bank (verified).** `FUN_00196b58` and `FUN_00196d18` (base constructors of talking
  objects, +0x588 bit 0) build the voice slot at actor +0x9b8 only when the object file (actor +0x7b0, the
  `.apdx`) holds a BAR entry of type 0x22 (`FUN_001684c0(file, 0x22)`); the slot's play method is
  `FUN_001ae8e0` -> `FUN_002c37b8(bank data, line, position, minimum volume 0x1800)` -> `FUN_002c25a8`.
  So an enemy's voice is NOT a `voice/<lang>/battle` bank: `obj/N_EX570_BTL.apdx` entry `seif` (38 lines),
  `N_EX500_BTL.apdx` `hyne` (26), `N_EX600_BTL.apdx` `sezt` (40) are `IopVoice` banks; Shadow, Soldier,
  Dusk and the Dancer have none. The party use `FUN_001ae180` banks (`voice/..`) instead.
  Host 1.262 (`0x1709c8`) and motion frame trigger 13/14 (dispatcher 0x188498 case 13/14, 0x1886f4: needs
  +0x588 bit 0; priority = second parameter when the trigger has two, else 0) call the object's method +0x54;
  for these enemies it is `FUN_001970b8` ([inferred] for N_EX500/N_EX600, whose tables were not traced),
  and the first argument is the line number of the enemy's own bank. Priority rule `FUN_001ae798`
  (`voice::VoiceSlot`, one per enemy). `FUN_001ae7d8` rewrites the playing sound's position from the actor
  every tick (`FUN_002c34c8`): voices follow their speaker (`LiveSound::follow`, also for Sora's lines;
  the "position is fixed" note above holds for kind 0 sound effects only).
  Port: `audio::enemy_voice`, `enemy_voice_calls` (host queue), `motion_sounds`. Log:
  `voice enemy <name> line <n> priority <p> -> bank <apdx> entry 0x22 id <n>`. Tests:
  `enemy_voice_banks_are_the_object_files_own_entry`, `enemy_voice_requests_carry_their_slot`.
- **Enemy footsteps (hook ported, nothing to play today).** `audio::enemy_footsteps`: trigger 2 resolves
  `footstep_number` with a per-enemy counter; trigger 19 asks for dust. None of the app's seven enemy kinds
  has a trigger 2 or 19 in any motion (`enemy_motion_sets_carry_no_footstep_triggers`); the NeoStatus ->
  bank chain resolves for all of them (`enemy_footstep_numbers_resolve_per_kind`: bank 729 in tt04).
- **Footstep dust `FUN_001b1c90` (rule ported; no dust element found on the disc).** Callers: frame
  trigger 19 (dispatcher case 19, 0x1887d0, kind = parameter 0) and `FUN_0017af10` (kinds 8/9 with the stop
  step, 10/11 when movement starts from idle; not for NeoStatus 0xe). Rule: ground material -> class byte
  (table 0x37b9b8), mask = bit class | bit kind+0x20, first element of the area manager's PAX
  (`DAT_0035a7f0`, set by `FUN_0015e6c0` from map +0x50 [inferred: the map's type 18 entry]) whose
  category holds the mask (`FUN_001db9e0`); kinds 0..0x13 probe a vertical line 50 above to 100 below the
  foot (0x37b598/0x37b59c) and start only on a hit, turned to the foot-to-toe direction. Port:
  `kh2_audio::footstep::DustRules` (tables read from the exe, test `dust_rules_match_the_executable_memory`),
  `fx::foot_dust`, `fx::AreaFx`. Gaps: (1) the foot point and direction use the actor's position and facing,
  not the bones 0x8028/0x8029/0x802c/0x802d (`FUN_0016aa18` maps them through `*(actor+0x8b0)` vtable +0x18,
  not decoded); (2) no map PAX of tt01, tt02, tt04, tt05, tt10 has an element whose category selects dust
  (tt02: all categories 0; tt04/05: 0 and 0x8000000000000001; tt10: 0x200 damage categories), so the
  lookup never starts anything on these areas: either the effects live elsewhere or the manager is not the
<<<<<<< HEAD
  map's PAX. Test `foot_dust_starts_the_area_effect_at_the_ground_under_the_foot` reports this and returns.

## Sound viewer: how to use (WP-SV1; the app's own developer tool, not in the original)

The original game has no sound test the app can port, so `crates/kh2-app/src/sound_test.rs` is the
app's own overlay for identifying ids: it steps through every sound the game can play and plays one
on demand through the same driver and request path the game uses (`audio::play_se`, the voice bank
cache and `Driver::play_voice_at`, `audio::play_bgm`); there is no second audio backend. Nobody has
listened to its output; checks are by its log lines and unit tests.

Start: `target\debug\kh2-game.exe tt04` and press **F10**, or start with the viewer open:
`$env:KH2_SOUNDTEST='1'; target\debug\kh2-game.exe tt04`. Keys while it is open (the game's movement,
attack, guard, lock-on and camera keys are swallowed, so nothing drives Sora or the camera; close it
with F10 to play):

| key | does |
|---|---|
| Left / Right | previous / next id in the bank (empty ids are skipped, both ends wrap) |
| Shift + Left / Right | 100 ids |
| PageUp / PageDown | 10 ids (Shift: 100) |
| Up / Down | previous / next bank (lands on the bank's first filled id) |
| Enter or Space | play the current id (the one before is stopped first) |
| Backspace | stop |
| F10 | close |

The overlay (top right, debug-HUD style text) shows the source, the bank name and file, the entry
and id, the data known about it (WD bank, track count, group and flags of a sound effect; duration
and sample rate of a voice line; WD of a BGM), the volume and pan the driver is given, and the last
play. Banks, in order: the global SE banks `se/se<nnn>.seb` (`se000` .. `se998`), the private SE
banks of the objects the app uses (`obj/P_EX100.apdx`, `obj/W_EX010.apdx`, the enemy kinds' files),
the party voice banks `voice/<lang>/battle/<world><n>_<speaker>.vsb` (every world, `<n>` 0..7, every
speaker name of the executable), the enemies' own voice banks (entry type 0x22 of their object
files, named `<file>#0x22`), then `bgm` (`bgm/music<nnn>.bgm`, played in the field slot: it replaces
the area music until the next area load).

**Id numbering** is the one `KH2_SOUNDLOG=1` prints for the game's own requests, so a viewer id and a
game log id mean the same thing:

- a sound effect: the game's number is `bank * 1000 + index` (`FUN_002c2258`: `n / 1000`, `n % 1000`);
  the log prints `se <bank>:<index> (<apdx or ->)`, the viewer shows `id 2001` and `log: se 2:1` for
  entry 1 of `se002`. The menu's system sounds are bank 2 (`PlaySe::number(2000 + id)`, docs/menus.md).
  An object's own bank (`PlaySe::apdx`) is listed by its file; the log form adds the file in brackets.
- a voice line: `voice <file>:<line>` (`<file>#0x22` for an enemy's bank inside its object file). The
  viewer's id is the line.
- a BGM: `music <n>`; the id is the number of `bgm/music<nnn>.bgm`.

Every play prints one line: `soundtest bank <name> entry <n> id <id> (<log form>) <data> volume <v>
pan <p> playing|not started`, for example `soundtest bank se002 entry 1 id 2001 (se 2:1) wd 20 tracks
1 group 0 flags 0x00 volume 0x3fff pan 0x40 playing`. The viewer plays without a position, as the
game's interface sounds do (`FUN_002c3318`: volume 0x3fff, pan 0x40; entry flags 0x10 / 0x20 still
pin the volume or pan in `Driver::play_se_handle`); a voice line is played at the same level and
centre pan, where the game would place it at its speaker with a minimum volume of 0x1800; a BGM at
the full slot level 0x3000.

`KH2_SOUNDTEST_LIST=1` prints every bank at start: `soundtest list <Source> <name> (<file>): <filled>
of <len> ids filled, <first log form> .. <last log form>`, then `soundtest list: <banks> banks, <ids>
ids`. A harness run with scripted keys (the harness knows Left, Right, Up, Down, Enter, Space,
Backspace; F10 and PageUp/PageDown are not scriptable): `$env:KH2_SOUNDTEST='1'; $env:KH2_NO_MUSIC='1';
$env:KH2_SCREENSHOT='local\sv1.png'; $env:KH2_SHOT_AT='9'; $env:KH2_KEYS='7:Right,7.3:Enter,8:Down,8.3:Enter';
target\debug\kh2-game.exe tt04`.

Tests (`cargo test -p kh2-app soundtest`): `soundtest_step_skips_empty_ids`, `soundtest_step_wraps_at_ends`,
`soundtest_jumps_by_10_and_100`, `soundtest_bank_step_wraps_and_lands_on_first_filled`,
`soundtest_step_entry_edge_cases`. Added for it: `Driver::stop_se` (keys the sound's notes off and drops
its player) and `Vsb::line_info` (size and rate of a line without decoding it).
=======
  map's PAX. Test `foot_dust_starts_the_area_effect_at_the_ground_under_the_foot` reports this and returns.

## The danger alarm (WP-F1 item 13; verified in the decompilation, implemented; nobody has listened)

The HP panel sounds an alarm while Sora's HP is in danger (`hp <= max >> 2`, `FUN_001a0be0`). Start
`FUN_002c1528`: sets `DAT_0036f58c`, `FUN_002c14a8(9, 0)` -> `FUN_002be49c(2, 9, 0, 0x3fff, 0x40)`: system
bank 2, sound 9, full volume, centre pan (the app's `PlaySe::number(2009)`, `hud_gauge::DANGER_ALARM_SE`).
Stop `FUN_002c1550`: clears the flag, `FUN_002c14f0(9, 0x3c, 0)` -> `FUN_002be5dc(2, 9, 0, -1, 0x3c)`: a fade
of 0x3c frames. The rule (`hud_gauge::alarm_step`): without an event `FUN_00157310` starts it when not knocked
out (`FUN_00166710`), the alarm silent (`FUN_002c1518`) and in danger (`FUN_001666e8`), stops it when it sounds
and the danger is over or Sora is knocked out; the hit handler `FUN_00157040` starts it when the hit leaves the
panel in state 1 (`FUN_00157cf0`) with the alarm silent; the heal handler `FUN_00157108` stops it when the heal
ends the danger. Other stops: `FUN_001573c8`, `FUN_002c0d00`, `FUN_002c10b0`, `FUN_002c11e0`, `FUN_002c57b8`
(panel teardown and sound-system resets, not read further). Port: one `PlaySe::number(2009)` per start, log
`hud: danger alarm starts (0x2c1528: bank 2 sound 9, hp N/M)`; the stop is logged only (`hud: danger alarm
stops`): the app's mixer has no stop or fade for a sound effect, so a looping sample would run once through.
Test `the_danger_alarm_starts_once_and_stops_when_danger_ends`. Harness: `KH2_SOUNDLOG=1 KH2_HUDHP=7:2`
(the panel's displayed HP drops into danger at 7 s): `se 2:9 (-) volume 0x3fff pan 0x40 playing`.
>>>>>>> wp/F1r
