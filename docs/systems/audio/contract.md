# Audio: contract

Ladder step L9. Ledger system name `audio`. Crates: `kh2-audio` (`bgm.rs`, `wd.rs`, `vsb.rs`, `ssl.rs`, `adpcm.rs`, `synth.rs`, `positional.rs`,
`footstep.rs`, `voice.rs`), `kh2-app` (`audio.rs`, `audio_bgm.rs`, `sound_test.rs`).
Sheet ids: `audio`, `dev.sound`. Scenarios: `scenarios/audio/` (none yet).

Status: music, instruments, sound effects and voices decode and play; positional volume and pan, the battle music handover, footsteps, enemy voices,
system sounds, the danger alarm and object-numbered weapon sounds are ported and measured against recordings where the docs say so. Nobody has
listened to any of it; checks are by logs, unit tests and measured levels. The sound driver as a whole (voice limit and stealing as the SPU does, mixing
level) is the open L9 work.

## What the system does
- Every sound an object plays goes through `FUN_00197008(actor, se_number)` (motion trigger 8, hit sounds, the wall sound) or `FUN_0016baa0`
  (footsteps), both calling `FUN_002c32b8(se_number, position)` with the actor's position (`FUN_0016b960`: actor `+0x70`, or `+0x540` when
  `FUN_001beb00` is 0); effect sounds (`FUN_001de968`) do the same with the effect's position; interface sounds use the interface bank `se000.seb`.
- Battle music: the area load (`FUN_00136ec0`) puts the field BGM into IOP slot 0 and the battle BGM into slot 1; the per-frame task `FUN_001373d0`
  compares the battle flag (`FUN_001903d8`, word `0x1c8a7d8`) with its last state and fades with `FUN_00137458` (battle starts) and `FUN_001374c0`
  (battle ends); pausing is `FUN_00137520`/`FUN_00137558`. The two BGM numbers are `arif` `+0x10` and `+0x12`.
- Voices (`kh2_audio::voice`), enemy voices, enemy footsteps and footstep dust, footsteps by frame trigger 2 (dispatcher `0x188498`, jump table
  `0x37aa78`) with the soundinfo record and ground material rule, sound numbers `500xxx` that play from the object's own bank (weapon sounds).
- The sound viewer (F10, `sound_test.rs`) is the app's own tool and not in the original.

## State and owners
| state | where in the original | owner in the port |
|---|---|---|
| battle music flag and state bytes | `0x354ebc`, `0x354ebd`, `0x354eb8` | `audio_bgm.rs` `BattleFlag` |
| IOP BGM slots 0 and 1 | the IOP sound driver | `audio.rs`, `kh2_audio::bgm` |
| `PlaySe` requests (`.at(pos)`, `number(2000 + id)` for the system bank) | `FUN_00197008` / `FUN_002c32b8` | `audio.rs` |
| soundinfo records and ground material | footstep trigger tables | `kh2_audio::footstep` |

## Entry points in the original
| address | what |
|---|---|
| `FUN_00197008`, `FUN_0016baa0`, `FUN_002c32b8`, `FUN_001de968` | sound requests with a position |
| `FUN_00136ec0`, `FUN_00137128`, `FUN_001373d0`, `FUN_00137458`, `FUN_001374c0` | area BGM load, battle music task and its two fades |
| `FUN_00137300`, `FUN_001371d8` | set / clear the "battle music exists" flag |
| `0x188498` | frame-trigger dispatcher (trigger 2 footsteps, 17 magic pay) |
| `0x2c1528`, `0x2c1550`, `0x2c14a8`, `0x2c14f0` | the danger alarm and its stop (bank 2 sound 9) |

## Open gaps
- Sound driver completeness: voice limit and stealing, mixing level (L9 scope); BGM transitions beyond the battle handover.
- Everything is unlistened: `audio.md` "WP-AU4 / AU5 / AU6" give the loudness measurements against the original recordings where they say so; the alarm's stop with a `0x3c`-frame fade is only logged.
- Footstep leftovers and enemy footstep tables (`footsteps.md` "Checks", `audio.md` "Footstep leftovers", "Enemy voices, enemy footsteps and footstep dust").
- Event voices and lip sync belong to `events-movies` (L10).

## Where the details are
Section numbers in references ("section 5", "20.14") are the old chapter numbers that still stand in the headings of the files below.

| file | KB | covers (section numbers are the old chapter numbers) |
|---|---|---|
| [audio.md](audio.md) | 50 | Positional volume and pan for sound effect; Battle music; Voices; Footstep leftovers; Enemy voices, enemy footsteps and footstep; Sound viewer: how to use; +5 more |
| [footsteps.md](footsteps.md) | 12 | The rule; Which soundinfo record applies; Ground material; The port; Checks |

Index of all systems: [../../README.md](../../README.md).
