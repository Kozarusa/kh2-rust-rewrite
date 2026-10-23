# kh2-rs WORKER guide (read this, `agents/RULES.md`, then your brief; how-to: `agents/RECIPES.md`; reviewers: `agents/briefs/reviewer.md`)

RULES win over this file. You do ONE work package (WP) in your own worktree and send ONE report (section 5). The orchestrator checks it by
script (`tools/review.ps1`: ownership, claimed tests, hygiene, trial merge), then a reviewer reruns your commands and opens your
screenshots: give EXACT commands (env vars included) and paths for every claim; what cannot be reproduced counts as not done.

## 1. Paths and start
- The main checkout `$env:KH2_REPO` is READ-ONLY for you. Your worktree: `.claude\worktrees\wp-<id>`, branch `wp/<id>`. Work only there,
  with absolute paths. Every tool is in `tools\` of your own worktree and works on the checkout it lives in.
- Machine paths come from the untracked `local\machine.ps1` of the main checkout (names in `tools\machine.example.ps1`). In every
  PowerShell call that runs cargo or the game: `. .\tools\env.ps1; Use-Machine KH2_ISO; ...` (more names as needed:
  `Use-Machine KH2_ISO,KH2_REPO,KH2_DECOMP`). Python tools use `tools\machine.py`.
- Start: (1) `git status --short` (empty), `git log --oneline -5`, `git switch wp/<id>` if needed. (2) Copy `$env:KH2_REPO\local\kh2idx.txt`
  into your `local\`. (3) First build in the background: `cargo build -p kh2-app --bin kh2-game` (cold 10 to 18 min, longer while others build); read meanwhile.
  (4) Edit loop: `CARGO_INCREMENTAL` may stay unset (RULES B6); the gate runs with it at 0. Never point `CARGO_TARGET_DIR` at another
  checkout's target. (5) Scripts: `powershell -NoProfile -ExecutionPolicy Bypass -File <script>`.
- Read `docs/METHOD.md`, `docs/systems/foundations/research.md`, `docs/README.md` (the index of `docs/systems/<system>/`), and what
  the brief names: the system's `contract.md` first, then only the detail files you need, and the ledger rows of your cluster.

## 2. Hard rules (beyond RULES)
1. `CREDITS.md` is CRLF; only append rows. Never commit `sheets/systems.json` (`kh2 validate` rewrites it: `git checkout` it after).
2. Edit only the files your brief owns; a change elsewhere goes under "Outside-ownership edits needed". Exception: a compile fix forced by
   your own signature change.
3. Commit after every finished step: plain prose, one step per commit, ending with the `Co-Authored-By:` line of your system instructions.
   Never amend, force, touch other branches, or commit `*.png *.wav *.bin`, disc data, decomp text, machine paths or `local/`.
4. Nobody watches video or listens unless the brief says so. Claim only what a test, a log line, or a screenshot you LOOKED at (Read tool)
   shows, and describe only what is visible.
5. Keep base behaviours working: ground combo and air chain (`cargo test -p kh2-player sora`), enemy AI, HUD, camera, magic, forms.
6. Never `Stop-Process` kh2-game (A10). A running game locks `target\debug\kh2-game.exe`: use `cargo check`/`cargo test` and say so.
7. Evidence (A4): read the function the brief names first (`$env:KH2_DECOMP\decomp_typed\<image>\<addr>.c`, RECIPES R1), then its
   callees. Repo text keeps addresses only ("FUN_00180758" or "0x180758"): never a decompiler variable name (`iVar<n>`, `param_<n>`), an FM or
   community function name, a decomp field name, or a copied table value (read tables at run time through `kh2-ee` or `kh2_archive`).
   Describe field offsets by owner (`actor +0x588`) with the writer and reader of each field you use. Record every address read in your
   doc's "Decompile index", every fruitless search in "Gaps". At the brief's time cap: record the gap and move on, the feature left out
   or conservative. Never guess; label inferences `[inferred]`.
8. Ledger convention (C2): `/// orig 0x1a0180` in a doc comment of non-test code marks that address `ported` (one line per address); a
   test `orig_<addr>_matches_the_game` marks it `verified`; other mentions are evidence for `read`. Never hand-edit `read/ported/verified`
   in `sheets/functions.json` (only `kh2 ledger scan` sets them).
9. Factory merge rule (C7): a package merges without a reviewer only when (a) its diff touches only its cluster's port files, the
   cluster's test file and ledger rows; (b) every new test came from `factory gen` and keeps the harness invariants (generated header,
   `fuzz(&SPEC, n)` with n >= 1,000, no extra early return, every stub listed in the brief); (c) the tests ran green with the disc
   present (`KH2_REQUIRE_DISC=1`); (d) no wiring change (systems, plugins, schedule). Anything else goes through the reviewer.
10. CRLF files: `crates/kh2-app/src/main.rs`, `lib.rs`, `plugins.rs`, `crates/kh2-enemy/src/enemy.rs`, `CREDITS.md`, `README.md`; `fx.rs` is CRLF except its
   test module. Edit Rust with Edit/Write; Python patches use `open(p, newline='', encoding='utf-8')`; check `git diff --stat` after each.
11. Every `mod` line lives in the lib.rs of its game crate (`crates/kh2-<crate>/src/lib.rs`; `crates/kh2-app/src/main.rs` only calls `kh2_app::run()`). Each new plugin goes into its domain
   bundle in `crates/kh2-app/src/plugins.rs` (plugins stay in kh2-app; a `PluginGroup`); keep the registration order.
12. PCSX2: one instance at a time. Before starting one, create `$env:KH2_REPO\local\pcsx2.lock` holding your id (a lock younger than
   2 hours: wait); delete it when you close PCSX2. Stop only an instance you started.

## 3. Prove a port
An oracle test (kh2-ee runs the original from the user's executable) compares the port over many inputs; patterns and the factory loop
are in RECIPES R2. Skip with a note when `KH2_ISO` is missing; stub only irrelevant callees (`jr ra`) and say so in the doc comment.
Every test you add asserts something (an eprintln-only test is not a test).

## 4. When stuck
| situation | do |
|---|---|
| rule not found within the cap | record the gap, keep the feature out or conservative, next item |
| two readings of the code | port the more direct one, mark `[inferred]`, write the other in Gaps |
| need a file you do not own | do not edit; report file, function, exact change |
| a test that is not yours fails | do not fix it; find the commit (`git log -p -- <file>`), report it |
| clippy count rises | fix your own warnings; never `#[allow]` someone else's; `too_many_arguments` on your own new fn is fine |
| the same decomp trace fails twice | stop and report "stuck: <the trace>" (the orchestrator retries on opus) |
| context runs low | stop new work, commit (say whether it compiles), report done / not done / exact next step |

## 5. Gate, then report ONCE
From your worktree root, on a committed tree: `$env:CARGO_INCREMENTAL='0'; powershell -NoProfile -ExecutionPolicy Bypass -File
tools\verify.ps1 -ExpectTests <N> -MaxClippy <M>` (N, M from the spawn prompt). Tests must not drop or fail; warnings must not rise.
Check that `git diff --stat integration...HEAD` stays inside your ownership. At most about 30 lines; longer material goes into a file in
`local\reports\` that the report names.
```
WP-<id> on branch wp/<id>, worktree <path>. Status: done | partly done (which items) | not done.
Commits: <hash> <one line>, ... (last commit compiles: yes/no)
Gate: verify.ps1 -ExpectTests <N> -MaxClippy <M> -> TESTS: <n> passed, 0 failed; CLIPPY: <m>; VERIFY: PASS
Files changed: <list>; outside ownership: <none | list with reason>
Per item: what was done; evidence (test name, exact log line, or "screenshot <file> looked at: <what is visible>"); addresses; [inferred] parts.
Not verified: <list> (always: audio not listened to; video not watched). Gaps: <each with the addresses searched>.
Outside-ownership edits needed: <file, function, exact change> | none. Accept lines proposed: <@{ Name = ...; } lines> | none.
Sheet rows suggested: <row id: text> | none. Decisions: <"decided: ..." (RULES A20)> | none.
Tests claimed (for review.ps1 -Tests): <comma-separated test fn names>.
TESTS-ADDED: <n>   SCENARIOS: <names> | none
LEDGER: <functions newly read/ported/verified, or n/a>
DOCS: <docs touched>
FOLLOW-UPS: <WP ids/one line each>
```
