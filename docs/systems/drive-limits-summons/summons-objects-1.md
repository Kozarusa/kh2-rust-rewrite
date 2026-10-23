# Summons: objects the summon scripts make (1)

[Contract](contract.md) | [Index](../../README.md)

### Objects the summon scripts make (WP-EN3)

Host 1.0 `create_object` (0x170190 -> FUN_0016e8c0) in a summon script makes the objects the summon's rows name: Peter Pan's
`appear` routine (R16b9 of P_EX330) makes Tinker Bell (objentry 1268 = `Entity2` of his `sumn` row) and sets her mode `appear` with
host 1.8 on her context; Chicken Little's `surprise` thread (R1033 at 0x2171 of P_EX350) makes objentry 2098 `P_EX360` (the
`SpawnObject1` of his row; type 9, a script and effects but no model entry and no motion set), flags it (1.70 bit 1) and puts
it 60 units off its maker with 1.148. `summon.rs` (`SummonHost::own`, `act`'s `Act::Kid`, `objects`) builds them a tick later in no
summon slot (handles `KID_BASE` + serial), at the origin (the game's place record is zero) until a 1.148 moves them, carries out
the calls made on them (position, mode, hide, flags) once they exist, and removes them with the summon. Seen in `KH2_SUMMON=4`
(Tinker Bell: `appear` then `Hide` about 5.5 s later) and `KH2_SUMMON=1` (nine P_EX360 over the run). The three gaps of that
work package are closed by WP-EN4 below.

**WP-EN4 (the leftovers).**

- *Tinker Bell at the player (host 1.18).* Her `appear` routine (R0679 of P_EX340) calls `link_start(me, player, 0x4002, 0, 0, 0)`
  (BDX trace line `Func { group: 1, index: 18, args: [Obj(<her>), Obj(<player>), Int(16386), Int(0), Raw(0), Raw(0)] }`), then plays motion 231 and,
  once it is done, hides. FUN_001beb90 (0x170dd8) stores the player as her parent (+0x570), the halfwords +0x574 = 0x4002 and +0x576 = 0, clears the
  offset vector +0x540 (0, 0, 0, 1) and, as bit 1 of +0x576 is clear, the facing +0x550; FUN_0016b698(.., 3) gives her the link mover and
  FUN_0016b960 then reads her pose from +0x70 instead of +0x540. The link mover's own update was not read: [inferred] she stands at the parent's
  origin with his facing. `ai::Link::pose` places her there each tick after the class's steering (`objects`). Run `KH2_SUMMON=4`
  (state with item 160 and no option word, so the arrival event path makes Tinker Bell): `summon: P_EX340#kid class 0x11: 2338 units from the
  player` (she was made at the origin) and then `summon: P_EX340#kid linked to the player (host 1.18, kind 0x4002, flags 0): placed at Vec3(-2015,
  40, 1190)`, the player's point; she is gone with `hidden by its script` at the end of motion 231. Peter Pan's
  own `appear` link of the player to him stays a stub (`enemy-ai.md`). The screenshot of that run (7 s) shows neither she nor Peter Pan
  (she is invisible between her motions and Peter Pan stands off screen), so her position is the log's.
- *Chicken Little's child (P_EX360).* 1.114 `bind_object(ctx, 1736)` finds the object whose id is 1736 (FUN_0016b078): the `sumn` row's Entity1,
  Chicken Little's own object, the first of the summon's slots (`SummonEnv::main`, `main_object`; test
  `host_1_114_finds_the_summons_own_object_by_the_rows_entity`). 10.53 / 10.54 are the writer and reader of the effect list at an object's
  +0xb5c (FUN_0010c5b8 makes it, FUN_00136388 / FUN_001364d0 enter and find an (instance, generation) pair, FUN_00136430 drops one); the child
  spawns its effect with 1.21 on itself, enters it in Chicken Little's list, asks 10.54 and ends it with 0.51 (test
  `effect_list_registration_follows_fun_00136388`). The app keeps the list with the calling script [inferred: the list's readers, the
  object's end, were not read]. 6.49 (FUN_0019b088) is asked of each enemy the child's 2.4 walk hands out: true when the object's actor stop bit (+0x518),
  FUN_0019af28 (+0x588 bits 4 and 0x40 clear and the summon state 1), +0x108 bit 0x400 or the manager at 0x36f6a4 holds; the app answers true in
  the arrival only (test `host_6_49_is_occupied_in_the_arrival_only`). Run `KH2_BDXLOG=1 KH2_WAVE_GROUP=b_44 KH2_STATE=<item 383, option word 11>
  KH2_DRIVE_POINTS=5 KH2_SUMMON=1 KH2_SUMMON_AT=4 kh2-game tt04`: `bdx P_EX360#kid t369: first host call 1.114 FindObject [Impl]`, `... 10.53 effect_list_add
  [Approx]`, `... 6.49 object_occupied [Approx]`, `Func { group: 10, index: 54, args: [Obj(41943040), Raw(2063597569)] } -> Bool(true)`, then
  `first host call 0.51 effect_destroy [Impl]` and `1.28 Hide`: the child's script runs to its end. Still unmapped in that script (warnings in the
  run): 1.100 (0x173178, FUN_00173178: `sqrt(2 x / DAT_0037a538)` of a float, a fall time from a height) and 1.128 (the mover's virtual call,
  role unknown).
- *The revive's Tinker Bell.* The knock-out path is FUN_001cec00 -> the revive sequence FUN_001ad9a0 / FUN_001ad8a8 -> FUN_001adeb8 (clears byte 7,
  makes objentry 0x4f4 at the player with FUN_001c5e20, sets the mode `reraise`, 0x37b880). In the app the knock-out is `State::Down` of `sora.rs`
  (no game-over screen); `objects` makes Tinker Bell the tick the player is down with Peter Pan out and byte 7 set (`revive_due`), clears byte 7, puts her at
  the player in mode `reraise`, and her script's host 1.277 (0x172030 -> FUN_00178df0: FUN_00178d80 clears the dead bit and resets the mover,
  FUN_001d4a50, then the HP method with the status word +4 as the amount) gives him full HP and the app's stand-up
  (`sora.rs`'s own `State::Down` revival). The type-2 revive sequence is not run (WP-SM1 below says what it does). Peter Pan's own
  healing (1.155) kept Sora up in every 125 s harness run (waves of `b_44`); WP-SM1 knocks him out with `KH2_SUMMON_KO` instead.

**WP-SM1 (summon gaps, part 1).**

- *The link's break and routine (host 1.18, FUN_00169ee8, FUN_001becb8).* 0x170dd8 stores the 5th and 6th arguments on the link record when the routine is not 0 (record +0xc a context word, +0x10 the
  routine, +0x14 its argument: object +0x57c, +0x580, +0x584); `ai::Link` keeps them (`routine`, `arg`). The break FUN_001becb8 (called
  by FUN_001beb90 over a link already there, by host 1.102 `link_end`, 0x170e38, and by FUN_00169ee8) takes the object out of the parent's list (+0x760 /
  +0x764), clears the parent (+0x570), and, when the context word (object +0x57c) is set, runs the routine (+0x580) with the argument (+0x584) (FUN_0016c550 with one argument). The app
  does the same through `SummonAi::link_start` / `link_end`: the link is dropped, the object stays where the link last put it (FUN_0016a618 at its
  pose), and `objects` runs the routine after the tick with `Runtime::call_routine` (the game runs it synchronously inside the break; the app queues it, in order, for the end of the script's update, because the host cannot re-enter the runtime that calls it: an [inferred] deviation; log `summon: <tag> link to the player broken (FUN_001becb8): routine
  R<pc> runs with argument <n>`). Tests `a_link_break_hands_back_its_routine_and_argument` and `a_broken_links_routine_runs_with_its_argument` (a real routine run through `call_routine`). Not carried out: the facing a break gives an
  object whose flags bit 1 is clear (FUN_003003e8 of -(+0x60), -(+0x68), then FUN_0016a7f0) and the clearing of its +0x7f8 value 8 (set by the link start), because
  the app's summon objects have neither field; neither shows without a moving linked object. The automatic break of FUN_00169ee8 (flags bit 0 set and the
  parent either gone or without bit 0x8000 in its +0x160) is not applicable here: the only link a summon script makes in the app is Tinker Bell's, with flags 0,
  and a link with bit 0 clear never takes that path; a parent's +0x160 is not modelled for the player. Whether Tinker Bell's `appear` or any other
  summon script calls 1.102 or passes a routine was not found in the app's runs (her trace line shows routine 0, argument 0).
- *Host 1.277 (FUN_00178df0).* FUN_00178d80 clears the dead bit (+0x7f8 value 4), FUN_00183fe0 puts the player on the floor point under him
  (FUN_001a5c30, then FUN_0016a618), and FUN_0016b698(.., 0) gives back the normal mover; FUN_001d4a50 sets +0x9a4 bit 0 and +0xa70 bit 0 and clears
  +0x20 bit 0 of the object at his +0xb3c (the bits FUN_001d49c8 changes at his death, FUN_00178c88 -> FUN_001d4a20), and the HP method (FUN_00186238) adds
  the maximum. That method refuses while the dead bit (value 4) is set, which FUN_00178d80 has just cleared, so the heal fills a player with HP left the same
  way (`SummonAi::revive_player`, test `host_1_277_fills_the_players_hp_and_stands_up_only_a_downed_player`). In the app the dead bit and the mover are
  `State::Down` and the stand-up of `sora.rs`, and the +0xb3c object's bits are what `sora.rs` derives from the state (`hittable`): the stand-up is done only
  for a player who is `State::Down` (log `raised the player (host 1.277, FUN_00178df0): HP 0 -> <max>, he was down and stands up`), a player not down only has his
  HP filled (`he was not down`). The floor snap is `sora.rs`'s own ground contact.
- *The revive sequence (FUN_001cec00 -> FUN_001ad8a8 -> FUN_001ad378 type 2 -> FUN_001ad9a0 -> FUN_001adeb8).* What the sequence does besides making Tinker Bell:
  FUN_001ad378 sets 0x35d268 bit 1, stops the music (FUN_00137520: the BGM stream stop FUN_00136a68 with 0x1e0 and the channel ramps FUN_002c1a08 (it takes no
  argument; the 0x2d0 FUN_00137520 passes is ignored), the music state kept in 0x354ebc / 0x354ebe), opens a message window object (0xb0 bytes, FUN_00195898 / FUN_00195a78 / FUN_00195ae0, which
  becomes 0x35e3a4; step 1 of the thread sets 0x36f598 through FUN_002c38b8) and starts a thread (FUN_00102bc8 priority 1, 15000). The thread's step 2
  waits while that window exists (FUN_00196418) and until the fade object at 0x356b30 has its +0xfc at 4 (idle), clears 0x36f598 (FUN_002c38c8), puts the player on
  the floor (FUN_00183fe0), and FUN_001adeb8 makes Tinker Bell, clears 0x35d268 bit 1 and starts the music again (FUN_00137558: the saved music state back,
  FUN_00136aa0 starts the area's music record). (Corrected by WP-RV2, below: state 4 is the finished fade to black, started by the event the "window" really is.) The "fade" is therefore not a picture fade started here: the thread only waits for the fade object at
  0x356b30 to be idle (+0xfc == 4; its writers are FUN_0014a6f8 / FUN_0014a7c8 / FUN_0014a7f0 / FUN_0014a818, the screen fades of other code), so a fade
  already running (the player's death) finishes before Tinker Bell appears. What the player sees and hears, in order: system sound 0x13 (FUN_002c14a8), the
  music ramping out, a message window if the argument is a text, (no pad lock comes from 0x36f598, see below) the wait until the window is gone and the screen is not fading, then the player on the
  floor, Tinker Bell made at him and the music back.
  *Ported (WP-SM4, `summon.rs` `Revive` / `ReviveAct`, `audio_bgm.rs` `revive_stop` / `revive_restart`):* the thread as a state machine with the game's order
  (begin: flag 0x35d268 bit 1, music stop, sound 0x13, window, 0x354f70 bit 2; thread: step 0 moves on, step 1 sets the sound gate 0x36f598 and falls into step 2, which waits
  on window and fade, then clears it, puts the player on the floor, makes Tinker Bell, clears the flag, restarts the music and, at the thread's end,
  clears 0x354f70 bit 2). The music state is held in `AreaMusic::held` (0x354ebc kept in 0x354ebe, so the battle switch does nothing meanwhile) and both BGM
  slots fade to 0 over 0x4b0 driver frames (WP-SM4b: FUN_002c1a08 calls FUN_002be2e8(slot, 0, 0x4b0) for slots 0 and 1, which is IOP command 0x11 as `Driver::bgm_fade`), restarting the audible slot at full volume where it stopped. Sound 0x13 plays as a system sound. Each act is
  logged (`summon: revive: ...`), with `KH2_NO_MUSIC` / `KH2_NO_SOUND` too. Tests: `the_revive_sequence_runs_in_the_games_order`,
  `the_revive_waits_for_the_window_and_the_fade_with_sounds_held`, `a_held_music_switches_nothing_until_released`, `revive_effects_hold_and_release_the_area_music` (the Bevy glue).
  *0x36f598 is not a pad lock (WP-SM4b).* `mips_scan --addr 0x36f598` finds one reader, FUN_002c3228 (0x2c322c); no load with offset 0xf598 or a gp-relative form turned up. FUN_002c3228
  returns a free 0x40-byte request slot of the sound engine (the pool of docs/audio.md) and returns 0 while 0x36f598 is set; its callers are FUN_002c36b8 and FUN_002c37b8, which start
  positional sounds. So during step 2 no new positional sound is started; the player's pad is not touched by it, and the app's `Pad::lock` is not called for it (WP-SM4 did, wrongly; removed). The
  pad lock of a message window is FUN_00196280 in FUN_00195ae0 (when the message's flag bit 3 is clear), which the app does not open here. The sound gate is logged only (the app's PlaySe
  is outside this work package). Its other setters are FUN_00195d28, FUN_00196138 and FUN_001b9140 (clears: FUN_00195968, 001959f0, 00195e68, 00196348, 001b9140, 001b99a0, 002c1158, 002c11e0).
  *The music fade through the wait (WP-SM4b).* The original's ramp is 0x4b0 frames (5 s at 240 Hz) and the wait ends when the window is gone and the screen fade is idle, neither of which the app has,
  so the app's wait is 2 ticks and the music is back before the ramp has lowered it audibly (the restart path of `bgm_start` just sets full volume). No minimum hold is added: the code gives no number
  for the wait except those two conditions. Not ported, unknown: IOP commands 0x16 (FUN_002be5dc, args -100, -1, 0, 0, 0x4b0) and 0x27 (FUN_002bf5e4), FUN_002c1c18 -> FUN_002c0b98, and the stream stop
  FUN_00136a68 (handle 0x354e94, 0x1e0).
  *Not ported:* the message window (its text: FUN_001ad8a8 hands its own argument, which FUN_001cec00 uses as the player object, to FUN_00195a78 as the text;
  FUN_001d1160 only stores it, so no message id was found: searched FUN_001ad378, FUN_001ad8a8, FUN_001ad8e0, FUN_001ad810, FUN_00195898, FUN_00195a78,
  FUN_00195ae0; the instruction at 0x1ad8b4..0x1ad8d0 confirms a1 = a0), so no window opens and the window wait is always over; the 0x356b30 fade is not
  modelled by the app (the wait is always over). Both waits are parameters of `Revive::tick`, tested. The thread's per-tick function (FUN_001ad918, created
  by FUN_00102bc8) has no decompile; that it calls FUN_001ad9a0 each tick is [inferred] from FUN_001ad9a0's shape. [Resolved by WP-RV2, next bullet: there is no text, and the fade is the event's own.]
- *The revive's "message window" is a camera event, and the 0x356b30 fade is its fade (WP-RV2, research; nothing ported, the files are not this package's).*
  FUN_001ad378 builds the "window" with FUN_00195898(object, 0xf, FUN_00193f08(0)), FUN_00195a78(object, a1) and FUN_00195ae0(object): this is the event
  object of docs/events.md "Start and end" (the same constructor FUN_00196428 uses for a trigger's event), not a text window. Its three arguments:
  0xf is the event kind (`evtp` row of kind 0xf in `03system.bin`, bytes `0f 00 64 00 ..`: no start fade (byte +1 is 0), flags 0x64 = bits 2, 5 and 6; bit 3 clear, so
  FUN_00195ae0 calls FUN_00196280 and the two players lose control for the event, and FUN_00196170 gives it back; bit 2 set, so the field HUD stays; bit 4 clear, so no
  fade-in at the end); FUN_00193f08(0) is the event record: `FUN_001684c0(DAT_01c8a97c, 0x16, 0)`, the first BAR entry of type 0x16 of `00battle.bin` (DAT_01c8a97c is
  the second file FUN_00193610 loads), field +8 of its row; and the player object FUN_001cec00 passes is stored as the "text" word (FUN_00195a78 -> FUN_001d1160
  stores a mode 2 or 3 and the word at +0x40 / +0x44) and handed on as the fourth parameter of FUN_002232c8 into DAT_0036bd50, whose only accesses are the clear in FUN_00223180
  and that store (`mips_scan --addr 0x36bd50`: two hits, both writes): nothing reads it, so no message id exists to find [V]. On the disc `00battle.bin` has
  three type 0x16 entries, all tagged `0a`: `re_gameover-0a` (612 bytes), `re_mscomp2-0a` (396) and `re_deadboss2-0a` (2760); entry 0 is the one used. Its commands
  (size, opcode, operands): 0 the name, 0xf / 0x10 setup, **8 end frame = 0x15 (21 event frames, FUN_00226fb0 stores it at DAT_0036bcf8; FUN_00222668 ends the event
  when the event frame reaches it)**, 0x14, one camera record (0x13, FUN_0022b4f8, eight channels) with one cut (6: cut 0, frames 0 to 0x1e), 0x17 / 0x18 (FUN_00226940 /
  FUN_00226a98 at frames 0 and 0x15, not read further), 0x46 (the camera flags), 0x27 (FUN_00226b58 starts a thread, not read), two 0x20 (FUN_00226fc0, a colour change
  FUN_0014cb50 at frames 4 and 0x14 [inferred]) and **0x12 at frame 0xb with length 0x32 and mode 2**. FUN_00226830 mode 2 is `FUN_0014a7c8(FUN_0014a978(), 0x32, 1)`: FUN_0014a978 returns
  the object at **0x356b30**, so the "0x356b30 fade" is this event's screen fade to black. FUN_0014a7c8 (disassembled: it stores state 3 at +0xfc, the flag at +0xf4 and jumps
  to FUN_0014a710 with the length untouched in a1) makes the counter 2 x length = 100 and FUN_0014a818 takes DAT_00356f44 (1.0, read from the executable) off it each
  frame while the alpha goes from 0 to 0x80; at zero the state becomes 4 and stays (black). FUN_001ad9a0 step 2 waits `+0xfc != 4` with `return 1`, i.e. **until the
  screen has faded to black** (the earlier reading "waits for the fade to be idle" had the sense reversed: state 4 is the finished fade-out, state 0 the idle one;
  a run without the event's fade would wait forever). The window wait (FUN_00196418: DAT_0035e3a4 != 0) is the event object's life: it ends when the event does (end frame 21
  event frames = 42 game ticks, the event frame being half the tick counter, FUN_00222728 and 0x37ec50 = 0.5) and the object's end states run (FUN_00195c08
  states 5 to 9 -> FUN_00196170, which also stops the event's own sound gate by FUN_001959f0). Event frame 0xb is game tick 22, so the fade ends at about tick 122
  (100 ticks after tick 22); at 60 ticks a second that is 2.0 s after the revive begins [inferred: that FUN_0014a818 runs once per game tick; the event frame to
  tick ratio is the one docs/struggle.md already uses]. So **Tinker Bell appears after about 2 s on a black screen with the pad locked**, not two ticks later. What lifts the black
  afterwards was not found: the callers of the fade-in FUN_0014a7f0 are FUN_001047a8 (every area start), FUN_00223a88 (the next event's start, which fades 0x356b30 and
  0x3565e0 in at once, only while an event runs and FUN_002c17e0 is set), the event end FUN_00196170 / FUN_00195ff8 (other faders, 0x3564d0 and the two of FUN_0014a958 /
  FUN_0014a968), the movie player FUN_00225860, FUN_00227280, FUN_001b65e0, FUN_0015d1a8 and FUN_002de1c8; Tinker Bell's script host 0.31 / 0.32 / 0.88 move 0x3564d0, not 0x356b30.
  [GAP: which of these returns 0x356b30 to clear in the revive; searched `mips_scan --jal 0x14a7f0`, `--jal 0x14a978`, `--jal 0x14a6f8` (only FUN_0014ab80 resets it).]
  For the port (code outside this package: `summon.rs` `Revive::tick`, `sora.rs`, `event_player.rs`, `audio_bgm.rs`): the window wait is the 42 ticks of the event, the fade wait ends at tick
  122, the pad is locked from the start to the event's end, and the camera plays the first 30 event frames of `re_gameover-0a`'s cut; the screen goes black from tick 22 over 100 ticks.
  Tests: none (nothing ported); the claim rests on the disc dump of the three events (a scratch BAR reader) and the addresses above.
- *The pad lock and the fade are wired, and the fade is WHITE (WP-RV4).* `sora::tick` locks the pad (`Pad::lock`) while `SummonState::revive_pad_locked()` (= `Revive::pad_locked`, ticks 0 to 42), and
  `summon.rs` draws a full-screen quad (`ReviveFade`, `paint_revive_fade`) with `Revive::fade_alpha()` (0 to 0x80 over ticks 22 to 122). The colour corrects the "fade to black" wording above: FUN_00226830 mode 2 passes the flag 1 to
  FUN_0014a7c8 (case 2 -> label 0x2268cc sets a local to 1), which stores it at +0xf4, and FUN_0014a710 reads +0xf4: non-zero gives `FUN_0014af90(0xff, 0xff, 0xff, 0)` and word 0x48, zero gives black and 0x44. So the revive's
  fade, the same object 0x356b30, is white. The blend word is not decoded, so plain alpha blending is assumed [inferred]. The quad is clear again once the thread ends (the gap above: what lifts the fade is unknown). Tests
  `the_revive_lock_and_fade_reach_the_screen`, `the_revive_waits_for_the_window_and_the_fade_with_the_sounds_held`.
- *The fade's blend, its alpha curve, what does not lift it, and the message text (WP-RV5, `revive_fade.rs`, `summon.rs`).*
  *Blend words.* The word FUN_0014a710 stores at +0x90 of the fader object (FUN_0014b390) is the GS register ALPHA_1: the draw packet FUN_0014b408 copies the object's words from +0x20, and FUN_0014b000 puts the register id 0x42 (ALPHA_1) at +0x98, right behind the +0x90 data word. The register's fields are A, B, C, D (two bits each from bit 0; A, B and D pick 0 source colour, 1 destination colour, 2 zero; C picks 0 source alpha, 1 destination alpha, 2 the fixed value) and the result is (A - B) * C / 128 + D. So 0x44 (A 0, B 1, C 0, D 1) is the plain alpha blend `Cd + (Cs - Cd) * As` and 0x48 (A 0, B 2, C 0, D 1) is the additive blend `Cs * As + Cd`. With the flag 1 the fade is white and additive: it adds white to the whole picture, the HUD included, and clamps, so it reaches full white before a plain alpha blend of the same alpha would. The flag 0 (black) uses the alpha blend. The port draws the revive's fade as a white quad with the additive blend (`ReviveFadeMaterial`, the blend of `hud_gfx::HudMaterial`, the HUD shader) on the keyhole wipe's layer (above the HUD and the menus [inferred from the task priorities: the fader thread FUN_0014aa00 runs at 0x19640 = 104000, the field HUD's tasks at 8000, docs/hud.md; the keyhole wipe and the 0x3564d0 fader come after, FUN_0014aa90 at 330000]). Test `the_blend_words_decode_to_alpha_and_additive`; the screenshots below.
  *Alpha curve.* The ramp is exact now, not linear [V]: FUN_0014a710 makes the counter 2 x 0x32 = 100 and the ratio 128 / 100 (DAT_00379130 = 128.0); each FUN_0014a818 run in state 3 takes DAT_00356f44 (1.0) off the counter and stores `0x80 - (char)(int)(ratio * counter)`, and at counter 0 or less it stores 0x80 and sets state 4. In single precision that is 2 after the first run, 0x40 after 50, 0x7f after 99, 0x80 after 100 (`revive_fade::alpha_after`, test `the_fade_alpha_follows_the_games_counter`). State 4 has no case in FUN_0014a818, so the fader keeps drawing at alpha 0x80: full white.
  *What lifts the fade: nothing in the revive.* The fader is at state 4 when the thread ends (FUN_001ad9a0 case 2 returns once the event object is gone and +0xfc is 4, then FUN_001adeb8 ends it). The only code that can take 0x356b30 out of state 4 is a fade-in call on it (state 1 through FUN_0014a7f0, or a new state 3 through FUN_0014a7c8 with the flag 0). The complete list of callers that reach 0x356b30 (`mips_scan --jal 0x14a978` and `--jal 0x14a7f0`, all hits read): FUN_001047a8 (every area start, fade-in length 0, flag 0); FUN_00223a88 (the event command thread: before its first command it fades 0x356b30 and the object of FUN_0014a948 in, length 0, flag 0, only when an event object exists and FUN_002c17e0 is set; DAT_01e33ca0 is set by FUN_00195e68 when an event starts and cleared by FUN_00195968 / FUN_001959f0 only when the event kind row's flag bit 0 is set, which the revive event's row 0x64 does not have); FUN_00226830 (the event command 0x12, modes 0 to 2 and 5); FUN_002226d0 (the event skip, FUN_00222820: a fade out to black); FUN_001ad9a0's own case 6, FUN_001adc18 (a fade out to black for the game-over types 0 and 1, never the type 2 revive). Everything else in the earlier list (FUN_001b65e0, FUN_002de1c8, FUN_0015d1a8, FUN_00195ff8, FUN_00196170, FUN_002233d8, FUN_00227280, FUN_0016d880 / 0016d958, FUN_0013f078 / 0013f1a0 / 0013f2e8) works on other faders (0x3564d0, 0x3565e0 and the FUN_0014a928 / 938 / 948 / 958 / 968 ones); the stores to +0xfc elsewhere are other objects' fields. The revive's own event (`re_gameover-0a`, entry 0 of the type 0x16 entries of `00battle.bin`, dumped from the disc) has exactly one 0x12 command (frame 0xb, length 0x32, mode 2) and its commands after the end frame 0x15 never run; its other commands are 0x20 at frames 4 and 0x14 (FUN_00226fc0 -> FUN_0014cb50, a colour parameter 0.1 and 1.0 handed to a thread, not read), 0x27 at frame 0 (FUN_00226b58, a small effect record and a thread at 0x226b10, not read) and 0x17 / 0x18 at frames 0 and 0x15 (FUN_00226940 / FUN_00226a98, the camera filter and a camera object, not the fader). So by the code the screen stays full white after Tinker Bell appears, until the next area start, the next event's start (its prelude fade-in, which re-colours the object black for the two hold frames and the two fade frames it runs) or an event's 0x12 command. That reading is [inferred from the code only, not observed]: a PCSX2 watch of 0x356b30 + 0xfc after a revive would settle it. The app does NOT keep the white: `Revive::fade_alpha` is 0 once the thread ends, because the app has no later fade-in call to lift it and a permanent white screen would be worse than a missing lift [conservative; the observed original, which the user's play-test reports outrank, may differ]. [GAP: the observed lift; searched every caller above and the stores to +0xfc and to the +0xf4 / +0xf8 fields through `mips_scan --store-off 0xfc`.]
  *The message window text: there is none.* The "window" is the kind 0xf camera event (the WP-RV2 bullet): FUN_001ad378 hands FUN_00195a78 the player object as its second argument (the second argument of the call; FUN_001ad8a8 passes its own argument through, FUN_001cec00 uses the player object there), FUN_00195a78 -> FUN_001d1160 stores it at +0x40 / +0x44 of the event and FUN_002232c8 hands it on into DAT_0036bd50, which nothing reads (`mips_scan --addr 0x36bd50`: two writes, no reader). No message id and no text bar entry is read anywhere on the path, and the three constructors take no message argument. "It has no text" is the result; the act `ReviveAct::WindowSkipped` only logs it (`the "window" ... is the kind 0xf camera event; it has no message text`).
  Run (Peter Pan charm is item 160, `{"items": {"160": 1}, "summon": {"level": 1, "exp": 0}}` as `KH2_STATE`): `KH2_STATE=<file> KH2_WAVE_GROUP=b_44 KH2_DRIVE_POINTS=5 KH2_SUMMON=4 KH2_SUMMON_AT=5 KH2_SUMMON_PARTY=1 KH2_SUMMON_KO=10 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_SCREENSHOT=<png> KH2_SHOT_AT=11.4 kh2-game tt04` logs `revive: tick 60, pad locked false, white fade alpha 0x31`, `tick 80 ... alpha 0x4b`, `tick 100 ... 0x64`, `tick 120 ... 0x7e`. The screenshot at 11.4 s (about tick 55, looked at) shows the town, Sora, the Dusks and the HUD washed with a faint white; the one at 11.9 s (about tick 100, looked at) shows everything, the command deck and the Peter Pan / Sora faces included, almost white. Nobody has listened or watched.
- *Ported (WP-RV3, `summon.rs` `Revive`).* `Revive::tick` keeps a clock from the start of the revive: the camera event (`window_open`, `pad_locked`) lasts 42 ticks, the black fade (`fade_alpha`, 0 to 0x80, linear [inferred]) runs from tick 22 for 100 ticks, and the thread makes Tinker Bell at tick 122 (`fade_black`), as the previous bullet found. Tests `the_revive_sequence_runs_in_the_games_order`, `the_revive_waits_for_the_window_and_the_fade_with_the_sounds_held`. Not wired: `sora.rs` / the HUD do not yet read `pad_locked` / `fade_alpha` (outside this package), so in the app nothing is drawn black and the pad is not held. The gap stays: what lifts the black afterwards (the fade alpha returns to 0 when the thread ends). No harness PNG of the revive was made.
- *The revive's Tinker Bell in a run.* `KH2_SUMMON_KO=<seconds>` (new, a harness aid) knocks the player out (HP 0, `State::Down`) once while a summon is out.
  Run: `KH2_STATE=localcc1\state-sum.json KH2_WAVE_GROUP=b_44 KH2_DRIVE_POINTS=5 KH2_SUMMON=4 KH2_SUMMON_AT=5 KH2_SUMMON_PARTY=1 KH2_SUMMONLOG=1
  KH2_SUMMON_KO=10 kh2-game tt04` (WP-SM4, with `KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_SCREENSHOT=<png> KH2_SHOT_AT=14`) logs `KH2_SUMMON_KO: the player is knocked out (HP 20 -> 0)`,
  `the revive sequence starts (FUN_001ad378 type 2)`, then in this order `revive: flag 0x35d268 bit 1 set`, `music stops`, `system sound 0x13`, `the message window ... is
  not opened: its text is not found`, `0x354f70 bit 2 set`, 38 ms later (two ticks) `Tinker Bell (objentry 1268) made at the player in mode "reraise" (FUN_001adeb8)` with
  `pad locked`, `pad released`, `put on the floor`, `flag cleared`, `music restarts`, `bit 2 cleared`, and 21 ms after that `P_EX340#kid raised the player (host 1.277):
  HP 0 -> 20, he was down and stands up`. The screenshot 4 s later (looked at) shows Sora standing with HP 20/20, Peter Pan beside him, Dusks around and no message
  window; Tinker Bell is not in it. The music itself and the sound were not heard (nobody has listened; the run had audio off, the stop and restart are logged).

