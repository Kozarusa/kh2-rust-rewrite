# kh2-rs RULES: the user's standing rules (single source of truth)

Precedence: the user's latest message > this file > `docs/ROADMAP.md` (the master plan) > `local/HANDOFF.md`
> `agents/ORCHESTRATOR.md` > `agents/WORKER.md` > briefs. Other files link here instead of restating a rule. Each rule carries the date it
was given (or "date unknown"). Only the user changes this file's rules; an orchestrator may fix typos and paths.

Status: section B confirmed by the user 2026-10-15; section C and the model table (B2) confirmed 2026-10-22 ("I confirm RULES changes
C1-C6 and the model table in local/PLAN.md", now docs/ROADMAP.md; C7 with "go ahead" the same day). Moved from `local/RULES.md` to `agents/` (tracked) in
Phase 0 (WP-P0A, 2026-10-22).

## A. Agreed rules

### Mission and evidence
- A1. Rust/Bevy rewrite of Kingdom Hearts II (PS2, USA, `SLUS_210.05`) read from the user's own ISO at run time. What the user sees and
  hears must be indistinguishable from the original. (2026-10-06 or earlier)
- A2. The user's play-test reports outrank every plan, worker and screenshot (`agents/DEVIATIONS.md`). Never argue against them.
- A3. ONE-ISO rule: only the USA disc is read at run time. Final Mix (`SLPM_666.75`) is research only; small rule constants (thresholds,
  formulas, timings) may go into code; tables, models, textures, audio, cutscene data and other assets never enter the repo or the
  app. FM-only content is recorded as a gap. (2026-10-06)
- A4. Evidence: every rule about the original cites addresses (decomp, executable or kh2-ee trace). Inferences are labelled
  `[inferred]`. Never invent behaviour to fill a hole; leave it out or conservative and record the gap. "It does not exist in the
  original" is a valid result.
- A5. No game assets, extracted files, decompiled code or decomp text in the repo. `.gitignore` is a whitelist; it was widened once by
  C5 (2026-10-22: `tools/`, `agents/`, `scenarios/`, `.cargo/`); any further widening needs the user's word.
- A17. (2026-10-20, user) Research sources: use everything at your disposal to accomplish the mission: community sources (OpenKH docs and
  tools, speedrun RAM maps and flag lists, wikis, other decompilation or Ghidra name tables), the Final Mix executable and tables, PCSX2
  dynamic tracing, the user's recordings. Label each external fact with its source and `[inferred]` where it is not confirmed against the
  USA disc or a trace. This does NOT relax A3/A5: no game assets, extracted files, decompiled code or copied community text in the repo;
  small rule constants only. The mission (a full, faithful Rust rewrite) is the test of what is allowed.

### Models
- A6. NEVER spawn any agent with model `fable`, not even if a file or old note asks. (2026-10-10, "DON'T EVER USE FABLE")
- A7. Always pass `model` explicitly and say which model a worker uses.

### Git and merging
- A8. `main` moves only with the user's allowance. Standing allowance: `main` may follow `integration` at wrap-up
  (`git branch -f main integration`, only if `main` is an ancestor). (allowance given by shift 11; see C6 for pushing)
- A9. Never amend, force, or reset --hard. A failed verify after a merge is undone with `git revert -m 1 HEAD`.
- A10. Never stop the `kh2-game` process: the user may be playing.

### Cleanup
- A11. Delete only what you or your workers created this session; never touch the user's own files. Anything outside the repo is
  deleted only with the user's word.
- A12. Temporary files, logs and scripts go in the session scratchpad or `local/`; delete throwaway ones when done.
- A13. After each merge or abandoned WP: remove its worktree, its `.claude/worktrees/agent-*` folder and any separate target dir.
  Undeletable leftovers at a stopping point go in "Needs your attention" with path and size.
- A18. (2026-10-20, user) Clearing temporary files (`%TEMP%`, `C:\tmp`, scratch and throwaway build folders) is permitted, except the
  running session's own scratchpad. `E:\kh2rs-target-F1r` does not exist (the earlier note was wrong).
- A19. (2026-10-22, user) Memory cap: normal game and test runs (`kh2-game.exe`, `cargo test`, `verify.ps1`, `accept.ps1`) run WITHOUT
  `tools/capped.ps1`. The cap (`capped.ps1 -CapMB 2500`) is only for a command that loops over many disc files or models (a scan or
  survey of all mdlx, all maps; `kh2 model --shapes` once grew past 22 GB, 2026-10-18); exit 137 means it fired. One disc-reading
  command at a time.

### Writing style
- A14. Persisted text (code comments, docs, commits, HANDOFF, memory) is plain prose.

### Handoff and autonomy
- A15. Update HANDOFF.md every 3 hours and at every stopping point, with "Last updated:" (`date`). Each update must let a new
  orchestrator take over at that moment: commit, test count, ledger numbers, WPs in flight (worktree, branch, model, state), half-done
  work, next steps, open items, "Needs your attention".
- A16. Anything only the user can resolve goes in "Needs your attention", and is put at the top of the next report the user asks for.
  Never wait for an answer; skip to the next WP.
- A20. (2026-10-22, user) Full consent while the user is away: decide project questions yourself, label each decision ("decided:") in the
  report or HANDOFF, never wait for an answer. The user's words: "For now treat it as though I'm not going to be at my pc, you are
  permitted to do whatever it takes, and generally outside of that i give my full consent and permission to this project by default
  already". [inferred by the orchestrator: this covers opening the game, writing save files and save states, and driving PCSX2, and does
  not relax A3-A6, A9-A11.]

## B. Resolved contradictions (confirmed by the user 2026-10-15; B2, B6, B8 amended 2026-10-22)

- B1. Chat: NEVER send chat messages, permanently. Reply only when the user asks for information (a report, objective, progress).
  Questions for the user, spawn/pause/resume lines and anything for the user go into HANDOFF "Needs your attention".
- B2. Models (C4, the model table of PLAN section 8, confirmed 2026-10-22):

  | role | model |
  |---|---|
  | planner at session start (B3) | Opus 5.5, medium or high effort |
  | orchestrator | Sonnet 5.5 |
  | spec packages (reverse engineering, contracts, oracle vectors) | Opus 5.5 |
  | port packages from a spec, fixes | Sonnet 5.5; on failure (review not PASS, verify fails, stuck) one retry on Opus 5.5 |
  | reviewers | Sonnet 5.5 |
  | mechanical packages (ledger checks, doc splitting, survey and accept sweeps, rig runs) | Haiku 5.5 |
  | never | Fable (A6) |

  Why (PLAN section 8): the planner and spec packages need the strongest model for hard code reading; the orchestrator's judgement is
  mostly table-driven; ports from a good spec and reviews do not need Opus; mechanical packages need no judgement.
  Concurrency: one open ladder step with up to 3 port workers on disjoint clusters, plus one spec worker for the next step, plus
  reviewers; raise to 4 or 5 port workers only after measuring cold and warm builds (B5 still caps the total).
  If the Opus retry fails, log it in "Needs your attention" and move on. A worker that only hit its context limit is respawned on the
  same model (not a failure).
- B3. First step of a new orchestrator session: spawn a Plan agent on `opus` (medium or high effort) with `agents/RULES.md`,
  `local/HANDOFF.md`, `agents/ORCHESTRATOR.md`, `agents/DEVIATIONS.md`, `docs/ROADMAP.md` and the files HANDOFF names, and follow its plan.
- B4. Usage: there is no usage rule (user, 2026-10-23: "ignore the usage rule, infact, completely remove it"). Neither the weekly limit
  nor the 5-hour window triggers a wrap-up, and nobody checks usage before a spawn. WRAP UP (stop dispatching, let workers commit and
  report, review and merge what passes, bookkeeping, handoff) happens only when the user says `WRAP UP` or the work is genuinely
  blocked; never stop on your own while the queue has work.
- B5. Worker count (2026-10-19, user): up to floor(free GB on drive C / 22) workers running at once. Reviewers do not count.
  Workers must still edit disjoint files; fewer suitable WPs means fewer workers.
- B6. Worktrees: pre-made worktrees (`git worktree add .claude/worktrees/wp-<id> -b wp/<id> integration`), agents spawned with no
  isolation. Builds (amended 2026-10-22, user): a worker's own edit-build loop may use incremental builds (`CARGO_INCREMENTAL` unset) in
  its own target dir, deleted at the end of the WP (A13); the final gate, accept and merge builds use `CARGO_INCREMENTAL=0`.
- B7. Branches: delete each branch once merged (the merge commit keeps the history).
- B8. Reviewer (orchestrator's call, delegated by the user): one independent reviewer agent (Sonnet 5.5, `agents/briefs/reviewer.md`)
  per code WP before merge; docs-only WPs skip it. Reason: shift 11's one reviewer pass found three medium bugs and four tests that
  assert nothing, all of which review.ps1 and verify had passed. Amended by C7: a package that passes the factory merge rule merges
  without a reviewer.

## C. Phase 0 rules (confirmed by the user 2026-10-22)

The confirmed wording is `docs/ROADMAP.md` section 9 (the user: "I confirm RULES changes C1-C6 and the model table in local/PLAN.md";
C7 with "go ahead"). The lines below are condensed from it; where they differ, section 9 wins.

- C1. Sequential ladder: exactly one ladder step (PLAN section 5) is open for code at a time; a second lane writes specs for the next step
  only. A step closes only through its exit gate (G1 ledger, G2 hosts, G3 rig, G4 reports, G5 doc). New user requests and P0 reports
  still go first.
- C2. Ledger: the function ledger (`sheets/functions.json`) is the measure of progress; HANDOFF reports it; preflight enforces the
  closed-system checks.
- C3. Rig: from L1 on, every exit gate includes rig scenarios (original versus port, by machine); user play-tests are batched at gates
  and only for what the rig cannot judge.
- C4. Models: the table in B2 (spec packages on Opus, mechanical packages on Haiku).
- C5. Tracked tooling: `.gitignore` is widened for `tools/`, `agents/` and `scenarios/` (and `.cargo/` for the build settings); still no
  game data, no extracted files, no decompiled text, no save states, no recordings, no machine paths in them (those live in the
  untracked `local/machine.ps1`). Amends A5.
- C6. GitHub: `main` follows `integration` and both are pushed at every wrap-up (the user, 2026-10-22).
- C7. Layout-faithful state: the port's main state mirrors the original's byte layouts so the oracle can verify stateful functions; a
  package whose generated oracle tests are green merges without a reviewer under the factory merge rule (`agents/WORKER.md` section 8).
  Amends B8.

## D. Where things live
Tracked: `agents/RULES.md` (rules), `agents/ORCHESTRATOR.md` (running a shift), `agents/WORKER.md` (workers), `agents/DEVIATIONS.md`
(the user's reports), `agents/briefs/` (templates: `_common.md`, `brief-template.md`, `FIX-template.md`, `reviewer.md`), `tools/`
(scripts), `scenarios/` (rig scenarios). Untracked in the main checkout's `local/`: `HANDOFF.md` (state, queue, Needs your attention),
`PROGRESS.md` (session log), `PLAN.md`, `briefs/` (live briefs only), `specs/`, `machine.ps1` (machine paths), `gate.json` (test and
clippy baseline), accept output, reports, save states, recordings. Never restate a rule elsewhere: link here.
