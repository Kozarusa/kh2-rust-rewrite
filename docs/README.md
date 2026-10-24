# kh2-rs documentation index

How the documents are organized: one folder per system under `docs/systems/`, in the order of the ladder (`docs/ROADMAP.md` section 5). Each folder starts with `contract.md` (at most about 150 lines: what the system does, its state with owners, entry points with
original addresses, open gaps, and a table of the detail files); the detail files are the long evidence and status notes, split by subject.
Agents read the contract first and open a detail file only for the part they need. Section numbers in references ("section 20.14") are the old
chapter numbers that still stand in the headings; the contract table says which file holds which.

Other documents: [METHOD.md](METHOD.md) (the Spreadsheet Method, the ledger, the diff rig and the ladder), [CHANGELOG.md](CHANGELOG.md) (index of
`changelog/shift-NN.md`, one file per shift). `sheets/systems.json` has one row per system id (below); its `spec` column names the doc that
holds the details. The function ledger (`sheets/functions.json`) and the scenario folders (`scenarios/<system>/`) arrive with the later Phase 0
packages; until then the "scenarios" column is empty.

## Ladder and systems

| step | system | contract | ledger system | crates | scenarios | detail files |
|---|---|---|---|---|---|---|
| L0 | Foundations: formats, tables, rendering, oracle, research tools | [contract](systems/foundations/contract.md) | `foundations` | kh2-iso, kh2-archive, kh2-bar, kh2-ps2, kh2-tables, kh2-sheets, kh2-model, kh2-image, kh2-anim, kh2-ee, kh2 | `scenarios/foundations/` (planned) | 7 files in [systems/foundations/](systems/foundations/) |
| L1 | Core runtime: saved state, story flags, areas and jumps, collision | [contract](systems/core-runtime/contract.md) | `core-runtime` | kh2-ard, kh2-map, kh2-game-core (progress, area, world, schedule), kh2-event (area_jump) | `scenarios/core-runtime/` (planned) | 4 files in [systems/core-runtime/](systems/core-runtime/) |
| L2 | Player and camera: Sora controller, attack selection, field and event camera | [contract](systems/player-camera/contract.md) | `player-camera` | kh2-anim, kh2-player (sora), kh2-game-core (motor, camera*), kh2-event (intro) | `scenarios/player-camera/` (planned) | 4 files in [systems/player-camera/](systems/player-camera/) |
| L3 | Combat and effects: damage, reactions, effects, prizes, abilities, player data | [contract](systems/combat-effects/contract.md) | `combat-effects` | kh2-fx, kh2-tables, kh2-game-core (combat, fx*, reaction_rules), kh2-player (clash, prize, abilities) | `scenarios/combat-effects/` (planned) | 6 files in [systems/combat-effects/](systems/combat-effects/) |
| L4 | Magic | [contract](systems/magic/contract.md) | `magic` | kh2-player (magic), kh2-game-core (magic_rules), kh2-bdx | `scenarios/magic/` (planned) | 2 files in [systems/magic/](systems/magic/) |
| L5 | Drive forms, limits, reaction commands, summons | [contract](systems/drive-limits-summons/contract.md) | `drive-limits-summons` | kh2-player (drive), kh2-party (limit*, summon, reaction_*), kh2-game-core (reaction_*), kh2-bdx | `scenarios/drive-limits-summons/` (planned) | 8 files in [systems/drive-limits-summons/](systems/drive-limits-summons/) |
| L6 | Party: Donald, Goofy, the world partners, friend AI | [contract](systems/party/contract.md) | `party` | kh2-game-core (party*), kh2-menu (party*), kh2-party (party*, friend_*, mickey), kh2-tables | `scenarios/party/` (planned) | 4 files in [systems/party/](systems/party/) |
| L7 | Script host and enemies: BDX, host calls, enemy kinds, area survey | [contract](systems/script-host/contract.md) | `script-host` | kh2-bdx, kh2-enemy (enemy*), kh2-event (npc), kh2-ard | `scenarios/script-host/` (planned) | 13 files in [systems/script-host/](systems/script-host/) |
| L8 | UI: HUD, command menu, pause menu, shops, journal, save, game over | [contract](systems/ui/contract.md) | `ui` | kh2-image, kh2-game-core (hud*), kh2-hud (hud*), kh2-player (hud*), kh2-menu (menu*), kh2-party (items), kh2-event (savepoint) | `scenarios/ui/` (planned) | 21 files in [systems/ui/](systems/ui/) |
| L9 | Audio | [contract](systems/audio/contract.md) | `audio` | kh2-audio, kh2-sound (audio, audio_bgm), kh2-app (sound_test) | `scenarios/audio/` (planned) | 2 files in [systems/audio/](systems/audio/) |
| L10 | Events, NPCs and movies | [contract](systems/events-movies/contract.md) | `events-movies` | kh2-ard, kh2-event (event_player, npc, savepoint) | `scenarios/events-movies/` (planned) | 3 files in [systems/events-movies/](systems/events-movies/) |
| L11 | World mechanics and minigames (today: the Struggle) | [contract](systems/world-minigames/contract.md) | `world-minigames` | kh2-event (struggle), kh2-hud (hud_mission), kh2-menu (menu_struggle) | `scenarios/world-minigames/` (planned) | 1 files in [systems/world-minigames/](systems/world-minigames/) |
| L12 | World map and Gummi | [contract](systems/worldmap-gummi/contract.md) | `worldmap-gummi` | kh2-ard (worldmap), kh2-worldmap (worldmap, worldmap_ui) | `scenarios/worldmap-gummi/` (planned) | 3 files in [systems/worldmap-gummi/](systems/worldmap-gummi/) |

`L0` is the foundation layer (Phase 0 and the file formats); `L1` to `L12` are the ladder steps of the master plan, in building order. Exactly one step
is open for code at a time.

## The 56 sheet ids

| id | what | crate (sheet) | system | contract | detail files named by the sheet | scenarios |
|---|---|---|---|---|---|---|
| `fs.iso` | ISO9660 disc reader | kh2-iso | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `fs.archive` | KH2.IDX/IMG archive: hashes, LZ decompression, world sub-indexes | kh2-archive | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `fs.bar` | BAR container | kh2-bar | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `ps2.vif` | VIF1 unpack into VU1 memory, DMA chains, VPU packets | kh2-ps2 | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `ps2.gs` | GS swizzle (PSMCT32 upload, PSMT8/PSMT4 read), CLUT addressing | kh2-ps2 | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `data.battle` | 00battle.bin tables | kh2-tables | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `data.system` | 03system.bin tables | kh2-tables | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `data.objentry` | Object registry | kh2-tables | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `render.model` | Skeletal MDLX models in rest pose (bones, weighted vertices) | kh2-model | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `render.texture` | Model textures (BAR type 7) | kh2-model | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `render.uvscroll` | Texture animation (UVSC scroll, TEXA sprites) | kh2-model | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `ui.image` | Standalone images (imd, imz, tm2, 2dd, 2ld) | kh2-image | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `anim.motion` | MSET slots + state fallback, ANB interpolated/raw motions, F-curves, c... | kh2-anim | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `anim.triggers` | Motion triggers (range + frame) in global 60 Hz frames: hitboxes, comb... | kh2-anim | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `anim.ik` | Control rig: one/two-bone IK chains, constraints (position, orientatio... | kh2-anim | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `anim.expressions` | Rig expressions (trig in degrees, curve-at-frame, joint distance, cond... | kh2-anim | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `dev.ee` | EE interpreter oracle: runs the game's own routines from the user's ex... | kh2-ee | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `game.combat` | Hit detection from objectcollision shapes + motion triggers; KH2 damag... | kh2-app | L3 `combat-effects` | [contract](systems/combat-effects/contract.md) | see the contract | `scenarios/combat-effects/` |
| `game.sora` | Sora: locomotion (prty speeds/turn), jump/fall/land, 3-hit + finisher ... | kh2-app | L2 `player-camera` | [contract](systems/player-camera/contract.md) | see the contract | `scenarios/player-camera/` |
| `game.sora.dodge` | Growth abilities: High Jump, Quick Run, Aerial Dodge, Glide from fmab ... | kh2-app | L2 `player-camera` | [contract](systems/player-camera/contract.md) | see the contract | `scenarios/player-camera/` |
| `game.sora.magic` | Magic phase 1: command menu Magic list, cast motions, MP cost and char... | kh2-app | L4 `magic` | [contract](systems/magic/contract.md) | see the contract | `scenarios/magic/` |
| `game.sora.abilities` | Abilities, Drive Forms, Limits, Summons, reaction commands, items | kh2-app | L3 `combat-effects` | [contract](systems/combat-effects/contract.md) | see the contract | `scenarios/combat-effects/` |
| `game.enemies` | Shadow (M_EX020), Soldier (M_EX010), Dusk (M_EX990, enmp 41) and N_EX5... | kh2-app | L7 `script-host` | [contract](systems/script-host/contract.md) | see the contract | `scenarios/script-host/` |
| `game.ai_bdx` | Enemy AI scripts (BDX): the game's own scripts run in an interpreter f... | kh2-ard | L7 `script-host` | [contract](systems/script-host/contract.md) | see the contract | `scenarios/script-host/` |
| `fx.effects` | PAX/DPX/DPD effects: particle program runtime (birth, parameter cursor... | kh2-fx | L3 `combat-effects` | [contract](systems/combat-effects/contract.md) | see the contract | `scenarios/combat-effects/` |
| `world.map` | Map geometry + sky (MAP/SK0) | kh2-model | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `world.collision` | Map collision from COCT (stored as model space with Y and Z negated); ... | kh2-map | L1 `core-runtime` | [contract](systems/core-runtime/contract.md) | see the contract | `scenarios/core-runtime/` |
| `world.bob` | Background objects (BOB/BOP) | kh2-map | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `world.event` | Area scripts (ard): spawns, entrances, events; AI (bdx) | kh2-ard | L1 `core-runtime` | [contract](systems/core-runtime/contract.md) | see the contract | `scenarios/core-runtime/` |
| `ui.msg` | Message text | kh2-msg | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `audio` | Music, instruments, SFX, voice (bgm, wd, seb, vag) | kh2-audio | L9 `audio` | [contract](systems/audio/contract.md) | see the contract | `scenarios/audio/` |
| `game.player` | Player movement at 60 Hz: digital walk/run (stick 0.8), instant ground... | kh2-app | L2 `player-camera` | [contract](systems/player-camera/contract.md) | see the contract | `scenarios/player-camera/` |
| `app` | Bevy application: boots from the ISO, area switching, HUD | kh2-app | L0 `foundations` | [contract](systems/foundations/contract.md) | see the contract | `scenarios/foundations/` |
| `game.progress` | Game state: inventory, battle items, magic, abilities, forms, equipmen... | kh2-app | L1 `core-runtime` | [contract](systems/core-runtime/contract.md) | see the contract | `scenarios/core-runtime/` |
| `game.items` | Command manager lists (deck, second deck, magic, items, forms, targets... | kh2-app | L8 `ui` | [contract](systems/ui/contract.md) | [items.md](systems/ui/items.md) | `scenarios/ui/` |
| `ui.menu` | Pause menu: top list, Status, Abilities, Items and Stock, Customize, E... | kh2-app | L8 `ui` | [contract](systems/ui/contract.md) | see the contract | `scenarios/ui/` |
| `game.party` | Party members (Donald, Goofy): spawn from the area script, follow, war... | kh2-app | L6 `party` | [contract](systems/party/contract.md) | see the contract | `scenarios/party/` |
| `game.struggle` | The Struggle (mission manager: orb counters, clock, orbs, script, HUD)... | kh2-app | L11 `world-minigames` | [contract](systems/world-minigames/contract.md) | [struggle.md](systems/world-minigames/struggle.md) | `scenarios/world-minigames/` |
| `game.summons` | Summons: gauge mode 3, Summon and Dismiss commands, experience, summon... | kh2-app | L5 `drive-limits-summons` | [contract](systems/drive-limits-summons/contract.md) | see the contract | `scenarios/drive-limits-summons/` |
| `game.prizes` | Prize objects: orbs, munny, item boxes, Master Form experience, collec... | kh2-app | L3 `combat-effects` | [contract](systems/combat-effects/contract.md) | [prizes.md](systems/combat-effects/prizes.md) | `scenarios/combat-effects/` |
| `game.story` | Story flags and area programs: flag test/set/clear, the flag side-effe... | kh2-app | L1 `core-runtime` | [contract](systems/core-runtime/contract.md) | [story-flags.md](systems/core-runtime/story-flags.md) | `scenarios/core-runtime/` |
| `game.events` | Event player: area events with actors, placements, motions, subtitles,... | kh2-app | L10 `events-movies` | [contract](systems/events-movies/contract.md) | [event-player.md](systems/events-movies/event-player.md) | `scenarios/events-movies/` |
| `world.talk` | NPC talk, save point offers and heal, talk camera | kh2-app | L10 `events-movies` | [contract](systems/events-movies/contract.md) | [npc-savepoints.md](systems/events-movies/npc-savepoints.md) | `scenarios/events-movies/` |
| `ui.save` | Save menus: kind 1 (save point screen), kinds 7 and 8 (save.2ld files) | kh2-app | L8 `ui` | [contract](systems/ui/contract.md) | [menus-pause.md](systems/ui/menus-pause.md), [menus-save.md](systems/ui/menus-save.md) | `scenarios/ui/` |
| `game.gameover` | Game over: the knock-out decision, the Continue / Load Game and Quit /... | kh2-app | L8 `ui` | [contract](systems/ui/contract.md) | see the contract | `scenarios/ui/` |
| `ui.load` | Load screen (menu kind 2) on the slot files the Save menu writes | kh2-app | L8 `ui` | [contract](systems/ui/contract.md) | [menus-save.md](systems/ui/menus-save.md) | `scenarios/ui/` |
| `ui.journal` | Jiminy's Journal (menu kind 3): world grid, story page | kh2-app | L8 `ui` | [contract](systems/ui/contract.md) | [menus-journal.md](systems/ui/menus-journal.md) | `scenarios/ui/` |
| `ui.shop` | Shops: Buy list, quantity, moogle synthesis (recipes, materials, point... | kh2-app | L8 `ui` | [contract](systems/ui/contract.md) | [menus-shop.md](systems/ui/menus-shop.md) | `scenarios/ui/` |
| `game.friend_ai` | Friend AI target styles 0 to 5 (Technic, Target, Huddle, Party, Sora, ... | kh2-app | L6 `party` | [contract](systems/party/contract.md) | [party-friend-ai.md](systems/party/party-friend-ai.md), [menus-customize.md](systems/ui/menus-customize.md) | `scenarios/party/` |
| `ui.tutorial` | Tutorial windows opened by mission scripts | kh2-app | L8 `ui` | [contract](systems/ui/contract.md) | see the contract | `scenarios/ui/` |
| `game.intro` | Event camera tracks and the battle intro (camera, effect, INFORMATION ... | kh2-app | L2 `player-camera` | [contract](systems/player-camera/contract.md) | [event-player.md](systems/events-movies/event-player.md), [struggle.md](systems/world-minigames/struggle.md) | `scenarios/player-camera/` |
| `fx.lens` | Lens and distortion effect draw (fx_lens, wgsl shaders) | kh2-app | L3 `combat-effects` | [contract](systems/combat-effects/contract.md) | [effects-damage.md](systems/combat-effects/effects-damage.md) | `scenarios/combat-effects/` |
| `game.jump` | Jump records: jump types 4 to 6, the party arrangement (word 4), the p... | kh2-app | L1 `core-runtime` | [contract](systems/core-runtime/contract.md) | [party-built.md](systems/party/party-built.md) | `scenarios/core-runtime/` |
| `ui.battle_report` | BATTLE REPORT ending screen (menu kind 0xe) and its counters | kh2-app | L8 `ui` | [contract](systems/ui/contract.md) | [menus-shop.md](systems/ui/menus-shop.md) | `scenarios/ui/` |
| `dev.assets` | Asset viewer (F12, KH2_ASSETS=1): models, motions, textures, effects, ... | kh2-app | L0 `foundations` | [contract](systems/foundations/contract.md) | [assets-viewer.md](systems/foundations/assets-viewer.md) | `scenarios/foundations/` |
| `dev.sound` | Sound viewer (F10): steps through every sound and plays it through the... | kh2-app | L9 `audio` | [contract](systems/audio/contract.md) | [audio.md](systems/audio/audio.md) | `scenarios/audio/` |

`game.sora.abilities` names drive forms, limits, summons and reaction commands as well; its detail lives in `combat-effects` (abilities, player data) and
`drive-limits-summons`. The world map and the Gummi flight (step L12) have no sheet row yet.

## Where the old documents went

Before Phase 0 the documents sat flat in `docs/`. Every old name now maps to these files (references in code and docs were rewritten to the new paths).

| old document | new place |
|---|---|
| `abilities.md` | [abilities.md](systems/combat-effects/abilities.md) |
| `area-survey.md` | [area-survey.md](systems/script-host/area-survey.md) |
| `assets-viewer.md` | [assets-viewer.md](systems/foundations/assets-viewer.md) |
| `audio.md` | [audio.md](systems/audio/audio.md) |
| `bdx.md` | [bdx-format.md](systems/script-host/bdx-format.md), [bdx-host-functions.md](systems/script-host/bdx-host-functions.md), [bdx-scripts.md](systems/script-host/bdx-scripts.md) |
| `camera.md` | [camera-event.md](systems/player-camera/camera-event.md), [camera.md](systems/player-camera/camera.md) |
| `drive-forms.md` | [drive-forms.md](systems/drive-limits-summons/drive-forms.md) |
| `dynamic-tracing.md` | [dynamic-tracing.md](systems/foundations/dynamic-tracing.md) |
| `effects-damage.md` | [effects-damage.md](systems/combat-effects/effects-damage.md) |
| `enemies.md` | [enemies-al-n.md](systems/script-host/enemies-al-n.md), [enemies-core.md](systems/script-host/enemies-core.md), [enemies-m-ex0-4.md](systems/script-host/enemies-m-ex0-4.md), [enemies-m-ex5.md](systems/script-host/enemies-m-ex5.md), [enemies-m-ex6.md](systems/script-host/enemies-m-ex6.md), [enemies-m-ex7.md](systems/script-host/enemies-m-ex7.md), [enemies-m-ex8-9.md](systems/script-host/enemies-m-ex8-9.md) |
| `enemy-ai.md` | [enemy-ai-host-calls.md](systems/script-host/enemy-ai-host-calls.md), [enemy-ai.md](systems/script-host/enemy-ai.md) |
| `events.md` | [areas.md](systems/core-runtime/areas.md), [story-flags.md](systems/core-runtime/story-flags.md), [event-player.md](systems/events-movies/event-player.md), [events-plan.md](systems/events-movies/events-plan.md), [npc-savepoints.md](systems/events-movies/npc-savepoints.md) |
| `final-mix.md` | [final-mix.md](systems/foundations/final-mix.md) |
| `footsteps.md` | [footsteps.md](systems/audio/footsteps.md) |
| `gummi.md` | [gummi.md](systems/worldmap-gummi/gummi.md) |
| `hit-reaction.md` | [hit-reaction.md](systems/combat-effects/hit-reaction.md) |
| `hit-stop.md` | [hit-stop.md](systems/combat-effects/hit-stop.md) |
| `hud.md` | [hud-banners-windows.md](systems/ui/hud-banners-windows.md), [hud-notes.md](systems/ui/hud-notes.md), [hud-panels.md](systems/ui/hud-panels.md) |
| `items.md` | [items.md](systems/ui/items.md) |
| `limits-reactions.md` | [limits.md](systems/drive-limits-summons/limits.md), [reaction-commands.md](systems/drive-limits-summons/reaction-commands.md) |
| `magic.md` | [magic-spells.md](systems/magic/magic-spells.md), [magic-turn.md](systems/magic/magic-turn.md) |
| `menus.md` | [menus-abilities.md](systems/ui/menus-abilities.md), [menus-battle-report.md](systems/ui/menus-battle-report.md), [menus-config.md](systems/ui/menus-config.md), [menus-customize.md](systems/ui/menus-customize.md), [menus-equipment.md](systems/ui/menus-equipment.md), [menus-gameover.md](systems/ui/menus-gameover.md), [menus-items.md](systems/ui/menus-items.md), [menus-journal-pages.md](systems/ui/menus-journal-pages.md), [menus-journal.md](systems/ui/menus-journal.md), [menus-new-help.md](systems/ui/menus-new-help.md), [menus-party.md](systems/ui/menus-party.md), [menus-pause-m1.md](systems/ui/menus-pause-m1.md), [menus-pause.md](systems/ui/menus-pause.md), [menus-save.md](systems/ui/menus-save.md), [menus-shop.md](systems/ui/menus-shop.md), [menus-status.md](systems/ui/menus-status.md), [menus-struggle-result.md](systems/ui/menus-struggle-result.md) |
| `movement.md` | [collision.md](systems/core-runtime/collision.md), [movement.md](systems/player-camera/movement.md) |
| `moveset-selection.md` | [moveset-selection.md](systems/player-camera/moveset-selection.md) |
| `party.md` | [party-built.md](systems/party/party-built.md), [party-data.md](systems/party/party-data.md), [party-friend-ai.md](systems/party/party-friend-ai.md), [party-specials.md](systems/party/party-specials.md) |
| `pcsx2.md` | [pcsx2.md](systems/foundations/pcsx2.md) |
| `player-data.md` | [player-data.md](systems/combat-effects/player-data.md) |
| `prizes.md` | [prizes.md](systems/combat-effects/prizes.md) |
| `progress.md` | [progress.md](systems/core-runtime/progress.md) |
| `render.md` | [render.md](systems/foundations/render.md) |
| `research.md` | [research.md](systems/foundations/research.md) |
| `struggle.md` | [struggle.md](systems/world-minigames/struggle.md) |
| `summons.md` | [summons-objects-1.md](systems/drive-limits-summons/summons-objects-1.md), [summons-objects-2.md](systems/drive-limits-summons/summons-objects-2.md), [summons-objects-3.md](systems/drive-limits-summons/summons-objects-3.md), [summons-spec.md](systems/drive-limits-summons/summons-spec.md), [summons-stages.md](systems/drive-limits-summons/summons-stages.md) |
| `tables-unfinished.md` | [tables-unfinished.md](systems/foundations/tables-unfinished.md) |
| `worldmap.md` | [worldmap-plan.md](systems/worldmap-gummi/worldmap-plan.md), [worldmap.md](systems/worldmap-gummi/worldmap.md) |
| `CHANGELOG.md` | [CHANGELOG.md](CHANGELOG.md) (index) and [changelog/](changelog/) |
| `METHOD.md` | [METHOD.md](METHOD.md) |
