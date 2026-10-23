# FIX-template: copy to `local/briefs/FIX-<n>-<short-name>.md` for every P0 the user reports (or any new request), fill the <...>

# WP-FIX-<n>: <what the user saw, in their words> -- model: sonnet (one opus retry on failure, RULES B2)

| field | value |
|---|---|
| branch | `wp/fix-<n>` in worktree `.claude\worktrees\wp-fix-<n>` |
| depends on | nothing (fixes jump the queue) |
| parallel worker | <the other running WP and its files> |
| gate | `tools\verify.ps1 -ExpectTests <N> -MaxClippy <M>` |
| recipes | `agents/WORKER.md` sections 2, 5; `agents/RECIPES.md` R1, R4 |
| time | about 2 hours (report "cause found, fix needs <X>" if not done by then) |

## The report (verbatim from the user; `agents/DEVIATIONS.md` row <date>)
<paste the user's words>

## Why
<one line: the user saw a difference from the original; the user's report outranks the plan>

## Goal (done when)
- [ ] the user's scenario reproduced BEFORE the fix with a harness command (screenshot LOOKED at, or log lines quoted)
- [ ] the original's rule found and written in <doc> with addresses (or: the gap recorded with the addresses searched)
- [ ] a unit test of the rule: <test name>
- [ ] the same harness command AFTER the fix shows the original's behaviour
- [ ] an accept scenario line (or a rig scenario) proposed that would catch a regression
The app does what the original does in this situation, proven from the original's code (addresses). If the user is right that the app
differs, the user is right: never argue from a screenshot or a guess against their report. If the original's rule cannot be found, make
the app conservative (leave the wrong thing out) and record the gap.

## Ownership
<the files where the behaviour lives; find them with: grep the log text or the feature name in crates/kh2-app/src; list them here>

## Steps
1. Reproduce: a harness command that shows the difference (screenshot or log). Commit nothing yet.
2. Find the original's rule (RECIPES R1); write it in the relevant doc with addresses.
3. Fix; test (unit test of the rule); reproduce again: the difference is gone.
4. Propose an accept scenario line that would catch a regression.

## For the orchestrator
After merging: mark the DEVIATIONS.md row "fixed <commit>, user not yet re-checked"; show the user the after picture at the next checkpoint.
