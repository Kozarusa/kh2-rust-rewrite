## Shift 6 (work done while the user was away; nothing here was play-tested by the user)

- **WP-6A (gaps).** The Mickey charge handler belongs to a Mickey-class player, not Sora (Sora sends 455 and 457 down the generic command path); the A343 keyblade "bug" is Sora's body hiding the blade from the default camera (the pose matches the game's evaluator); the kind 8 footstep is the stop step on stick release (ported); Anti has no separate MP drain (an instruction scan found no writer).
- **WP-6B (magic phase 1).** Magic page in the command menu; Fire, Blizzard, Thunder and Cure at level 1 with the original's MP cost, charge and damage rows. Verified by 8 new tests, a harness run with log lines, a passing `magic-fire` scenario and a looked-at screenshot. Open: voices, cast effects at the keyblade tip, levels 2 and 3, Magnet, Reflect, drive-form casts.
- **WP-6C (second enemy).** The area scripts of tt00, tt02 and tt04 spawn no Shadow and no Soldier; tt04 spawns Dusks and others. Dusk (M_EX990) runs under its own script with the oracle at 0 differences, and tt04 waves follow its battle groups. New crate `kh2-ard`.
- **WP-6D (HUD gaps).** The original draws no HP digits. Message windows (frame from the world message file, four classes) are drawn and can be shown with `KH2_MSG`. Open: closing, page breaks, selection slots, the callers that open windows.
- **WP-6E (reaction and clash gaps).** Sora's 455/457 generic path, flags bit 6 windows, the reflect cone, owner flags, recoil blend 0, a forced guard-bounce spark harness. Open: air push, `ReflectAction`, several reaction rows with no known offerer.
- **WP-6F (summons spec).** `docs/summons.md` with disc and kh2-ee oracle tests. The party-member AI is the big prerequisite.
- **WP-6G (enemy follow-ups).** Clash event 19, the Dusk surprise message path, the drop computation (no prize objects yet), enemy lists from `KINDS`.
- **WP-6H (second enemy type).** N_EX570_BTL (the name "Seifer" is inferred) runs under its own script in tt04; spawn parameters, HP record and trigger-bit host calls. Open: the frame-trigger bit was never seen set in a run; Struggle rules.

