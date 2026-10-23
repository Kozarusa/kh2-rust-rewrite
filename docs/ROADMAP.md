# kh2-rs master plan

Written 2026-10-22 after a full read of the repository, `local/`, the sister workspace `../kh2_ps2` and an interview with the user.
Moved from `local/PLAN.md` to `docs/ROADMAP.md` at Phase 0 step R6 (2026-10-22). The specs it names (`local/specs/*.md`) stay untracked.
Precedence stays as in `agents/RULES.md`: the user's latest message, then RULES, then this plan. Section 9 lists the RULES changes this plan
proposes; the user confirmed them on 2026-10-22 and they are in `agents/RULES.md`.

## 1. Mission and definition of done

Mission (unchanged, RULES A1): a from-scratch Rust rewrite of Kingdom Hearts II, PS2 USA (`SLUS_210.05`), reading everything from the user's
own disc at run time, indistinguishable from the original in what the player sees and hears.

Decided by the user in the interview (2026-10-22):
- Done means the full USA game, faithful: boot, title, new game, every world, area, enemy, boss, minigame, in-engine event, movie, Gummi
  mission and the credits, as the USA disc does them. No modern extras, no Final Mix content (A3 stands).
- Order: all systems first, then the content. Systems are completed one after another, not side by side ("sequential order" was the user's
  main complaint: systems were left unfinished while agents moved on to other things).
- Play-testing by the user is rare. Proof comes from an automated original-versus-port comparison rig, built early.
- Reorganize fully: process files, tracked tooling, the app crate split, and a function coverage ledger.
- Push `main` and `integration` to GitHub at every wrap-up.
- The model policy is set by this plan and confirmed once by the user (section 8).
- `../kh2_ps2` stays the private reverse-engineering workspace; `../AGENTS_ROUTING.md` (Pi AI / OpenCode) is stale and ignored.

The measurable definition of done:
1. Function ledger closed: every one of the 16,257 functions of the executable and its 4 overlays has a final status (ported and verified,
   or not needed with a reason). Section 4.
2. BDX host table closed: every one of the 812 host functions that disc scripts call is `imp` (or proven unreachable). Today 104 are `imp`.
3. Every system of section 5 passed its exit gate.
4. Diff-rig playthrough: the story's critical path, recorded in the original as input recordings from save states, replays in kh2-rs with every
   checkpoint inside tolerance. Section 6.
5. `DEVIATIONS.md` has no open P0 or P1 row.

## 2. Where the project stands (measured 2026-10-22)

- 19 days old, 1,482 commits, 149,363 lines of Rust in 17 crates; `kh2-app` alone is 114,091 lines in 101 flat files. About 1,375 tests.
- The original: 12,468 functions in the main executable (2.19 MB of code) plus 3,789 in the overlays ovl0 to ovl3 (0.70 MB; Gummi flight,
  Gummi menu, title and one more).
- Code and docs of kh2-rs cite 4,019 distinct original functions; 3,965 of them are in the main executable, which is 40 percent of its code
  bytes. No overlay function is cited (0 percent). Citing is not porting, so the real ported share is lower and unknown: nobody can say what is
  left without reading the tree. This is the gap the ledger closes.
- `sheets/systems.json` has 56 rows and almost every one is "implemented", so it no longer measures anything: the rows are far too coarse.
- BDX host functions: 812 distinct ones are called by disc scripts; the app has about 392 arms (104 imp, 138 approx, 96 stub, 54 missing).
- Content: Twilight Town slice, world map; 289 of 289 areas load; 54 enemy kinds in `enemy::KINDS` of about 140 enemy objects (88 absent,
  8 of them bosses); no movie player, no event voices, no Gummi, one minigame (the Struggle), one player class (Sora; no Roxas class, lion,
  merman, Tron or outfit forms).
- Open user reports: about 40 `DEVIATIONS.md` rows say "user not yet re-checked".
- Ground-truth tooling that already works: the Ghidra project and the decomp (12,468 `.c` files), the `kh2-ee` oracle (runs original MIPS
  functions from the user's executable and compares them with the port), PCSX2 with PINE (RAM read/write, save states) and the patched
  PCSX2-MCP debug server (watchpoints, breakpoints, calling functions by hand), the user's save states, a WASAPI loopback recorder.
- Machine: i9-12900K (24 threads), 32 GB RAM, about 219 GB free on C. More than 3 cold builds at once saturate the CPU.

What slows the project down, with causes:
1. Scattering. The queue is fed from many fronts (enemy kinds, audio, menus, world map, fixes) at once; each system reaches 70 to 90 percent and
   is left. There is no rule that closes a system before the next opens, and no number that would show a system is closed.
2. Per-item work where a table would do. Enemies run the original scripts, so what blocks a kind is almost always a host function. 54 kinds
   were added one work package at a time; closing the host table would enable whole families at once.
3. Token cost. Agents read documents of up to 428 KB (`menus.md`), 244 KB (`enemies.md`), 144 KB (`events.md`, `party.md`, `summons.md`);
   the orchestrator rereads long HANDOFF sections; every merge reruns the full gate.
4. Build cost and conflicts. One 114k-line crate recompiles for any change; every enemy work package edits the same `KINDS` line, constants
   and tests and conflicts with its neighbours (HANDOFF lessons record git interleaving two test bodies and a resolver duplicating KINDS).
5. Escapes. Harness checks are log-based; what looks or sounds wrong is found only when the user plays (the audio reports of 2026-10-22, the
   combat intro camera, the command menu icons).
6. Fragile process files. `local/` is untracked: `accept.ps1` was truncated to 0 bytes on 2026-10-21 and had to be rebuilt from transcripts.

## 3. Strategy in one paragraph

Measure first, then close systems in a fixed order, and prove each against the running original by machine. Concretely: (a) a function ledger
that gives every original function a system and a status and turns "how far are we" into a number per system; (b) a diff rig that replays the
same input from the same save state in PCSX2 and kh2-rs and compares RAM values, frames and sound; (c) a ladder of systems worked strictly in
order, each closed by an exit gate, with parallel workers only inside the open system; (d) table-driven closing (host table, effect runtime,
area-script opcodes) instead of per-content work packages; (e) a cheaper process: short doc contracts, crate split, a build cache, spec-then-port
work packages, and bookkeeping done by scripts.

## 4. The function ledger

Purpose: coverage truth at the level of the original's code, which is what the Spreadsheet Method intended (`../Spreadsheet.json`: "no coverage
truth" is failure mode 3) and what `sheets/systems.json` no longer provides.

Sheet: `sheets/functions.json`, columnar like the others, one row per function: `addr`, `image` (main, ovl0..ovl3), `size`, `system`,
`status`, `evidence`, `note`. It holds addresses, sizes and statuses only: the same kind of fact the docs already cite 4,019 times. No
decompiled text, no community names (RULES A5).

Statuses, in order:
- `unread`: nobody has looked at it.
- `read`: its behaviour is described in a system doc (cited there).
- `ported`: Rust code implements it and names the address in a doc comment.
- `verified`: a test proves it: an oracle test that calls the original (`kh2-ee`), a disc test, or a diff-rig scenario that exercises it.
- `not-needed`: with a reason: `sdk` (Sony runtime and libc: replaced by Rust or the PC platform), `debug` (debug-only code the retail game never
  reaches), `hw` (PS2 hardware plumbing the port replaces: DMA, VU microcode upload, IOP RPC), `dead` (no caller, proven), `fm` (not in USA).

How it is filled:
1. Import the 16,257 rows (address, image, size, callers, callees) from `../kh2_ps2/sheet/code/functions_*.json`. Callers and callees stay in
   `../kh2_ps2` (local); the tracked sheet keeps only the columns above.
2. Seed systems automatically: a function cited in `docs/<system>.md` gets that system; cited in code under a domain crate gets that crate's
   system. Conflicts go to a review list.
3. Propagate along the call graph: an uncited function whose callers all belong to one system joins it; leaves called from everywhere become
   `core`. The 113 functions already tagged `sdk` in `../kh2_ps2` and the Sony library ranges become `not-needed: sdk` after a check.
4. `kh2 ledger scan` (new CLI command) sets `read`/`ported`/`verified` from the tree: addresses in docs, in Rust doc comments, and in test names
   or test doc comments. Humans and agents never hand-edit those statuses; they edit `system`, `not-needed` reasons and notes.
5. `kh2 ledger report` prints per system: functions and bytes per status. `kh2 ledger brief <system> [cluster]` prints the unread functions of a
   cluster with callers and existing citations: the skeleton of a work package brief.
6. Preflight gains two checks: a system marked closed in `sheets/systems.json` with any `unread`/`read` function is an error; a `ported`
   function with no code reference is an error.

The headline number becomes "verified or not-needed bytes / total bytes", per system and overall. It is the number HANDOFF reports.

## 5. The system ladder

Each step is one system (or a tight group). Exactly one step is open for porting at a time. A second lane may write specs for the next step
(docs and ledger only, no app code), so workers never idle and never leave half-built code behind. Workers inside the open step take disjoint
clusters of its functions.

Every step closes with the same exit gate:
- G1 ledger: every function of the system is `verified` or `not-needed`.
- G2 hosts: no `stub` or `missing` host arm belongs to the system (where it has host functions).
- G3 rig: the system's diff-rig scenarios pass (section 6), and `accept.ps1` is green.
- G4 reports: every `DEVIATIONS.md` row of the system is fixed and rig-checked (the user re-checks only what the rig cannot judge, batched).
- G5 doc: the system doc's contract section (section 7) is final; `sheets/systems.json` row closed; CHANGELOG line.

A step may be reopened only by a new user report or a failing rig scenario; that fix goes to the front of the queue (as today).

The order runs from the bottom of the dependency stack to the top, and finishes the systems that are already mostly built before new ones:

| step | system | contents (all of it, as the original has it) |
|---|---|---|
| L0 | foundations | Phase 0 (section 10): reorganization, ledger v1, diff rig v1. Not a game system; it gates everything after it. |
| L1 | core runtime | object and task system, frame order and timing, RNG (exact port, seeded from RAM so the rig can run in lockstep), area load and jump (all jump types), area scripts (every opcode, incl. 0x16 `load_file`), story flags and their side effects, save block and progress, COCT collision complete (ground, ceiling, raycasts gated like walls), memory-card save format (read the user's real PCSX2 saves; write saves the original can load) |
| L2 | player and camera | Sora's controller (all actions, growth abilities, Quick Run camera speed), the Roxas player class, every camera type (incl. type 4) and blocker, look-around, the head look-at |
| L3 | combat and effects | attack, damage, reactions, hit stop, clash, status effects; the PAX effect runtime complete (all effect programs on the disc run with no unknown opcode); prizes and drops; experience, levels and bonus rewards (max HP/MP growth); all abilities incl. passives |
| L4 | magic | every spell and level, Magic Lock-on, MP charge, form magic, magic on party members |
| L5 | drive, limits, summons | every form incl. Anti rules and form levels, every limit scene (camera, participants, gauge animation, host 7.28), the four summons with their limits and the summon task |
| L6 | party | friend AI complete, every world partner (Beast, Auron, Mulan, Aladdin, Jack Sparrow, Simba, Jack Skellington, Tron, Riku, Mickey and Mickey's rescue), generic reaction commands, member swap |
| L7 | script host table | all 812 host functions: enemies, NPCs, missions, area and boss scripts. Gate: the area survey soaks every enemy object of ObjectType 4/21 headless with no stub/missing host, every boss included. `enemy::KINDS` becomes a deny-list of proven-broken objects (empty at the gate). |
| L8 | UI | HUD (every element), every menu and page, shops and synthesis, journal (incl. lighting), save and load screens with header art, title screen, new game, config, tutorials, game over (scene model, save-body restore), BATTLE REPORT |
| L9 | audio | the sound driver complete (voice limit and stealing as the SPU does, mixing level), every sound placed as the original places it, voices, BGM transitions; the rig compares envelopes against recordings |
| L10 | events and movies | the event player complete, event voices and lip sync, subtitles, the PSS movie player (MPEG-2 video and ADPCM audio, decoded in Rust from the disc), credits and endings |
| L11 | world mechanics and minigames | the other player classes (lion, merman, Tron, Halloween/Christmas Town and Port Royal forms, Timeless River), Atlantica's musicals, the Light Cycle, the 100 Acre Wood games, skateboard, Agrabah's carpet, Mulan's morale, Port Royal's medallions and curse, escort and timed missions, the Olympus cups, every other mission manager kind |
| L12 | world map and Gummi | the world map complete (route markers, cursor movement, events), the Gummi flight, Gummi menu and editor (overlays ovl0 to ovl3, 3,789 functions, nothing ported today) |

Then Phase 3, content certification: the story's critical path world by world (Twilight Town, Hollow Bastion, the first visit worlds, the
second visits, The World That Never Was), each certified by rig recordings of its path, its bosses and its optional content, then one full
playthrough. Content needs no new systems by then; what a world exposes as missing goes back to its system step as a fix.

Why this order: L1 to L6 are 60 to 90 percent built (closing them first answers the "sequential" complaint and makes every later step stand on
finished ground); L7 turns content into data; L8 to L10 are wide but shallow; L11 and L12 are the largest unknowns and need L1 to L10 under them.

## 6. The diff rig (original versus port, by machine)

What it does: start the original and kh2-rs from the same state, feed both the same pad input frame by frame, record the same named values,
frames and audio, and compare. It replaces most user play-testing (the user chose to play-test rarely).

Parts, in build order:
1. State transfer. A PCSX2 save state (`.p2s`) is an archive holding the EE memory image [inferred: check on 2.8.2]. `kh2 state from-p2s`
   reads the save block (0x33e860, 0xb4e0 bytes), the current world, area and entrance, and Sora's position from it and writes a `KH2_STATE` file.
   `Progress` already keeps the party block as raw bytes, so this is mostly plumbing. Later (L1) the same from real memory-card saves.
2. Input feed. Either PCSX2's Input Recording files (pad state per frame, played from a save state) [inferred: check that 2.8.2 and the
   PCSX2-MCP build both support it], or writing the pad buffer in EE memory each frame through the debug server (the DYN3 idea). kh2-rs gets
   the same per-frame pad script (`KH2_PAD=<file>`, replacing `KH2_KEYS` timing by seconds with frames).
3. Value traces. The original: per-frame reads of a list of addresses at a fixed point of the frame (an execution breakpoint on the frame
   function with logging, or frame-advance plus PINE reads). kh2-rs: the same list by name from the app's own fields, which already carry their
   original offsets in doc comments. Values: Sora's position, facing, motion id and frame, HP/MP/drive, camera position and target, enemy
   positions and modes, RNG state, story flags, menu state.
4. Frames and sound. Screenshots at checkpoint frames in both (PCSX2 at the native internal resolution, kh2-rs at 640x448 for the rig), a
   structural similarity score with per-scenario masks; audio as RMS per 100 ms and peak per checkpoint window against the loopback recording.
5. Scenarios. `scenarios/<system>/<name>.toml` (tracked; no game data): save-state name (the file stays local), pad script, checkpoints, the
   values to compare and their tolerances. `tools/rig.ps1 <scenario>` runs both sides and writes a report into `local/rig/<run>/`
   (values table, side-by-side frames, audio plots, a pass/fail line).
6. Scenario forge. Most of the user's save states are early in the game. To reach any point without playing: from a save state, write the save
   block through PINE (story flags, world, area, inventory, party) and trigger an area load; save the result as a new state. Where the forge
   cannot reach a scene (mid-event, Gummi), list it in "Needs your attention" as a save state for the user to make, batched.
7. Lockstep where possible, statistics elsewhere. Menus, events, camera, movement and UI can match frame by frame once the RNG is ported (L1).
   Busy fights may drift; there the rig compares distributions over a long run (damage per hit, mode durations, drop rates, sound density).

Rig v1 (Phase 0) is parts 1, 2, 3 and 5 for movement and menus in Twilight Town. Parts 4, 6 and 7 grow with L1 to L3. From L1 on, every exit
gate includes the rig.

## 7. Cheaper, faster process

Documents:
- Each system doc starts with a contract of at most about 150 lines: what the system does, its state (fields with owners), its entry points,
  the open gaps, and links into the detail. Agents read the contract plus the ledger rows of their cluster, not the whole doc.
- The big docs are split by subject (`menus.md` per menu, `enemies.md` per family, `events.md`, `party.md`, `summons.md` by topic) under
  `docs/systems/<system>/`. `CHANGELOG.md` is split per shift under `docs/changelog/`.
- `docs/README.md` maps each system to its doc, ledger system name, crate and scenarios.

Builds:
- `kh2-app` split into domain crates (section 10, R8), so a change rebuilds one crate and parallel work packages touch different crates.
- `sccache` shared by all worktrees (cold builds of a new worktree come from the cache); `cargo nextest` for the test runs.
- Workers run the tests of their crate and the rig scenarios of their system; the full `verify.ps1` runs once per merge, by the orchestrator.

Work packages:
- Spec then port. A spec package (reverse engineering) writes the contract section, the ledger rows (`read`) and oracle test vectors; a port
  package implements from that spec and lands `ported`/`verified`. Spec packages need the strongest model; port packages from a good spec do not.
- Briefs are generated: `kh2 ledger brief` produces the function list, callers, citations and ownership globs; the orchestrator adds the goal
  and the gate. A brief stays under about 4 KB.
- Bookkeeping by script: the worker's report carries a small machine-readable block (tests added, ledger delta, docs touched, scenarios);
  `tools/bookkeep.ps1` appends the CHANGELOG line, updates the sheets row and the verify count. The orchestrator reads verdicts, not outputs.
- Enemy kinds stop being hand-added entries: once the crate split moves enemies into their own crate, a kind is a file of its own that a
  generated registry collects (no shared `KINDS` line to conflict on), and from L7 on every enemy object runs unless it is on the deny-list.

Orchestrator context:
- HANDOFF is rewritten, not appended: one screen of state (commit, tests, ledger numbers, open step, packages in flight, needs your
  attention). History goes to `local/archive/`.
- Reports from workers are at most about 30 lines; anything longer goes to a file the orchestrator reads only if a check fails.

## 8. Agent and model policy (proposed; the user confirms once)

| role | model | why |
|---|---|---|
| planner at session start (RULES B3) | Opus 5.5, medium or high effort | as today |
| orchestrator | Sonnet 5.5 | judgement is mostly table-driven (section 4 of ORCHESTRATOR.md); keeps the long session cheap |
| spec packages (reverse engineering, contracts, oracle vectors) | Opus 5.5 | the bottleneck is reading hard code; today Sonnet fails these and retries on Opus anyway |
| port packages from a spec, fixes | Sonnet 5.5; one retry on Opus (B2) | as today |
| reviewers | Sonnet 5.5 | as today (B8), now with the rig report as evidence |
| mechanical packages (ledger propagation checks, doc splitting, survey and accept sweeps, rig runs) | Haiku 5.5 | no judgement needed; cheapest |
| never | Fable | RULES A6 |

Concurrency: one open ladder step with up to 3 port workers on disjoint clusters, plus one spec worker for the next step, plus reviewers. With
sccache and the crate split this may go to 4 or 5 port workers; measure cold and warm build times after R8 before raising it.

## 9. RULES changes this plan proposes (need the user's word; only the user changes RULES)

CONFIRMED by the user 2026-10-22: C1 to C6 below and the model table of section 8 ("I confirm RULES changes C1-C6 and the model table in
local/PLAN.md. Run Phase 0 once shift 19 has wrapped up."). They enter RULES.md at Phase 0 step R6.

- C1 (sequential): exactly one ladder step is open for code at a time; a second lane writes specs for the next step only. A step closes only
  through its exit gate (section 5). New user requests and P0 reports still go first.
- C2 (ledger): the ledger is the measure of progress; HANDOFF reports it; preflight enforces the closed-system checks.
- C3 (rig): from L1 on, every exit gate includes rig scenarios; user play-tests are batched at gates and only for what the rig cannot judge.
- C4 (models): the table of section 8 replaces B2's "workers start on Sonnet" for spec packages and adds Haiku for mechanical packages.
- C5 (tracked tooling): `.gitignore` is widened for `tools/`, `agents/` and `scenarios/` (the user chose "track local/ tooling"); still no
  game data, no extracted files, no decompiled text, no save states, no recordings in them. This amends A5's "do not widen it".
- C6 (GitHub): `main` follows `integration` and both are pushed at every wrap-up (the user, 2026-10-22).
- C7 (layout-faithful state, confirmed with "go ahead" 2026-10-22): the port's main state mirrors the original's byte layouts so the oracle can
  verify stateful functions; a green oracle merges without a reviewer (amends B8).

## 10. Phase 0: the reorganization runbook

Start condition: the live shift (19) has wrapped up: no agents, no game processes, no worktrees except the main checkout, everything merged,
`main` = `integration`, pushed. Then integration is frozen for code until R8 is done. Tag `pre-reorg` first.

- R1 Inventory and backup. Copy `local/` (without `accept/`, `archive/`, outputs) to `local/archive/pre-reorg-<date>/`. Record what is where.
- R2 Track tooling. New tracked folders:
  - `tools/`: verify, accept, review, merge, trial-merge, snapshot, capped, state (`.ps1`), `mips_scan.py`, `pine.py`, `dbg.py`, `wavstats.py`,
    `vidframes.py`, `pcsx2_shot.py`; `looprec` becomes a tracked crate or tool folder. Machine paths (ISO, decomp, PCSX2, videos) move into one
    untracked `local/machine.ps1` / environment variables that the scripts read.
  - `agents/`: RULES, ORCHESTRATOR, WORKER, the brief templates, DEVIATIONS (no game data in any of them). HANDOFF, PROGRESS, live briefs,
    reports, save states, recordings and accept output stay in `local/`.
  - `.gitignore`: add `!/tools/`, `!/agents/`, `!/scenarios/` (C5). Run the hygiene greps of ORCHESTRATOR section 6 before the commit.
- R3 Retire stale files. The superseded stubs `local/ORCHESTRATION.md`, `PLAYBOOK.md`, `PCSX2.md`, `VIDEOS.md`, the old `PROGRESS.md` body,
  stale patches (`main-dirty-enemy.patch`, `stray-en15c.patch`) and root leftovers (`build-py5.log`, `soak.out`) move to `local/archive/`.
  `target-trial/` (build output) is listed in "Needs your attention" with its size (A11: older material is the user's to delete).
- R4 Docs reorganization. `docs/README.md` index; `docs/systems/<system>/` with a contract file per system and the split detail files;
  `docs/changelog/shift-NN.md`; `docs/METHOD.md` updated for the ledger and the rig. Every path reference in code comments, tests, briefs,
  WORKER and ORCHESTRATOR is rewritten in the same commit (grep for `docs/`). Done by a mechanical package plus one reviewer.
- R5 Ledger v1 (section 4): import, auto-seed, propagation, `kh2 ledger scan/report/brief`, preflight checks. First report goes to HANDOFF.
  Spec package (Opus) for the propagation rules and the system list; port package for the CLI.
- R6 This plan moves to `docs/ROADMAP.md`; RULES gets C1 to C6 if the user confirmed them; ORCHESTRATOR and WORKER are rewritten for the
  ladder, the ledger, generated briefs, report blocks and the model table (shorter than today).
- R7 Build speed: sccache configured for all worktrees, `cargo nextest`, and a measurement of cold and warm build and test times before and
  after (written to HANDOFF).
- R8 Crate split of `kh2-app`, in three sequential packages on a frozen integration:
  1. Graph: measure the module dependency graph (`use crate::` edges) and propose the cut; output a doc and a list of moves. Opus.
  2. Core: extract the shared layer (data and assets access, area, world and collision, schedule, harness, progress, skeleton and motor,
     camera core) into `kh2-game-core`.
  3. Domains: `kh2-enemy` (enemy, host, AI, objects), `kh2-party` (party, friend AI, limits, summons, reactions, Mickey), `kh2-player` (Sora,
     drive, magic, abilities, combat, clash, prizes), `kh2-ui` (HUD and menus; perhaps two crates), `kh2-event` (event player, intro, NPC,
     Struggle, jump), `kh2-worldmap`, `kh2-sound` (app-side audio); `kh2-app` keeps only plugin wiring and the binary. Moves are `git mv` so
     history follows; no behaviour change; the test count must match before and after; accept sweep and a look at idle-hud and a menu shot.
  The proposed cut is a starting point: R8.1 decides it from the measured graph.
- R9 Enemy registry: one file per kind and a generated registry (section 7), replacing the shared `KINDS` array.
- R10 Diff rig v1 (section 6, parts 1, 2, 3, 5): state transfer, input feed, value traces, scenario files, Twilight Town movement and two menu
  scenarios passing. Spec (Opus) for the PCSX2 side first: Input Recording support, frame-exact read point, `.p2s` layout.
- R11 Re-baseline: full accept sweep, ledger report, rig report, rewritten HANDOFF; wrap-up and push.

Speed levers (added 2026-10-22, the user: "go ahead"):
- R12 Port factory (lever 1). `kh2 ledger brief` plus a generated oracle test per function: kh2-ee runs the original on fuzzed inputs, the
  port must match; an agent loops until green. A green oracle merges without a reviewer (reviewers only on failures and on code the oracle
  cannot cover). Leaf and pure functions on Haiku/Sonnet in batches of a cluster per package. Spec (Opus) first: input generation, stubbing
  rules, how state is laid out.
- R13 Layout-faithful state (lever 2, rule C7 below). The main structs (objects, status block, motion state, save and party blocks) mirror the
  original's byte layout (`#[repr(C)]` or raw bytes with typed accessors, as `Progress` does), so stateful functions are checked by copying
  state into kh2-ee, running both and comparing bytes. Spec (Opus) lists the structs and the migration order; ports follow per system.
- R14 Readable decomp (lever 4). In `../kh2_ps2` only: match functions against the Final Mix decomp (govanify/kh2, about 6,500 symbols and the
  class layout) to carry names and types to the USA functions; define the main structs in the Ghidra project; re-decompile everything with
  them. Names and types stay out of the repo (A5); the ledger keeps addresses.
- R15 Turbo harness (lever 5). A fixed-tick run as fast as the CPU allows with offscreen rendering (`KH2_TURBO=1`), screenshots by tick
  instead of seconds, accept scenarios in parallel; PCSX2 uncapped for rig recordings.
- R16 Build flags: `rust-lld`, line-tables-only debug info, and the Cranelift backend for dev builds if it works on Windows MSVC; measured
  with R7.
- R17 Hybrid runtime, scoping only (lever 3): one spec package (Opus) on running unported functions in kh2-ee or a build-time recompiler
  from the user's disc (never committed) next to the Rust port, what it needs (R13 layouts, GS/SPU fallback) and its cost. The user decides
  after reading it. DECIDED 2026-10-22 (the user left it to me): option c+ of local/specs/HYB.md, a headless reference runtime (kh2-ee boots
  the user's executable and runs the whole original in-process, no picture or sound) as the reference side of the rig and the factory's source
  of real state; 9 to 13 shifts; HYB-1 and HYB-2 start in Phase 0 beside R10. The full hybrid stays possible later.

Phase 0 is about 3 to 5 shifts of work (R12 to R16 add about 2 more, partly in parallel with R8 to R10); R2 to R4 can run side by side (disjoint files), R5 and R7 side by side, R8 and R9 alone, R10 alongside R8.

## 11. The first packages after Phase 0 (L1, core runtime)

In order; the spec lane starts L2 specs as soon as L1's specs are done.
1. L1-S1 (spec, Opus): ledger classification of the core runtime (task system, frame order, RNG, area load and jump, area-script interpreter,
   story flags, save block), contracts for each, and the list of unread clusters.
2. L1-P1: the RNG, ported exactly, seeded from the save state; oracle test; rig scenario that compares RNG state over 600 frames.
3. L1-P2: area-script opcodes to 100 percent (0x16 `load_file` and anything else the ledger shows unread), with the survey as the check.
4. L1-P3: collision completion (ground, ceiling, raycast and `blocked_m` gated by the group mask; quick checks against the rig).
5. L1-P4: memory-card saves: read the user's PCSX2 memory-card saves into `Progress`; write saves in the original's format.
6. L1-P5: frame order and timing against the rig: object update order, the 60 Hz logic tick, area load timing.
7. L1 gate.

## 12. How progress is reported

HANDOFF's first screen and every report the user asks for show:
- ledger: verified-or-not-needed bytes over total, overall and for the open step; functions per status;
- host table: imp / approx / stub / missing of 812;
- rig: scenarios passing over total, for the open step and overall;
- `DEVIATIONS.md`: open P0 / P1 / P2;
- the open ladder step, what is left in it, and the packages in flight.

A rough horizon from today's pace (about 2 percent of main-executable code bytes newly cited per day, with no verification depth): the remaining
code is about 70 percent of all bytes, so the systems ladder is months of shifts, not weeks. Phase 0 should raise the pace (generated
briefs, table-driven closing, cheaper builds, fewer failed Sonnet attempts on hard reverse engineering) and the ledger will show by how much
within the first two ladder steps. Re-estimate at each gate from the measured bytes per shift.

## 13. Risks and how the plan handles them

| risk | handling |
|---|---|
| the rig cannot run frame-exact in PCSX2 | fall back to checkpoint comparisons (state at events, screenshots at stable frames) and statistics; the R10 spec decides early |
| save states for late content are missing | scenario forge first; a batched list for the user only where the forge fails |
| crate split stalls the queue | it runs on a frozen integration in three short packages; nothing else is merged meanwhile |
| ledger propagation mislabels systems | conflicts go to a review list; `system` stays hand-editable; gates check bytes, not labels alone |
| overlays have no decompilation | load them in Ghidra at 0x1F18000 (the `kh2_ps2` pipeline has `LoadOverlays.java`) and decompile per overlay before L12's specs; overlay rows exist in the ledger from R5 |
| movies need an MPEG-2 decoder | a Rust decoder (an existing crate if one fits the licence, else a port of the standard); spec at L10 |
| the user's reports contradict the decomp | the report wins (A2); the rig shows which build or edition it matches (the 2026-10-06 note about Final Mix behaviour) |
| process files lost again | tracked in `tools/` and `agents/` (R2) |

## 14. Needs the user (collected; nothing here blocks Phase 0 except item 1)

1. Confirm the RULES changes C1 to C6 (section 9) and the model table (section 8).
2. `target-trial/` in the repo root (build output) and `E:\kh2rs-target-F1r` if it ever reappears: yours to delete.
3. Later, batched: save states the scenario forge cannot make (listed per ladder step).
