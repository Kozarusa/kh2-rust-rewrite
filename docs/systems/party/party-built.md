# Party: status as built, stages 1 to 4 and later fixes

[Contract](contract.md) | [Index](../../README.md)

## 11. Status: stage 1 in the app (WP-7P1)

Files: `crates/kh2-app/src/party_rules.rs` (pure rules, constants read from the executable at run time),
`party.rs` (spawn and tick), `friend_ai.rs` (the members' scripts). Log `KH2_PARTYLOG=1`; harness
`KH2_NO_PARTY=1`, `KH2_PARTY_WARP=<s>` (the forced warp of FUN_00110e08), `KH2_PARTY_AT_RECORD=1` (Sora starts at
the player's row of the friends' map group: the app otherwise starts him at its own spawn point, outside the
walls of tt02's Usual Spot).

Done:

- Slot bytes: preset 1 applied to an empty party (FUN_001a7330), `FRIEND_1` = slot byte 1, `FRIEND_2` = byte 2,
  resolved through the world's memt merge (FUN_0017cd20). In Twilight Town: `P_EX020` and `P_EX030`, AI classes 0
  and 1 (`party_tt02_friends_are_donald_and_goofy`). The rows are the first `FRIEND_1` / `FRIEND_2` rows of the
  area's `map` script (tt02 `m_00`); the entrance byte is not tested (the app has no entrances).
- Model and motion set (objentry `AnimationName`); player-type motion sets (4 slots per motion id).
- The member's script attached, init run, mode `idle` set and run each tick (`friend_ai.rs`). The idle mode
  (`P_EX020` R0489 -> R0499) faces the target and asks for motion 0; that request is logged and not carried out
  (the player-type remap FUN_0017da30 is not ported), so the mode ends after a frame.
- Follow: FUN_0010e140 in the friend's motion state 0x1f (FUN_00111070 keeps a free member there), which is the
  path the field follow takes; FUN_0010e5e8's own body (state 0) is not ported. Corrections to section 4: in this
  path the distance the speed modes compare is the 3D one (+0xb9c), and while the turning bit is set the
  direction is the one to the player (+0xb90), not to the follow point (oracle-checked,
  `party_follow_step_matches_fun_0010e140`). Steer FUN_00110468 (`party_steer_matches_fun_00110468`), trail
  FUN_0010b270 (`party_trail_matches_fun_0010b270`), follow points FUN_00111670 (section 3).
- Warp: FUN_00110eb0 / FUN_00110a58 / FUN_001108c8 / FUN_00110e28: due when the cooldown is out and the far
  counter is above 39; the member must be off screen; trail points tried from `(warps + 0x1c + random(4)) & 31`
  (absolute ring indices, 0x110ccc), eleven, three apart; forced (flag 0x8000) takes the accepted point nearest
  the player. Fade-in 30 frames, cooldown 480, counter 0, warps + 1; a new trail point clears the warps.

Inferred or approximated in the app: the walk/run/turn speeds from the `prty` row of the NeoStatus; the facing
turns toward the movement (toward the player while turning); the motion is the motion id of the speed (0, 1, 2)
in the field state; the screen test (FUN_00146648) is the camera frustum on a box of the body shape; the spot
test of FUN_001108c8 is "a floor under the point"; +0x660 bit 4 is "the last step was blocked"; the random
generator is not the game's.

Not done in stage 1 (stage 2 below did the weapons): the weapons. FUN_0017c718 attaches the hand objects through FUN_0016ef28 (the member's
saved weapon word, `Progress::member`) -> FUN_001845b0 (`went` of the moveset) -> FUN_001751b0 -> FUN_00184190,
which ends in resource handles, not a bone number; the attach bone of Donald's and Goofy's models is not known
(objentry `WeaponJoint` is 2 and 3, not a bone). Sora's bone 178 was set by looking, which this WP did not do.
Also not done: members do not push each other or Sora apart (nothing in the paths read does it); the spawn
refusals during a form or a summon (stage 6); battle (the members follow in battle too until stage 2).

## 12. Status: stage 2 in the app (WP-7P2)

Files: `crates/kh2-app/src/party_battle.rs` (pure rules of the battle AI), `party.rs` (the battle branch of the
tick, weapons, push-apart), `friend_host.rs` (the members' script host: group 10 and the motion remap),
`friend_ai.rs` (runs the scripts through it). Log `KH2_PARTYLOG=1`: "battle flag set", "target", "chose record",
"attack motion", "actions in the last second", "hit". Harness `KH2_PARTY_WARP_ONSCREEN=1` (with
`KH2_PARTY_WARP=<s>`) lets the forced warp happen in view, `KH2_PARTY_WEAPON_BONE=<n>` overrides the weapon bone, `KH2_PARTY_WEAPONS=<Donald item>,<Goofy item>` hands an item in for
a zero weapon word.

Done:

- **Battle gate** (FUN_001903d8): the tick reads the game's battle flag (`audio_bgm::BattleFlag`, word
  0x1c8a7d8). Clear: the stage-1 follow. Set: the battle branch of FUN_00111070.
- **Action records** (`ClassAi::from_exe`): the class's block (+0x60..+0x80, +0xa0/+0xa4 entries, +0xa8 follow-ups)
  and the records of Donald (0x353090, 20, from FUN_00114550) and Goofy (0x3533a0, 18, FUN_00114668), bound to
  their 0x80-byte entries by id and to the ability table 0x352ec0 / item order 0x352eb0 as FUN_00113450 does.
  Printed by `party_action_records_from_the_executable`.
- **Range** FUN_0010f118 (oracle-checked over 432 cases, `party_range_matches_fun_0010f118`), **feasibility**
  FUN_0010f220, **anti-repeat** FUN_0010fc68 / FUN_0010ed00, **choice** FUN_0010fcd0 (continuous records kept,
  setting-2 records only in the second pass, the triple repeat skipped once, nothing feasible = stand).
  Handlers read for phase 0/1: FUN_001137d8 (attack: FUN_001103f8 then the command FUN_0010c8f0), FUN_001135e0
  (stand facing the target, wait +0x70..+0x74), FUN_00113728 (move, wait +0x78..+0x7c), FUN_00114738 (Goofy:
  run the way it started); the waits count down in FUN_001112e8 (+0xcd4). The idle handlers refuse while any
  attack is feasible (FUN_0010ed20).
- **Target, style 0** (FUN_0010d0c0, read): the remembered attacker's nearest shape, else the enemy hurt shape
  whose surface (centre distance minus radius) is nearest and within the block's +0x68; re-target
  (FUN_0010cbe0) when the target is invalid, the attacker differs, or the target is beyond +0x6c. Battle
  distance (FUN_0010df08): farther than +0x80 from the player makes the target invalid (bit 0x40).
- **Moving in battle**: toward the target FUN_0010e5e8's body with the relation +0xbb0 (row 2 thresholds,
  ground distance; the stop when the target is right above, 0x378564 / 0x378568); following the player with
  row 1.
- **Attack command** (FUN_001112e8 -> FUN_00174cf8 -> FUN_00110790): the entry's motion through the player-type
  remap FUN_0017da30 (battle row 1 of the table 0x37a5c8, checked equal to `kh2_anim::PlayerState`'s orders);
  an air entry (+0x20 != 0) jumps `+0x20 - dy` (held between 0 and the jump height) toward the target at the
  run speed. The member is busy until the motion ends on the ground. Its attack volumes hit enemies through
  `combat` (a `Fighter` of the player team; not hittable before stage 3).
- **Motion remap for the scripts** (follow-up 2): `friend_host` remaps the scripts' motion requests
  (FUN_0017e000 -> FUN_0017da30) before the enemy host plays them; the requested slot plays while the member
  stands (the `idle` mode's motion 0).
- **Group 10** (`friend_host`): 10.14 (target bit; record copy not carried out), 10.19, 10.31 (false: not a
  summon), 10.36, 10.40 (false: no linked controllers), 10.55 (bit 16 of the argument's word +4, read from
  FUN_002f0758). Wrappers 0x2f6da0, 0x2f72e8, 0x2f7118, 0x2f7230, 0x2f6e60 decompiled with ghidra.
- **Push-apart** (follow-up 4) [inferred]: members move out of each other, Sora and the enemies by their body
  radii; the game's object-to-object push was not read.
- **Weapons** (follow-up 1): FUN_0016ef28 -> FUN_0016ee98 reads the member's saved weapon word (`Progress::member`),
  FUN_001845b0 the `went` object of the member's moveset for the item's `Flag1 | Rank << 8` slot, FUN_00184640 the
  `wmst` motion set of hand 0 (item 75 -> `W_EX020`, item 49 -> `W_EX030`, `party_member_weapons_from_went`). The
  bones: Donald 131, Goofy 101, the wrist bones with four finger children on the bind pose's -X side, the side of
  Sora's 178; on screen the staff and the shield sit in the hand facing outward (the other wrist puts Goofy's shield
  edge-on into his leg). Shown only while the battle flag is set [inferred from the reference video
  `10_menu_open_close.mp4` at 0.0 s, where neither member holds anything out of battle]. **Gap**: the app's new
  game leaves both members' weapon words 0 (the original's writer of those words was not found here; searched: item-id
  immediates 0x4b / 0x31 in the decompilation; WP-RV2 found it: host 0.75 in `00common.bdx` event 7, below), so no weapon appears without `KH2_PARTY_WEAPONS=75,49` (harness
  aid) or a state that sets the words.
  (WP-PY3) The scenarios that load a state file (`KH2_STATE`, e.g. `local/acc1/state-sum.json`, which has no
  `members` entry) skip the app baseline, so the words stay 0 and `party: P_EX030: no weapon (item 0: no went entry
  for moveset 20)` is the correct answer to a zero word, not a failed lookup: FUN_0016ee98 answers 0 for a word of
  0 and FUN_0017c718 then builds no hand object. The original's own source of the words is the story (`plrp` rows
  of another Id through FUN_001c0cb0 / FUN_001c0b80 -> FUN_001a18d8, bit 15 = equipped) [inferred; only
  `FUN_001c0cb0(0)` has a caller, see docs/systems/core-runtime/progress.md Gaps]. (WP-PG1) Checked: no `plrp` row of `Id` 0 or 1 for
  Donald or Goofy lists a weapon or shield item, so the rows are not the source (docs/systems/core-runtime/progress.md "App baseline");
  `Progress::start` now applies the staff / shield entry to a state file that carries no `block` too
  (`apply_party_weapons`). A file or slot with a `block` keeps its words; `KH2_PARTY_WEAPONS` still works.
  **(WP-RV2) The real writer.** Host 0.75 (table slot 0x35bc08, native FUN_0016d9d8 -> FUN_00187d88(0x33fec0, member, w0, w1) -> FUN_00187648) stores the two weapon
  halfwords at record +0 and +2. `00common.bdx` entry event 7 (routine R00c1) calls it 13 times: member 1 (Sora) 41, 2 (Donald) **75**, 3 (Goofy) **49**, 4 81, 5 54, 6 55 and 80,
  7 72, 8 62, 9 59, 10 60, 11 61, 12 74, 13 73, then host 0.76 (FUN_0016da08, the form record's +0) for forms 1 to 4 = 0, 5 = 71, 9 = 42. FUN_00194d78 runs that event: it makes a
  thread of `00common.bdx` (DAT_01c8a980, loaded by FUN_00194d50), starts event 7, runs it to its end and frees the thread. Its caller is the game object's constructor FUN_00105638 (the only
  `jal 0x194d78`), in the same constructor right after FUN_00187d20(+0x598) (not read), so the words are the defaults of every boot and a loaded save replaces them. The constants 75 and 49 (and Sora's 41) are
  exactly `DONALD_STAFF`, `GOOFY_SHIELD` and the Kingdom Key of the app baseline. The other writers of record +0: FUN_001878f8 (story events of FUN_001d5048 give Sora 0x29, 0x2b or 0x180; it keeps the
  old word in DAT_00341854 and FUN_00187910 gives it back, 0x29 when none) and the second event script writer, `ard/tt04.ard` `tt04_ms106` (host 0.75 on Sora with 384, 502 and 501). No other
  writer of Donald's or Goofy's words was found (FUN_00187648 is reached only through FUN_00187d88, whose callers are host 0.75 and FUN_001d5048; host 0.75 has 16 call sites in two scripts,
  `00common.bdx` and `tt04.ard`; the equipment menu's writers, docs/systems/ui/menus-pause.md 2.2, were not re-read) [inferred: the words stay as the boot default until the menu changes them]. Ported by a later package: `Progress` should take the 13 member
  words and the form words from this table at the point FUN_00194d78 runs (a boot-time default) instead of the two constants.
  **(WP-RV3) Ported.** `progress::BootWeapons::read` runs event 7 of `00common.bdx` on a recording host (hosts 0.75 / 0.76 only) and `Progress::apply_party_weapons` writes the 13 member words and the form words (forms 5 = 71, 9 = 42; 1 to 4 = 0 change nothing) where the first word is 0; `DONALD_STAFF` / `GOOFY_SHIELD` are gone. Sora 41, Donald 75, Goofy 49 are read, not coded. Tests `the_boot_script_sets_the_member_and_form_weapon_words`, `boot_weapons_fill_empty_words_only`. Members 4 to 13 now carry their words too.

Not done or approximated in stage 2:

- (Done in WP-P3b, section 13: the combo follow-ups, items, specials, Cure and the emergency pass.)
- (Closed in WP-F1 item 5.) A member's hit now uses the member's own record: `enemy::receive_hits` builds the
  attacker's `Stats` from `Progress::member(neo)` (strength, magic, defense, level, max HP: the status block
  +0x188 / +0x18a that `FUN_001a0610` reads is rebuilt from the saved record, `FUN_0019fd30` -> `FUN_0019f920`),
  without the hand-0 weapon's attack and magic (`enemy::member_stats`; [inferred] gap: the rebuild's equipment
  pass is not applied for the members, unlike Sora's keyblade in `GameData::sora_stats`); the combo count is
  Sora's only for Sora's hits. The hit sparks come from
  the member's weapon effect file (`fx::hit_sparks`: the first hand object's rig; `party::spawn_party` registers
  `obj/W_EX020.apdx` / `obj/W_EX030.apdx` with `drive::register_effects`), the keyblade's when the weapon has
  none. Log: `<enemy> hit: atkp N damage D ... (attacker Donald ...)` lines carry the damage.
- (WP-F1 item 6.) The app baseline equips item 75 on Donald and item 49 on Goofy (`Progress::apply_baseline`,
  constants `DONALD_STAFF` / `GOOFY_SHIELD`) when their weapon words are 0, so the weapons show without
  `KH2_PARTY_WEAPONS`; the original's writer of those words is still not found (above).
- The settings bytes (record +0xd4) are read from `Progress` (WP-PA3; byte 0 = the battle style); the item / ability use
  settings (bytes 1..) are read by the feasibility test (`record_setting`: byte `1 + record.setting`) and the HP / MP item handler
  (`item_setting`: consumable `id` is byte `id`, `FUN_00113888`) since WP-PA4; the Customize page 0x1b writes them.
- (WP-P3b) The walk/run/turn/jump speeds now come from the `prty` row of the NeoMoveset (FUN_00194438; Donald
  row 17 and Goofy row 20: walk 2, run 8), no longer the row of the NeoStatus (run 12) that stage 1 inferred.

## 13. Status: stages 3 and 4 in the app (WP-P3b)

Files: `party.rs` (status block, hits, KO, revive, items, the battle branch's stage-4 steps, the panels),
`party_battle.rs` (pure rules: `hp_change`, `revive_time`, `revive_step`, `revive_heal`, `pref_row`, and
WP-P3's `mp_gate`, `danger_member`, `emergency`, `item_pick`, `cure_target`, `followup_dir`), `friend_ai.rs`
(the special's mode switch, the MP payment), `friend_host.rs` (hosts 1.140 and 1.276), `hud_party.rs` (doc).
Log `KH2_PARTYLOG=1`; harness `KH2_PARTY_HP=<n>` (members start with n HP), `KH2_PARTY_HIT=<s>:<atkp>,...`
(the nearest enemy lands that `atkp` row on every member at those seconds).

Done:

- **Status block**: HP/MP from the saved record (`Progress::member`: max HP and MP; [inferred] a new block
  starts full), the MP charge (+0x1bc/+0x1c0) counting down per frame (FUN_00175f48 -> FUN_001a0540, regen 1.0)
  and refilling MP at its end (FUN_001a05a8).
- **Hits** (`party_receive_hits`, FUN_001106b8 -> FUN_0017cde0 -> FUN_00175340 -> FUN_00185a68): the `atkp`
  row's damage on `enemy::member_stats` by the enemy's stats, at least 1 unless ineffective, through the HP
  method port `hp_change` (FUN_00176c98 -> FUN_00186238 / FUN_001a0100); voice 9 when the hit takes the HP
  into danger (FUN_001a0be0 before and after); the reaction from `reaction_rules::victim_outcome` played as a
  motion of the member's set through the player-type remap (pushes and the blow's launch not carried out).
  A KO'd member is not hit [inferred]. Log: `party: P_EX020 hit: atkp 17 damage 3 HP 2 -> 0 of 18 ...`.
- **KO** (FUN_00186530 -> FUN_0017ce60): voice 2 (FUN_00175f10), the revive timer = StunTime of the +0x12c
  row x Hyper Healing (1.0): Donald 2400, Goofy 2100 frames. While KO'd and on the ground: motion 0x36 and
  system effect 0xe bound to the body (FUN_0017ca70); the effect is told to end when the member gets up.
- **Revive** (FUN_0017ca70 -> FUN_0017cc20 -> FUN_0017cee8): the timer counts down by a frame in battle and by
  `sstm +0x5c` (8) frames out of battle; at 0 the heal `int(maxHP x sstm +0x58 (0.5))` goes through the HP
  method, which on a KO'd member clears the KO bit and scales the heal by `sstm +0x54` (0.5); any heal (an
  item) revives the same way; motion 0x37 (blend `DAT_0037a55c` = 8) when the member lay in 0x36.
- **HUD** (`party_hud`): `hud_party::PartyMembers` per slot from the members' HP and MP.
- **Battle branch** of FUN_00111070: the MP gate each frame (FUN_00110f90 with 0.75 / 0.25 from 0x378598 /
  0x37859c), the member in danger (FUN_0010f830), the emergency pass (FUN_0010f8d8, which sets the gate) before
  the choice, the status block in the feasibility (MP cost, MP charge).
- **Combo follow-ups** (FUN_0010c800): direction = yaw + angle (`DAT_003784a0` degrees to radians), motion 3
  (blend 0), a jump of the entry's height moving the entry's speed per frame until the member lands
  ([inferred] the app's motor gravity instead of FUN_001b0758's `2h / t^2`).
- **Items** (FUN_00113888 / FUN_00113b08 phase 1 `item_pick`, phase 0 FUN_0010c9d0 -> FUN_00175e20): motion 9
  (blend 8); at frame trigger 6 (counted from the motion's start, section 10) `Progress::take_item(item,
  NeoStatus)` (FUN_00175ec0); after 30 frames (FUN_001b82b8) `items::item_effect` on the target (Sora, the
  member, the other member; everyone for the group items, FUN_001a1fe0); a member's HP part goes through its
  HP method (so a Potion revives). The use effects are not started.
- **Specials and Cure** (FUN_00113d48 / FUN_00113e68 / FUN_00114828): phase 1 needs the record's mode, the
  script not busy with a special ([inferred] stand-in for +0x390) and FUN_0010f220 (ability equipped, MP gate);
  Cure also `cure_target` below the entry's +0x3c. Phase 0 sets +0xaf4 and switches the script's mode
  (`friend_ai`). Host **1.276** (0x170a00 -> FUN_001769e0 -> FUN_00186368) pays +0xaf4: MP drops, or the MP
  charge starts (MP 0, length = the row's MpDrive 1200 x `DAT_0037a974` 1.0) when MP is short (Goofy's `blade`
  pays 10 per use in a b_44 run). Host 1.140 (0x173740 -> FUN_0018ed58) answers the member's target record.

Not done: (Cure's heal and the specials' payment: section 14; Donald's thunder, fire, cure and Goofy's tornado
attack objects and effects: section 15; Donald's blizzard, Goofy's blade and turbo: section 16) the members'
ability block, the attacker memory +0xcec, the panel flashes, limits and forms (stages 5, 6).
- FUN_00111040 forces motion state 0x1f only for classes 12, 15 and 16, so Donald and Goofy (classes 0, 1)
  may follow through FUN_0010e5e8's body (state 0) rather than FUN_0010e140 as stage 1 assumed; which state a
  free class-0 member stands in was not determined. The field follow keeps stage 1's port.
- The random generator is not the game's.

## 14. Status: Cure and the specials' payment (WP-P3c)

- **Donald's specials pay.** While the script runs a special's mode (+0x390 busy, FUN_0010f698) FUN_0010fcd0 and
  FUN_0010f8d8 return at once, so the member no longer chooses or moves (Choice::Busy; [inferred] the steer's
  speed is held at 0 so the cast motion is not replaced). riend_ai fills the motion controller's trigger
  bytes +0x238 / +0x239 (hosts 1.150 / 1.151, as enemy_ai does for enemies, frames counted from the motion's
  start, section 10): the cast's wait R089a (P_EX020) waits for bit 0 of +0x239, set by frame trigger 9 of
  motion 192 (slot 768, tick 26). Log: party: P_EX020 paid 10 MP (host 1.276): MP 100 -> 90 (thunder),
  paid 255 MP (host 1.276): MP 100 -> 0, MP charge 1200 (cure).
- **The scripts' attack objects** (riend_host): 2.10 (0x18e2f0) keeps the tkp id by a handle; 2.17
  (0x18e6e0: FUN_0018c5f8 / FUN_0018c6d0, then the target's receive method +0x64) delivers it on the member's
  target record; 2.13 / 2.22 need nothing. party_script_hits carries the hits out: on an enemy, a
  combat::Hit; on a party object, attack types 5 and 6 are a heal (FUN_00185a68 adds the amount; FUN_00175340
  lets these types through even on a KO'd member), through the HP method, which revives a KO'd member
  (FUN_00186238 -> FUN_0017cee8, scaled by sstm +0x54). Cure's attack 1525 is type 6, Power 200, so the
  amount is (magic x 200 + 99) / 100 of Donald (GameData::damage).
- The record FUN_00113d48 copies into +0x9f4 is phase 1's: Cure's is cure_target (the member in danger, else
  the first below +0x3c), the others' the member's enemy target.
- Log: party: P_EX020 heals P_EX030 with atkp 1525 (type 6, FUN_00185a68): +2, HP 0 -> 1 (was KO'd: revived),
  party: P_EX020 heals Sora with atkp 1525 (type 6, FUN_00185a68): +2, HP 6 -> 8.
- [inferred] The caster's magic is the saved record's plus the hand-0 weapon's item_stat magic (the status
  rebuild's equipment pass, FUN_0019f920, as GameData::sora_stats adds the keyblade); with the new game's record
  Donald's magic is 1, so Cure heals 2.
- Harness: KH2_PARTY_HIT=<s>:<atkp>[:<model>] (the model limits the hit to one member).
- (Done in WP-S3, section 15: thunder's / fire's attack objects and the specials' effects; the caster's
  magic is the rebuilt status, so Cure heals 12, not 2.)

## 18. Status: the slot bytes across area loads, member rebuild, the third slot (WP-MN8)

Files: `party.rs` (`PartySlots::enter_world`, `wanted_members`, `member_key`, `spawn_party`).

- **Carry-over.** Before, every area load re-applied preset 1 to an empty party, so a Party-page swap was lost at the
  next area. The original keeps one four-byte record per world (`0x340c9c + world x 4`, FUN_001a71c0) in the save data;
  only the area-script command 0xf (FUN_001a7330, `rules::apply_preset`) rewrites it. The app's `PartySlots` now keeps a
  record per world it has entered: `enter_world(world, fresh)` stores the bytes of the world being left, restores the
  record of the world entered, and calls `fresh` (preset 1) only for a world with no record. [inferred] The app has no
  new-game writer and does not run command 0xf for these bytes (gap, section 10), so preset 1 stays the start value.
  Test `slot_bytes_survive_area_loads`.
- **Rebuild.** `spawn_party` also runs when the bytes change inside an area. `member_key(bytes, merged)` is the sorted
  list of objects the slot bytes 1..3 name through the merged `memt` (FUN_0017cd20, `rules::slot_object`); when it differs
  from the key the standing members were built for, their entities are despawned and the named members spawned again,
  each at the place of a standing member of the same NeoStatus, else at Sora's [inferred]. A swap of two slots (the Party
  page) keeps the key, so nothing is rebuilt: the entities already trade `slot` (`menu_party::swap_members`). Test
  `slot_changes_change_the_member_set`. Nothing in the app changes the set yet (Party In / Out, page 0x15, is a gap),
  so the rebuild path is covered by the key test only; it was not run in the game.
- **Slot 3.** [Superseded by WP-PY1, section 20: byte 3 is a reserve member and spawns nobody.] It spawned a third
  member (`PARTNER`) beside Sora [inferred: no map script row places a third member].

**Gaps** (not read): page 0x15 and its caller; the writers of the hidden-member list FUN_001c7be8 (the lock rule of `menu_party::rows` now reads the list, WP-PY5); the members' leave / enter animations; the model tweaks
of FUN_0023b5d0 / FUN_0023b5b0; a third follow point in FUN_00111670.

## 19. Status: no members in the Struggle (WP-S8)

Files: `party_rules.rs` (`mission_presets`), `party.rs` (`mission_party`).

- **The rule [V].** Nothing removes members at a mission's start: the members are never made. tt04's Struggle `btl`
  programs (0xb6, 0xb7, 0xb8; read from the disc) are `0xf [0]; 0x15 [mission]; 0x1d [1]; spawn b_5x; spawn b_60`,
  and their `map` programs spawn no group. Opcode 0xf (FUN_00180b48 -> FUN_001a7330) applies preset 0 `[0,-,-,-]`
  (section 1) to the world's slot bytes, which empties slots 1..3. The groups b_50 / b_51 / b_52 do hold FRIEND_1 /
  FRIEND_2 rows (0x237 / 0x238, spawn kind 0) beside the player's, but the factory resolves them through the slot
  bytes (FUN_0016e208 -> FUN_0017cd20: FUN_001a71c0(-1) the current world's record, FUN_001a71e8 the byte & 0x1f,
  0x10 -> no object), so nobody is created and Sora fights alone. The Seifer programs 0x4c, 0x4d, 0x4e and 0xb5
  also apply preset 0 (test output). Opcode 0x1d [1] (FUN_001abfe8 case 1) ends a drive form if one is active, else
  calls FUN_001ac130 on the status blocks 1..15 (FUN_001a0240, FUN_001a0100) [inferred: a heal back to full]; it
  removes nobody and is not ported here.
- **The return [V].** The slot bytes stay as preset 0 left them (the record survives area loads, section 18) until
  the next opcode 0xf or the Party page rewrites them; applying preset 1 again gives Donald and Goofy back
  (`apply_preset`).
- **The port.** The app does not run a `btl` program at area load: the wave spawner loads the group and sends
  `struggle::MissionStart`. `mission_party` reads that message, finds the area's `btl` program whose opcode 0x15
  names the mission (`rules::mission_presets`) and applies its 0xf presets to `PartySlots`; `spawn_party`'s
  rebuild (section 18) then despawns the members, since the bytes name nobody. Log: `party: mission 0x128
  TT04_MS00: its btl program applies preset 0 (opcode 0xf, FUN_001a7330): slot bytes [00, 01, 02, 10] -> [00, 10,
  10, 10]`, then `party: the slot bytes ... name other members than the 2 standing: rebuilding`. Tests
  `a_struggle_program_leaves_sora_alone`, `party_the_struggle_programs_leave_sora_alone`.
- **Order (WP-REG1).** The mission can start on the area's first tick, before `spawn_party` has entered the world for the
  area; the preset would then edit the previous world's bytes and `enter_world` would start the area over from preset 1,
  so Donald and Goofy fought in the Struggle (b_50 was won with no input). `mission_party` now keeps the start until
  `spawn_party` has built for the loaded area (`area_entered`). [inferred] The original runs the program before the area's
  objects exist, so the preset is always the last write; waiting for the area pass gives the same result. Test
  `mission_presets_wait_for_the_area_party_pass`. The queue (`drain_ready`) is dropped when the loaded area changes.
  Coverage is the pure helpers (`area_entered`, `drain_ready`, test `mission_starts_drain_once_and_die_with_their_area`)
  plus the accept scenario `struggle-b50-lose`; the system itself has no unit test.
- **Gaps.** [inferred] The app's rebuild removes the standing members at once (the original never created them,
  since the program runs before the area's objects). The app does not apply opcode 0xf at area load (the
  area-script run, `ScriptRun::world_records`, is collected but no system applies it), so after a Struggle the
  members stay out in world 2 for the session unless the Party page changes the bytes; in the original the next
  program with an 0xf preset would restore them. Opcode 0x1d's status reset is not ported.

## 20. Status: the area load applies opcode 0xf, the members return (WP-P3)

Files: `party_rules.rs` (`apply_world_records`), `party.rs` (`spawn_party`), `kh2-ard` disc test
`twilight_town_world_record_programs`.

- **The rule [V].** The area load runs the `map`, `btl` and `evt` programs the program table names (FUN_00181148,
  `kh2_ard::run_area`); each opcode 0xf in them reaches FUN_00180b48 case 0xf -> FUN_001a7330 on the current world's
  record (section 18), in program order, before the area's objects are made. `ScriptRun::world_records` holds the
  arguments; the app now applies them (`rules::apply_world_records`: argument = index into the trailing `memt`
  presets, then `apply_preset`) in `spawn_party` right after the world's bytes are restored (`enter_world`) and before
  the members are chosen, so the first build already names the right members. Log: `party: tt04: the area's programs
  apply presets [1] (opcode 0xf, FUN_001a7330): slot bytes [..] -> [..]`. An area whose programs have no 0xf leaves the
  bytes as the record holds them (a Party-page edit survives).
- **What the disc says [V, test `twilight_town_world_record_programs`, 187 programs].** Opcode 0xf is in the `btl`
  mission programs (preset 0 almost everywhere; preset 1 in tt04 0x50 mission 0xc2, tt07 0x19 / 0x1a, tt08 0x6c, tt14
  0xa6, tt25 0x95, tt29 0x98, tt30 0x99, tt40 0xa0, tt41 0xba) and in `evt` programs. The story `evt` programs split
  by party: tt02 `evt` 1 / 0x47 / 0x48, tt03 `evt` 5, tt04 `evt` 1, tt15 / tt25 / tt27 / tt28 / tt31 / tt38 / tt40
  `evt` 1 apply preset 1 (members present); the cut-scene programs (tt00 0x33, tt01 0x34..0x3d, tt02 0x3f..0x46,
  tt03 0x1..0x4b, tt04 0x51..0x53, tt05 1..5, ...) apply preset 0 (the event runs without them).
  Test `party_the_area_load_brings_the_members_back` (real tt04): bytes `[0,1,2,-]` -> the Struggle program
  (`run_area` with btl 0xb6) `[0,-,-,-]` -> the load of tt04 with programs (0, 0, 1) `[0,1,2,-]`, Donald and Goofy
  wanted again. Unit test `party_return_after_the_mission_program`.
- **In the app.** The programs of a load come from the program table (`progress::Story::programs`, zero in a new game:
  no program, no 0xf) or from a jump that names them (a type-2 jump to a story program sets map, btl and evt to it,
  docs/systems/events-movies/npc-savepoints.md section 4). The members therefore return when the next load's programs apply preset 1: a jump to
  tt04 program 0x50 (btl, mission 0xc2) or a table entry selecting `evt` 1. A load whose programs apply preset 0
  (the cut-scene programs) now empties the party, as in the original. A Struggle entered by a type-2 jump to 0xb6..0xb8
  applies preset 0 at the load already; the wave spawner's `mission_party` (section 19) then finds the same bytes.
- **Gaps.** Which program the story flags put into tt04's table after a Struggle ends (the table writers, op 1 of
  FUN_0017ff60, docs/systems/events-movies/npc-savepoints.md section 4) was not traced here, so in a new game with a zero table nothing restores the
  members after a Struggle until a jump or flag names a preset-1 program. The programs of other worlds are not in the
  app's area set. [inferred] A load that runs a mission `btl` program and `evt` program 1 together ends on preset 1
  (btl runs before evt, FUN_00181148).
## 21. Status: the jump record's party words and the notice page 0x15 (WP-PT1)

**The hook [V].** `FUN_0019d9d8` (a trigger's jump record, docs/systems/core-runtime/contract.md) reads word 4 (`record +0x10`), word 5 (`+0x14`) and word 6
(`+0x18`). After the destination is known: when word 4 is 0 or more it copies the destination world's four slot bytes to `DAT_01c8bda0`
(`FUN_001a71c0(world)`, the current world when the record has none) and runs `FUN_001a7330(record, word 4)`, the same rule as the
area-script command 0xf (section 1, `rules::apply_preset`, the preset being entry `word 4` of the end of `memt`). Then word 6: 1 calls
`FUN_0024f300(0, 0x1c8bda0)`; 2 first runs `FUN_001d73d0(previous, new)` and calls `FUN_0024f300(2, 0x1c8bda0)`; any other value
opens nothing. `FUN_00104e18(dest, fade, wait, 0, word 5)` gets wait 2 when the camp was opened, else 0 (`LAB_0019db28`); with a wait it
runs `FUN_001abfe8(word 5)` at once, without one the jump task runs it after the fade-out and before the area unloads
(`FUN_00104d58`, `+0x10`). The defaults of a record are word 4 = -1, word 5 = 0, word 6 = 0 (`FUN_0019d8d0`, called by `FUN_00197780` on the trigger's
record at trigger +0x34; the trigger's own +0x10 / +0x14 that `FUN_00197780` sets are other fields), so a trigger with no sub-command
4, 5 or 7 arranges nothing, resets nothing and opens no page. With no destination world (types 5 and 6) the arrangement applies to the
current world and `FUN_001abfe8(word 5)` runs at once (the argument register is loaded from the record at 0x19db2c; ported).

**The arrangement.** The app applies word 4 in `party::jump_party`: `PartySlots::edit_world(world, fresh, ..)` rewrites the bytes of
the destination world (the current bytes when it is the world standing, the saved record of another world, the default preset for a
world the app has not entered), with `rules::arrange` = `apply_preset` over `memt` trailing entry `word 4`. When the members the bytes
name differ from the ones standing, `spawn_party`'s rebuild (section 18) swaps them. Log: `party: jump record word 4 = 1: preset applied
to world 2 (FUN_001a7330): slot bytes [..] -> [..]`. 

**The reset `FUN_001abfe8(code)` [V].** `rules::reset_effect`: 0 returns. 4, 5 and 6 end the form (`FUN_001c75c0`: `DAT_0035ff30 = 0`,
the hidden-member count; `FUN_001ac170`: form bytes `DAT_00340c88` +4 and +5 cleared, the player's block reset) and return, 4 and 5
only when a form is active (the low half of `DAT_00340c88` +4), 5 also raising bit 0x40000 of `DAT_0035d268` (docs/systems/ui/contract.md, the
drive command locked); 0xb does the same only for form byte 5; 6 always. Every other code ends the form for 2, 0xc and 10 (and 1
when a form is active) and then runs the loop over the status blocks 1 to 15 (`FUN_0019fe48`): `FUN_001ac130` = `FUN_001a0240(block,
0)` (a running MP charge ends, MP set to its maximum) and `FUN_001a0100(block, maximum, 0)` (HP raised by its maximum, clamped: full
HP; the same pair `FUN_001a04b0` runs at a form start, docs/systems/drive-limits-summons/drive-forms.md). `party::jump_reset` applies the loop to Sora and the
standing members (not at 0 HP [inferred]) and pushes `DriveRequest::Revert` for the form end [inferred: the app's form end is the
Revert command, which refuses Anti by command]. Not ported: the lock bit 0x40000 (the app keeps no such flag, docs/systems/ui/contract.md), the
hidden-member count (the app's drive module keeps its own absorbed flags), and blocks of members the app does not run.

**Page 0x15 [V for the content, [inferred] for the frame].** `party_switch.rs`: `rules::switch_notice(previous, new)` is
`FUN_0026e970`: for each slot 0 to 3 whose member (`FUN_001a7200`: the `objentry` member id of the slot's object) is not 0, the
member is "in" when the stored record lacks it and "out" in the other direction; a slot is locked (icon code 10 before the name,
`FUN_002554c0(10, ..)`) for slot 0, a byte with bit 0x80, or a hidden member (`FUN_001a7268`); names are `FUN_001b5868(member)`
(table 0x37ba90) joined by the format at 0x381ad8; the last line (`FUN_0026e618`) is message 0xcc69 when `DAT_0036cd30` is 2, else the
message of table 0x381b08 for the member of the last locked slot 1 to 3. The page is the window text 0x46d "Party Members have been
switched." over "Party In" / "Party Out" (0x46e, 0x46f) and the two lists; a confirm or cancel closes it (`FUN_0026e8d0`,
`FUN_0026e158`, which goes on to page 0x14). The app draws a translucent
panel with the lines centred and holds the field (so the jump waits for the page, as the original's menu task does [inferred]).
Not found: the frame objects (`FUN_0029b6c0` with the groups of the words at 0x381af8 and 0x381afe) and the window geometry, the
meaning of `DAT_0036cd30` (the app passes false), the predicate `FUN_001d4e00` that swaps one member's line for 0xcc9f, and
`FUN_001d73d0`'s effect on members that arrive with code 2 (their growth blocks are cleared; the app keeps no block per absent member).

**Harness.** `KH2_JUMP=1:<area>:<entrance> KH2_JUMP_WORDS=<word 4>:<word 5>:<word 6>` (decimal, signed) gives the forced record its
words; without `KH2_JUMP_WORDS` the arrangement is -1. Example: `KH2_JUMP=1:0:0 KH2_JUMP_WORDS=0:-1:1 KH2_JUMP_AT=4 KH2_SHOT_AT=7`
(preset 0 empties the companions, word 6 = 1 opens the notice).
Tests: `a_record_word_four_arranges_the_party`, `the_party_reset_codes_follow_the_switch`, `the_notice_lists_who_came_in_and_who_went_out`,
`the_notice_slots_read_the_bytes` (party_rules), `the_last_line_follows_the_mode_and_the_locked_member`,
`the_executable_has_the_tables_and_the_texts_exist` (party_switch), `a_record_arrangement_edits_the_destination_world` (party).

## 20. Status: the class 16 chooser, the reserve slot, Auto Change, Auto Healing, Auto Limit (WP-PY1)

Files: `party_battle.rs` (`choose_class16`), `party_rules.rs` (`auto_change_swap`, `member_index`), `party.rs`
(`wanted_members`, `party_receive_hits`).

- **Class 16 chooser [V, partly modelled].** FUN_00119d58 is `return 1`: the class always re-targets
  (`Retest::Always`). FUN_00119d60 (Peter Pan's class, NeoStatus 202): (1) among the object walk's shapes that
  FUN_00112b28 and FUN_0018edd8 accept, whose object has +0x10c bit 0x20000 (set by FUN_00185460 when FUN_0018b778
  answers; cleared by FUN_001855a8) and +0xae4 equal to the player (`DAT_0034e908`), the nearest surface to the member
  (first while the best is 0, then a smaller distance); (2) none and FUN_00119ce8 true (`0x34e9d0` >= 0, the member's
  +0xb7c bit 0x200 clear, `0x34e9c8` + `DAT_003788d8` not below `0x34e9cc`; FUN_00112850(1) writes the two floats
  equal, so the time test reads as "the constant is not negative" [inferred]): FUN_0010d848's pick (the player's
  attack object, style 1's source); (3) the member's previous record (the second argument of FUN_00119d60 is the copy
  FUN_0010dcf0 made before clearing). Port: `choose_class16`; the scene carries `held` (empty: the app models neither
  +0x10c bit 0x20000 nor +0xae4), `attack_gate` (false: no +0xb7c bit 0x200) and `previous`. The shape record flag
  test (+8 bit 0x40, +0xc non-zero) is not modelled. No member of the app has class 16 (the friend AI classes 14..18
  are NeoStatus 200..204, run by the summon code), so the routine is covered by tests only. Test
  `party_target_class16_held_then_attack_then_previous`; `party_target_tables_from_the_executable` checks both table
  entries.
- **Slot 3 is a reserve member [V].** The field holds two companions: FUN_0017cd08 reads two entries of `DAT_01c893a8`,
  FUN_0017d400 loops over indices 0 and 1, the map script resolves only 0x237 / 0x238 (slot bytes 1 and 2,
  FUN_0016e208), the byte-3 id 0x3ee is not made on the field (FUN_0016efd8 is a flag test, see below), and the disc's presets 2 to 4
  put the partner in slot 1 and Donald in slot 3. Corrected from WP-MN8: `wanted_members` names slots 1 and 2 only and
  the `PARTNER` spawn is gone. A filled byte 3 spawns nobody; a Party-page swap with slot 3 changes the key and rebuilds.
  Harness: `KH2_JUMP_WORDS` presets 2, 3 or 4 give a filled byte 3 [not run in the game by this WP].
  WP-PL1 correction of the reading of `FUN_0016efd8`: it is a plain id test, true for 0x319, 0x31a and 0x3ee and for an id
  with bit 29 set (`FUN_0016f008` is the same test for bit 28), not a "menu stand-in" list. Its readers: `FUN_0016e430`
  and `FUN_0016f038` (the weapon-child attach: the hand objects are looked up only when the id is not one of these),
  `FUN_001a7808`, `FUN_0016eda8`, `FUN_0016e638`, and the factory `FUN_0016e8c0`, which returns 0 for such an id (after the
  `FUN_0016ef80` branch) before it resolves anything. So the factory does refuse a raw 0x319/0x31a/0x3ee, but that refusal
  is the flag's effect, and the label "menu stand-in" is [inferred]. The id-to-member mapping is `FUN_0016e208`: 0x319 ->
  `FUN_0017cd78(1)`, 0x31a -> `FUN_0017cd78(2)`, **0x3ee -> `FUN_0017cd78(3)`**, the slot-3 member (the same as 0x237 /
  0x238 -> `FUN_0016e208`'s `FUN_0017cd20(1 / 2)` for slots 1 and 2); the slot-3 member is therefore a real object id
  elsewhere (the weapon-child paths), but nothing on the field loop asks for it (FUN_0017cd08 / FUN_0017d400 take two).
- **Auto Change [V rule, ported with the gaps below].** FUN_0017ce60 (KO method): ability 418 (bit 130) in the KO'd
  member's block, FUN_001b42f0 (no limit, byte 3 filled, `0x35fec0` clear), FUN_0017d0f0 (FUN_0017d0a8 on the member
  before its KO bit is set by FUN_00175f10; FUN_001a7698 answers no: the member's slot is not 0, lacks the 0x80
  flag and the member is not hidden) start FUN_001b4170: after the fade FUN_001b4260 runs FUN_001a7558 (slot 1 or 2
  holding the member index takes byte 3's index, byte 3 takes the member's index, bytes masked 0x9f) and the new member
  is made (FUN_0017d290). Port: `rules::auto_change_swap` changes the slot bytes at the KO in `party_receive_hits`
  (ability read from the saved record's list), `spawn_party`'s rebuild replaces the member (at Sora's place). Not
  modelled: the 8-frame fade, `0x35fec0` bit 2, +0x108 bit 0x100, FUN_0017d030 (no object with +0x570 set to the member:
  [inferred] satisfied), the limit lock, the hidden list. Test `auto_change_swaps_the_reserve_in`.
- **Host 10.35 for the members (WP-PY2, was the `P_EH000` `BadAddress(8) at 0x058e`).** FUN_002f71f0 turns the first word into the AI (FUN_0010c5f0: +0xb60 of the object) and calls FUN_001141d0(AI, id): the object at AI +8 is the class parameter block (FUN_001133e0 stores it; FUN_00113450 binds the action records to its entries the same way), and the answer is the address of the first 0x80-byte entry (+0xa4 list, +0xa0 count) whose first word is the id, else 0. Port: `ClassAi::entries_at` / `entries` hold the class's whole list as the executable has it; `friend_ai` hands the member's class list to `FriendFields`; `friend_host::group10` index 35 answers `entry_address`, and `read_mem` serves the bytes at the answered executable address (so `ldf +0x8` reads the entry's MP cost word). Tests `host_10_35_finds_the_entry_with_the_id`, `host_10_35_on_rikus_class_list` (class 11, id 111 found at 0x351c10, an unknown id gives 0). Harness: `KH2_JUMP_WORDS=4:0:0 KH2_JUMP=1:0:0 KH2_JUMP_AT=4 KH2_BDXLOG=1 target\debug\kh2-game.exe tt02`: no script error for `P_EH000`, `10.35 Impl x2`; its next missing hosts are 10.50 (x75), 10.58, 1.244 `in_battle`.
- **Gap: Goofy `P_EX030#1 BadAddress(0) at 0x0e14` in the summon-chicken scenario is not 10.35.** The log shows 1.161 `has_link` and 1.83 `bone_to_world` called with `Obj(0)` (the script's weapon/shield object: `party: P_EX030: no weapon (item 0: no went entry for moveset 20)`), then the vector read at address 0. Cause: the member's weapon object does not exist in that state (saved record weapon item 0, no `went` entry); owner is the weapon spawn in `party.rs` / the summon scenario state, plus 1.83 and 1.161 which the friend host does not implement.
- **Auto Healing [V, ported by WP-AH1].** FUN_0017d4b8 (called at 0x1b77e4, inside the object tick `FUN_001b74e0`'s
  neighbourhood in a function the decomp lacks; the exact caller and its gate were not read, so the app runs it every
  party tick while a party stands, `party_auto_healing`) lowers `DAT_0035cc08` (cleared at boot, so the first tick
  runs the pass) by the frame step `DAT_00356f40`; at or below 0 it visits slots 1..3 (`FUN_001a7200(record, i)`, the member
  the slot byte names through the world's `memt`). A member who is not one of the two field actors (`FUN_0017d400` loops
  `FUN_0017cd08(0..1)`, comparing `FUN_001699a8`) gets his status block (`FUN_0019fe48` -> `FUN_0019fd30`: find a live block
  of the member `FUN_0019fbc8`, else make one and fill it from the record, `FUN_0019f920`); if `+0x1dc & 0x10` (ability 420)
  it calls `FUN_001a0140(block, 1, 0, 0)`; then `FUN_0019ffb8` releases the block. Then the timer takes `sstm` +0x168.
  `FUN_001a0140` calls the HP method through the vtable of the actor in block `+0x24c` or, with none (a reserve member has no actor),
  `FUN_001a0100`: `current + 1` held between the minimum (word 2) and the maximum (word 1). The release writes the block back
  into the record (`FUN_001a0030`: `+4` HP, `+5` max, `+6` MP, `+7` max MP, `+0xc` / `+0xd` drive bytes), so the heal is
  persistent. Port: `rules::auto_heal_due` (timer), `rules::reserve_members` (slots 1..3 not standing), `rules::clamped_add`
  (FUN_001a0100), `Progress::heal_reserve_member`; the ability is read from the saved record's list as for Auto Change.
  Log line `party: Auto Healing (FUN_0017d4b8): reserve member <neo> HP <a> -> <b>`. Tests
  `auto_healing_timer_clamp_and_reserve_set`, `member_vitals_write_back_heal_and_round_trip`.
- **Member HP and MP outside the field [V, WP-AH1].** The original keeps them in the party record (`+4` HP, `+6` MP, held to
  `+5`, `+7`); a status block is a pooled object (`DAT_01c8bdb0`, 600 bytes each, `+0x248` the use count) rebuilt from the record
  by `FUN_0019f920` (HP and MP each `min(maximum, current)`) and written back by `FUN_001a0030` when it is released
  (`FUN_0019ffb8` at count 0, the field actor's destructor `FUN_00174998`, `FUN_0019fb98`). `Progress` already holds the
  record in its `block` key, so no new slot key exists: an old slot file carries the bytes as they were. Port:
  `Progress::member_vitals` / `set_member_vitals`, `party_record_sync` (writes a standing member's HP and MP into the record
  each tick one of them changed; the original writes at the release, [inferred] equal because nobody reads the record of a
  standing member), `spawn_party` starts a member from the record (`rules::spawn_vitals`). Gap: a stored HP of 0 (the
  member left the field knocked out) starts at the maximum, the app's earlier start; what the original actor does with a
  block of HP 0 was not traced (`FUN_0017d290`, `FUN_0017ca70` read; no HP-0 test found). **Open question (WP-AH1b):** what the original does with a stored HP of 0 at area entry. The current rule spawns the member at his maximum HP; with Auto Healing a reserve member at 0 HP first gets 1 HP (`FUN_001a0100` allows 0 to 1) and then spawns at 1 HP, so the same member comes back at the maximum or at 1 depending on the ability. Game over (WP-GO1) restores Sora's HP only;
  the original restores the whole save body, so after Continue the members keep the HP they had.
- **Auto Limit [V rule, ported by WP-AL1].** FUN_001d25e0 offers a partner's limit as a reaction command; `reaction_cmd::auto_limit_offer`
  and `limit.rs` port it (rule, gates and gaps: docs/systems/drive-limits-summons/limits.md "Auto Limit (WP-AL1)").
- **Limit hide gaps (WP-PY5 review).** FUN_001b28a0 hides only party objects 0 and 1 and only when +0x588 bit 0x400000 is clear and +0xb4c is zero; the port hides every member that is not away and ignores both conditions. FUN_001b37f8 brings the members back only when FUN_001b3bc0 returns 0; with +0x40 bit 0x10 set it calls FUN_001c75c0, which clears the hidden list without showing anyone. The port always shows them. A pause-menu scenario with a locked row was not run (unit test only).
