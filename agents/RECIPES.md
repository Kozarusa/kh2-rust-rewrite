# kh2-rs worker recipes (how-to by topic; read the sections your brief names; the rules are in `agents/WORKER.md` and `agents/RULES.md`)

## R1. Reading the original
- `$env:KH2_DECOMP\decomp_typed\<image>\<addr>.c` (images main, ovl0..ovl3; same file names as `decomp\`, which stays as the untyped
  reference; it also covers about 1,000 code pieces `decomp\main` lacks). A matched function is named `<name>_<addr>`, an unmatched one
  `FUN_<addr>`: grep callers by the bare address (`grep -l "001802e0(" decomp_typed/main`), not by `FUN_`.
- The comment `/* LV4: FM <addr> <FM name> (method, confidence, sim); govanify C: src/<file> */` says where the function is in Final Mix
  and where a matching C body is (`$env:KH2_DECOMP\third\govanify-kh2\src`). Trust names at confidence >= 0.8; below 0.6, or "out of
  order", the name is a hint. FM struct offsets may differ from USA.
- Types: OBJ (actor), MOTION (OBJ +0x140), STATUS (OBJ +0x48c), SAVE_BLOCK (0x33e860, global `g_save`), PARTY_BLOCK, PARTY_MEMBER,
  FORM_RECORD. Field names are ours (`raw\lv4_structs.json` gives the evidence of each). A parameter comment `LV4 <rule>` says why it is
  typed; the `offsets`, `calls`, `down`/`up` rules are inferences.
- `$env:KH2_DECOMP\raw\fm_usa_match.json`: one row per USA function: usa_addr, fm_addr, name, demangled, confidence, method, sizes, sim,
  out_of_order, usa_in_decomp, module, fm_source. `raw\fm_usa_data.json`: USA to FM data addresses.
- No `.c` file: decompile with `ghidra script run $env:KH2_DECOMP/scripts/DecompileAt.java --project kh2ps2 --program SLUS_210.05 --
  <addrs.txt> <out.c>` (addrs.txt one hex address per line; both files in your scratchpad), or scan the instructions:
  `python tools\mips_scan.py --jal 0x1a0180` (call sites), `--store-off 0x180` / `--load-off 0x238` (field writers/readers), `--li 0x1c4`
  (constants), `--addr 0x37b540` (address builders); `(?)` = in a function the decomp lacks. It does not find fields written through a
  computed base or register offset: say so before concluding "no writer exists".
- Executable `$env:KH2_DECOMP\input\SLUS_210.05`; Ghidra project `..\ghidra_projects\kh2ps2_typed` (local/specs/LV4.md section 2).

## R2. Oracle tests and the factory
- Patterns: `crates/kh2/tests/summon_oracle.rs`, `crates/kh2-bdx/tests/oracle.rs`, `crates/kh2-fx/tests/oracle*.rs`,
  `crates/kh2-audio/tests/oracle*.rs`. `Ee::new()`, `load_elf` with the executable from the ISO; lay inputs at free addresses
  (0x0180_0000 up), point globals at them (`w32`); `ee.call(FUN, &[args], &[floats], STACK)`; compare outputs with the port over many
  inputs. Skip with a note when `KH2_ISO` is missing. Stub irrelevant callees with `jr ra` only, and say so in the test's doc comment.
- Factory loop for a cluster (local/specs/FAC.md 2.1): port each function on the C7 layout types (`layout!` blocks, pointer fields as
  handles), named for what it does, not a MIPS transliteration; `factory gen <addr> <rust::path> <file>` into
  `crates/<crate>/tests/orig_<cluster>.rs` (one file per cluster); fix only what the brief marks (`Range`, `nulls: false`, globals, stubs);
  `cargo test -p <crate> --test orig_<cluster>`; never relax a comparison or the case count; after three failed fix rounds mark the
  function `blocked` with the failing case and go on; cap 90 minutes. TODO(orchestrator: remove when WP-P0C is merged): `kh2-factory`
  and `kh2 ledger brief <system> [cluster]` are planned in WP-P0C.

## R3. BDX host calls and enemies
- Host calls: the `match` in `crates/kh2-app/src/enemy_host.rs`; each arm is `imp` (exact), `approx(.., "why")`, `stub(.., "why")` or
  `missing(.., "why")`; `KH2_BDXLOG=1` prints the per-run table. Names and signatures: `docs/bdx.md` "Named host functions". Object
  fields go in `Obj` (`enemy_obj.rs`), one per game field, doc comment with offset and writer. Test with `kh2_bdx::MockHost`
  (`crates/kh2-bdx/tests/disc.rs`) and the oracle (`cargo test -p kh2-bdx --test oracle -- --nocapture` ends "0 failures"). Update
  `docs/enemy-ai.md` "Host call mapping".
- New enemy: pick from data (`cargo test -p kh2-ard --test disc -- --nocapture`); add `enemy::Kind` to `enemy::KINDS`; its `enmp` constant
  (host 2.9) from `cargo run -p kh2 -- bdx <model>`; run `KH2_BDXLOG=1 KH2_WAVE_GROUP=<group> target\debug\kh2-game.exe tt04` and
  implement each `missing` call; add it to the oracle list and a mode-cycle test like `dusk_runs_its_mode_cycle`; screenshot and LOOK.

## R4. Harness, acceptance, rig
- `target\debug\kh2-game.exe <area>` (tt00 arena, tt02, tt04 town). `KH2_SCREENSHOT=<png>` + `KH2_SHOT_AT=<s>` (nothing before ~6 s),
  `KH2_KEYS=t:key[:hold],...` (keys J K Q Space F3 F4 F6 W A S D E C Left Right Up Down T G B; other names are ignored silently),
  `KH2_NO_SOUND=1 KH2_NO_MUSIC=1`, `KH2_NO_ENEMIES=1`, `KH2_WAVE_GROUP=b_44`, `KH2_YAW` (normal play only), `KH2_DIST`. Full list:
  `grep -rhoE "KH2_[A-Z0-9_]+" crates/kh2-app/src | sort -u`.
- Accept scenarios (`tools\accept.ps1`, orchestrator-owned) are lines `@{ Name; Area; Env; Keys; ShotAt; Expect; Soft; Look }`; propose
  yours with exact log substrings for `Expect`. TODO(orchestrator: remove when R15 lands): `KH2_TURBO=1`, `KH2_SHOT_TICKS` and
  `accept.ps1 -Turbo -Jobs 8` (parallel turbo sweep) are planned.
- Rig: `scenarios/<system>/<name>.toml` (save-state name, pad script keyed on vsyncs, checkpoints, values, tolerances; no game data)
  runs the original and the port from one state with one input and compares named values (local/specs/RIG.md 2). TODO(orchestrator:
  remove when RIG-5 lands): `tools\rig.ps1 <scenario>` and the report in `local\rig\<run>\` are planned.

## R5. Tables, HUD, effects, sounds
- Layouts: `sheets/tables.json` rows generate structs; `cargo run -p kh2 -- table <name> <row>` prints real rows. Values never go in sheets or code.
- HUD: field `.2dd` groups (kh2-image `Seqd`), text `msg/us/*.bar`, fonts `fontinfo.bar`; modules `hud_*.rs`, draw path `hud_gfx.rs`.
  Debug: `KH2_HUDLOG`, `KH2_HUDTEST`, `KH2_HUDMENU`, `KH2_HUDTEXT=<hex>`, `KH2_MSG=<hex>[:class]` (docs/hud.md "Debug aids").
- Effects: `fx::StartEffect` (owner's `.pax`/`.apdx`), `fx::StartSystemEffect { id, at }` (`00effect.bar`). Sounds: `audio::PlaySe`
  (`.at(pos)`), voices via `kh2_audio::voice`; `KH2_SOUNDLOG=1`. A wav for the user goes in `local\<wp>-audio\`; say "nobody has listened".
  Loopback recording of what the machine plays: `cargo run -p kh2-looprec --release -- <out.wav> <seconds>`; loudness:
  `python tools\wavstats.py <wav>`.

## R6. PCSX2 and reference videos
- Installed PCSX2 `$env:KH2_PCSX2` (data `$env:KH2_PCSX2_DATA`, PINE enabled, slot 28011): `Start-Process $env:KH2_PCSX2 -ArgumentList
  '-batch','--',"`"$env:KH2_PCSX2_ISO`""` (the PCSX2 copy of the ISO; ~20 s to the opening). The patched build with the debug server
  (TCP 21512: breakpoints, memory, pad injection at 0x167370, RIG.md 1.3) is `$env:KH2_PCSX2_MCP`. Close what you started when done
  (it competes with builds); the lock rule is in WORKER section 2.
- `python tools\pine.py status | r32 <addr> [n] | r8 | rf | watch <addr> [s] [32|16|8|f] | save <slot> | load <slot>` (EE addresses as in the
  decomp: save header 0x33e860 "KH2U-", inventory 0x340ce8); `python tools\dbg.py '{"cmd":"status"}'` (debug server);
  `python tools\pcsx2_shot.py <png>` grabs the window. PINE has no key injection; the debugger (Debug > Open Debugger) breaks on reads and
  writes. Final Mix works the same (research only).
- Videos `$env:KH2_VIDEOS` (4.mp4 .. 10_menu_open_close.mp4); sheets in `local\videoframes\<n>_sheet.jpg`; frames:
  `python tools\vidframes.py 4.mp4 <seconds...>`. LOOK; describe only what is visible. 4: combat, command deck LEFT edge, HP ring/MP bar
  RIGHT edge, Reversal prompt, green lock-on triangle. 5: tutorial pages, pause menu main screen. 6: Abilities and Items screens. 7: save
  point, save menu and file list. 8: area transition wipe and area title. 9: prize orbs and munny pickup, "NEXT LEVEL"/"OBTAINED MUNNY"
  strips. 10: pause menu open/close timing (1080p, Sora/Donald/Goofy).

## R7. Merge conflicts (orchestrator)
- Host-call `match`: keep both arms (same `group/index` twice: keep `imp`/`approx`, drop `stub`/`missing`); `Obj` fields and
  `Obj::new` keep both; appended doc sections keep both, delete markers; plugin tuples keep both (max 15 per tuple); `Cargo.lock`
  `git checkout --theirs Cargo.lock`, `cargo check`, commit the lock alone; `CREDITS.md` keep both rows, CRLF; type mismatch without a text
  conflict: `tools\trial-merge.ps1` shows it, fix in the merge commit. Resolve with Python (`newline=''`), then `cargo check --workspace --tests`.

## R8. Bookkeeping, cleanup, main (orchestrator)
- O5 bookkeeping per merge: CHANGELOG line under the shift heading `**WP-<id> (<topic>).** <what>. Verified by <tests/logs/screenshot>.
  Open: <gaps>.` (from the report's blocks); `sheets/systems.json` rows by Python (`newline=''`, `json.load` after), true statements only;
  `README.md` (CRLF) feature bullets and "Not done yet"; `agents/DEVIATIONS.md` fixes -> "fixed <commit>, user not yet re-checked".
- O6 cleanup (A11-A13, B7): `git worktree remove --force --force .claude\worktrees\wp-<id>`; `git branch -d wp/<id>`; its target dirs.
  "Permission denied" while the agent lives: retry after the next merge; still stuck at a stopping point -> Needs your attention with path
  and size. Older material is MOVED to `local\archive\<date>\`, never deleted. Keep the two newest `local\accept\<run>` folders plus
  `local\accept\pre-shift5` and `local\baseline`.
- Finished briefs move from `local\briefs\` to `local\archive\briefs\` (only live briefs stay in `local\briefs\`).
- O7 main (A8, C6): `git merge-base --is-ancestor main integration; if ($?) { git branch -f main integration } else { "main has diverged: stop" }`,
  then push `main` and `integration`.

## R9. Orchestrator pitfalls (learned)
- Agents die on 429 limits; commits survive, uncommitted work does not (`tools\snapshot.ps1`). Workers repeat final reports: ignore repeats.
- Agents spawned with `isolation: "worktree"` could not use git/powershell reliably: use pre-made worktrees.
- Five worktrees building at once filled drive C (10-19 GB each): spawn by B5, remove merged worktrees at once. A shared `CARGO_TARGET_DIR`
  corrupted a build (fix: `cargo clean -p <crate>`).
- `-File x.ps1 -Only a,b` passes one string; the scripts split on commas.
- A game window locks `target\debug\kh2-game.exe`: verify falls back to `cargo check` and prints NOTE (tests still count).
- Screenshots before ~6 s are blank. `KH2_YAW` does not turn the camera in the motion viewer. Bevy plugin tuples hold at most 15 entries.
- Embedded asset paths (`embedded://<crate>/...`) follow the crate holding `embedded_asset!`: after a move the HUD vanished while accept
  said PASS (shift 13). Accept PASS is log-only: LOOK at idle-hud and one menu PNG after a structural merge.
- A merge undone with `git revert -m 1 HEAD` is not merged again by a second merge.ps1: first `git revert --no-edit HEAD` to reapply it.
  `-ExpectTests` is a minimum: too high makes verify FAIL and reverts a good merge (shift 16, WM4): use the worker's count or lower.
- Untracked process files get lost (accept.ps1 truncated 2026-10-21): they are tracked in `tools/` and `agents/` now; never write a file
  with `open(p,'w').write(open(p).read())`.
