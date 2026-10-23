# Common brief text (every work package; the spawn prompt names this file and your brief)

Read in this order: `agents/RULES.md`, `agents/WORKER.md`, the sections of `agents/RECIPES.md` and the docs, specs and ledger rows your
brief names, then your brief. Paths are relative to the main checkout (`$env:KH2_REPO`) unless they name your worktree.

- Worktree: yours is `.claude\worktrees\wp-<ID>` on branch `wp/<ID>` (already created from integration). Work only there (absolute
  paths), never in the main checkout. Copy `local\kh2idx.txt` into your worktree's `local\` before running disc tests.
- Builds: incremental in your edit loop (RULES B6); the gate with `CARGO_INCREMENTAL=0`:
  `$env:CARGO_INCREMENTAL='0'; powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify.ps1 -ExpectTests <N> -MaxClippy <M>` from
  your worktree root, on a committed tree (N and M are in the spawn prompt). Tests may only go up. Commit often, plain-prose messages
  ending with `Co-Authored-By: Claude <your model, e.g. Opus 5.5 or Sonnet 5.5> <noreply@anthropic.com>`.
- Decide yourself when the user is away (RULES A20): label each decision "decided:" in your report; never wait for an answer.
- Shared machine: other workers run at once. PCSX2 only one instance, with `local\pcsx2.lock` (WORKER 2.12). Never stop processes you did
  not start. Disc-reading scans over many files run through `tools\capped.ps1 -CapMB 2500` (RULES A19); one at a time.
- Rules that never bend: A3-A5 (no game data, no decompiled text, no decompiler or FM names in the repo), A6 (never Fable), A9 (no
  amend/force/reset --hard), A14 plain prose. Names from `decomp_typed` stay out of the repo.
- Report once, in the format of `agents/WORKER.md` section 5 (at most about 40 lines with its blocks LEDGER, DOCS, FOLLOW-UPS).
