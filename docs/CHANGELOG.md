# Changelog

One short section per work package. "Verified" says how; nothing here was checked by watching video, and audio was listened to only where stated.

## Shift 5

- **WP-5A (finish 2G2, combo depth).** Aerial Recovery now needs the jump button; explicit actions (Finishing Leap, Upper Slash, Horizontal Slash) start from Square in the order of the original's code; attack-record key 1 is a ledge probe (no floor ahead), not a wall probe; the hit point uses the attack volume's centre. Verified by unit tests, `KH2_MOVELOG` runs and the ground/air acceptance scenarios. Open: the A343 keyblade shaft is missing at motion frames 18 to 20 (culling ruled out, cause unknown); a successful Aerial Recovery was not observed in a real run.
- **WP-5B (finish 2B2, drive polish).** Outfit variants and hand weapons per form from the disc tables; Anti has no hand weapon (empty row); the player's drive-gauge charge rule (commands 455 and 457) ported as tested code. Verified by tests, per-form harness logs and screenshots of Valor, Wisdom, Final and Anti. Open: the Anti partial MP drain was not found; the charge rule is not wired into Sora; world-variant keyblade files are not on the disc, so the plain model is used (inferred).
- **WP-5C (player side of reaction commands).** Soldier command 34 plays its paired dodge on Sora (no input, unhittable, secondary velocity, queued motion) and clashes recoil or bounce Sora per the original handler. Verified by unit tests and a harness run with logs and screenshots. Open: air push, a few clash branches.
- **WP-5D (positional audio).** Positional sounds are re-mixed every tick against the camera, as the original does. Verified by tests and a rendered pan sweep that nobody has listened to. Open: the kind 8 landing footstep (the gate was not found).
- **WP-5E (camera follow-ups, clash spark).** Look-around fades Sora and his weapons as alpha; the clash spark starts through a system-file effect. Verified by camera logs and screenshots. Open: the Quick Run camera speed (no writer found), the guard-bounce spark was not seen in a run.

## Shift 4

- **WP-4A2 camera.** Field camera model, look-around, area camera types, attribute-5 collision.
- **WP-4B3 HUD phase 3.** Gauges, command menu with the reaction capsule, text with commands, area-name card, banners, party panel code.
- **WP-1F3 effects.** Sno clip, cull and bias verified against the game; bind rules; death and leave effects.
- **WP-1B audio.** Voice RNG and line picks (oracle-checked), USA voice path, battle music switch, footsteps. Three renders were listened to and approved.
- **WP-2C2 reaction commands.** Native offers from collision parts, event 11 delivery, Soldier command 34 on the script side, clashes between enemy attackers.
- Free-aim reticle hidden until a target is locked (a screenshot-only reading had drawn it).

## Shift 3 and earlier

Combat slice: Sora's movement, ground and air combos, guard, lock-on; Shadow and Soldier enemies; hit detection and the damage formula; effects runtime; the enemy script interpreter (0 differences against the game's own code over 1,246 scripts); map, collision and sky. See `README.md` and `sheets/systems.json`.
