# kh2-rs ORCHESTRATOR guide (how to run a shift)

RULES (`agents/RULES.md`) win over this file. State, queue and "Needs your attention" live in `local/HANDOFF.md`; workers use
`agents/WORKER.md`. You spawn, review, merge, accept and keep the books; you do not implement WPs. Allowed yourself: one-line compile/type
fixes in a merge commit, docs-only corrections a worker asked for, `local/` files, accept lines. Only you write PROGRESS, HANDOFF,
DEVIATIONS, README, sheets. Keep context lean: read verdicts and report blocks, not outputs. No chat messages (B1). Scripts below are
`tools\<name>.ps1` of the main checkout, run with `powershell -NoProfile -ExecutionPolicy Bypass -File`.

## 1. Session start
0. Plan agent (B3), then from the main checkout: `git status --short; git log --oneline -3; git worktree list`, `tools\verify.ps1`
   (baseline from `local\gate.json`), `cargo build -p kh2-app --bin kh2-game`, `tools\state.ps1 -NoVerify`.
1. Clean the leftovers HANDOFF lists (worktrees, `.claude\worktrees\*`, stray target dirs).
2. Move `local\PROGRESS.md`'s body to `local\archive\progress-<date>.md`; write the `start` line (hash, tests, clippy, usage, free disk).
3. `tools\accept.ps1 -SkipAudio` if the last sweep is older than the last merges; LOOK at the PNGs.

## 2. The loop (never idle while usage allows)
1. Work only inside the open ladder step (C1, PLAN section 5) plus one spec lane for the next step; user requests and P0 reports first.
   The step closes only through its exit gate G1-G5. Progress is the ledger (C2): `kh2 ledger report`
   gives verified-or-not-needed bytes per system; HANDOFF's first screen carries it, the host table count, rig passes and open DEVIATIONS.
2. Before each spawn: usage (`get_usage`) and free disk on C; spawn count B5; at 90% of the 5-hour window go to section 5.
3. Next WP: from HANDOFF's queue, dependencies merged, files disjoint from running WPs. Briefs are generated: `kh2 ledger brief <system> [cluster]` gives the function list, callers, citations and ownership globs; you add goal, gate and
   model (B2) from `agents/briefs/brief-template.md`. A brief stays under about 4 KB. A missing spec becomes a spec WP (Opus).
4. Spawn (O1); on a report: review (O2), merge (O3), bookkeeping (O5), cleanup (O6); log each step in PROGRESS.
5. Every second merge: `tools\accept.ps1 -SkipAudio -Only idle-hud,combat-ground,combat-air,tt04-dusk,magic-fire,reaction-34-forced` plus
   the WP's own scenarios and rig scenarios of its system (C3; TODO(remove when RIG-5 lands) `tools\rig.ps1`; turbo parallel accept
   `-Turbo -Jobs 8` is planned, R15).
6. HANDOFF every 3 hours and at every stopping point (A15): rewritten, one screen of state; history to `local\archive\`.

## 3. Recipes
- O1 spawn: `git worktree add .claude\worktrees\wp-<id> -b wp/<id> integration`. Agent: `model` per B2 (said in the brief header),
  `subagent_type: general-purpose`, no isolation, background. Prompt: `Read <repo>\agents\briefs\_common.md, then <repo>\local\briefs\<brief>.md
  and do work package <id>. Your worktree is <repo>\.claude\worktrees\wp-<id> on branch wp/<id>. Integration has <N> tests, clippy <M>.
  You are <model>. Report once as _common.md says.` Log `spawn` with the model.
- O2 report: ignore duplicates. `tools\review.ps1 -Branch wp/<id> -Own '<brief globs>' -Tests '<claimed tests>'`; READ the `REVIEW: PASS`
  line, never chain the merge after it with `;`. Code WPs: reviewer (Sonnet, no isolation, prompt in `agents/briefs/reviewer.md`) unless
  the package passes the factory merge rule (WORKER 2.9, C7; `kh2 ledger merge-check`). Docs-only WPs
  skip it (B8). Spot-check one claim yourself and LOOK at one cited screenshot. MERGE or MERGE WITH FOLLOW-UP -> O3 (PARTIAL items become
  brief `<id>b`, front of the queue); DO NOT MERGE -> SendMessage the failing items; after the fix a fresh reviewer reviews again.
- O3 merge: `tools\merge.ps1 -Branch wp/<id> -ExpectTests <old + new, at most the worker's count> -Message 'Merge wp/<id>: <line>'`
  (merge --no-ff with the trailer, verify, revert on FAIL, removes the worktree and branch on PASS; a PASS updates `local\gate.json`).
  Then apply the report's docs/one-line outside-ownership edits (separate commit), add proposed accept lines to `tools\accept.ps1`, run
  the WP's scenarios.
- O4 conflicts, O5 bookkeeping, O6 cleanup, O7 moving main and pushing (A8, C6): `agents/RECIPES.md` R7 and R8, after every merge.

## 4. Decision table
| situation | answer |
|---|---|
| review.ps1: file outside ownership | explained (own signature compile fix, brief exception) -> accept; else SendMessage: revert it |
| review.ps1: claimed test missing | SendMessage: "test <name> is not on the branch; add it or correct the report" |
| review.ps1: `sheets/systems.json` changed | merge, then `git checkout HEAD~1 -- sheets/systems.json` in a follow-up commit |
| trial merge: text conflict / compile error | resolve (O4) / one-liner in the merge commit, else the later worker merges integration and fixes |
| verify FAIL after a merge | merge.ps1 reverted it; SendMessage the worker the failure |
| clippy +1..2 | trivial -> fix in a separate commit; else send back |
| accept or rig FAIL after a merge | rerun once; from the WP -> revert, send back; an old pattern -> fix the pattern, never delete a scenario or loosen a tolerance |
| worker fails (review not PASS, verify fails, stuck) | retry once on `opus` from its branch; fails too -> Needs your attention, next WP (B2) |
| worker hit its context limit | respawn on the same model: brief + "continue on branch wp/<id> from commit <hash>; read git log first" |
| worker died (rate limit, error) | SendMessage to resume; else `tools\snapshot.ps1`, respawn as above |
| worker silent 2 hours | `git -C <wt> log --oneline -3`: recent commits -> wait; none -> SendMessage "status?" |
| worker asks a question | answer from RULES, brief, docs; a user decision -> conservative default (feature out), "decided:" in HANDOFF (A20) |
| worker needs an outside-ownership edit | docs/one-liner -> apply after the merge; else a step of the next WP owning the file |
| user reports a difference or asks for something new | DEVIATIONS row; P0 or request -> brief from `agents/briefs/FIX-template.md`, front of the queue |
| the feature does not exist in the original | valid result: merge the docs, close the item everywhere |
| the app does something the original does not | DEVIATIONS P1 "app invention"; keep unless the user says remove |
| permission prompt blocks a step | skip it, note it in HANDOFF, next WP |
| disk on C below the spawn threshold | no new spawn; clean merged worktrees and leftovers first |

## 5. Wrap-up (90% of the 5-hour window, a stopping point, or the user says `WRAP UP`)
1. Stop dispatching; SendMessage running workers: "Commit what compiles and stop now; report done / not done / last commit." Merge what passes.
2. Full `tools\accept.ps1 -Out local\accept\<shift>-final`; LOOK at idle-hud, combat-ground, tt04-dusk, magic-fire and every new scenario.
3. Hygiene (empty except the known `docs/systems/player-camera/camera.md` hits, if any): `git status --short`;
   `git ls-files | Select-String '\.(png|wav|bin|iso|img|idx|elf|vsb|seb|wd|tm2|mdlx|apdx|pax|mset)$'`;
   `git grep -nE '\b(iVar|uVar|lVar|puVar|piVar|pcVar|bVar|param_)[0-9]+\b' -- .`; `git grep -nE '[A-Z]:\\Users' -- tools agents scenarios`.
4. `cargo run -p kh2 -- preflight`; `cargo run -p kh2 -- validate; git checkout sheets/systems.json`.
5. Bookkeeping (O5) for every merged WP, `docs/changelog/shift-NN.md` for the shift (and its line in `docs/CHANGELOG.md`); cleanup (O6), leftovers check; main and push (O7).
6. Rewrite HANDOFF.md so a new orchestrator can take over at once (A15). No chat message.

## 6. Pitfalls
Read `agents/RECIPES.md` R9 (learned the hard way) once per session.
