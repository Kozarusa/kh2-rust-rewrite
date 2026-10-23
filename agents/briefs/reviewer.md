# Reviewer brief (you did not do the work; change nothing)

Spawn prompt (orchestrator): `Read <repo>\agents\RULES.md, then <repo>\agents\briefs\reviewer.md and review WP-<id>: brief
<repo>\local\briefs\<brief>.md, branch wp/<id>, worktree <path>. The worker's report follows: <report>` -- model Sonnet 5.5 (RULES B2),
no isolation.

Input: the brief, the worker's report, branch `wp/<id>` and its worktree. Read-only: no edits, commits or branch switches.
1. `git -C <wt> status --short` (empty); `git -C <wt> log --oneline integration..HEAD`; `git -C <wt> diff --stat integration...HEAD`
   (list files outside ownership and whether the report explains them).
2. For EACH Goal item, produce the evidence yourself: run the claimed tests (`. .\tools\env.ps1; Use-Machine KH2_ISO; cargo test -p <crate>
   [--test <file>] <name> -- --nocapture` in the worktree); spot-check one branch or constant per ported rule against
   `$env:KH2_DECOMP\decomp_typed\main\<addr>.c`; rerun claimed harness commands (`KH2_NO_SOUND=1 KH2_NO_MUSIC=1`, one run at a time, never
   touch the keyboard) and search for the line; open the claimed PNGs; check that claimed doc sections cite addresses; flag tests that
   assert nothing; rerun claimed rig scenarios when the rig exists.
3. Hygiene: no assets, no `iVar<n>`/`param_<n>` names or FM names in added lines, no machine paths in tracked files, `sheets/systems.json`
   untouched, CREDITS.md CRLF.

```
REVIEW of WP-<id> (branch wp/<id>, head <hash>)
Files outside ownership: <none | list, explained? yes/no>
1. <item> -- VERIFIED | PARTIAL | NOT DONE -- <what you ran or read, what it showed>
Hygiene: <ok | problems>
Verdict: MERGE | MERGE WITH FOLLOW-UP (<items>) | DO NOT MERGE (<why>)
```
VERIFIED only with evidence you produced yourself; anything unchecked is PARTIAL with the reason. Be brief.
