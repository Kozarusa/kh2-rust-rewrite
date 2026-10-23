//! `kh2 ledger`: the function ledger (`sheets/functions.json`, PLAN section 4) and the factory's merge gate.
//!
//! - `import [--reseed]` builds the sheet from the private workspace (`KH2_PS2`, default `../kh2_ps2`): every row of
//!   `sheet/code/functions_*.json` plus the function starts the typed project adds (`raw/decomp_typed.jsonl`, note
//!   `lv4`), seeds the systems (below), then scans. Existing rows keep their system unless it is `unsorted` (or
//!   `--reseed`); conflicts go to `local/ledger-conflicts.txt`.
//! - `scan [--demote]` sets `read`/`ported`/`verified` from the tree (`kh2_sheets::ledger`). A `ported` or `verified`
//!   row the tree no longer supports is an error; `--demote` writes the lower status instead.
//! - `report [--system s]` prints functions and bytes per status, per system and ladder step.
//! - `brief <system> [cluster|lo-hi] [--spec]` prints the skeleton of a work package brief (under 4 KB).
//! - `merge-check <git diff range> [--run]` says whether a package may merge without a reviewer (FAC 2.1).
//!
//! Seeding, in order: overlay image (each overlay is one system); the Sony runtime (main at or above 0x2f8210, or
//! tagged `sdk` in the workspace) becomes `sdk` / `not-needed:sdk`; the system whose docs cite the function most;
//! the system whose code cites it most; the workspace's own tag; then call-graph propagation: a function whose
//! callers are all in one system joins it, a function called from several systems that calls nothing outside `core`
//! and `sdk` becomes `core`, a function whose callees (outside `core`/`sdk`) are all in one system joins it, a
//! function between two neighbours of the same system joins it; the rest stays `unsorted`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use kh2_sheets::ledger::{self as lg, Cites, IMAGES, Key, Row, fmt_addr};
use serde_json::{Value, json};

/// The ledger systems: id, ladder step (PLAN section 5), `sheets/systems.json` rows, what they hold.
pub const SYSTEMS: &[(&str, &str, &[&str], &str)] = &[
    ("core", "L1", &["app"], "shared leaves and helpers: math, vectors, lists, memory, strings"),
    ("core.task", "L1", &["app"], "object and task system, frame order and timing"),
    ("core.file", "L1", &["fs.archive", "fs.bar"], "file loading: IDX/IMG archive, BAR, reads"),
    ("core.area", "L1", &["world.event", "game.jump"], "area load and jump"),
    ("core.script", "L1", &["world.event"], "area scripts (ard) and their opcodes"),
    ("core.story", "L1", &["game.story"], "story flags and their side effects"),
    ("core.save", "L1", &["game.progress"], "save block, progress, inventory, memory-card saves"),
    ("core.collision", "L1", &["world.collision", "world.bob"], "COCT collision: ground, ceiling, raycasts"),
    ("core.motion", "L1", &["anim.motion", "anim.triggers", "anim.ik", "anim.expressions"], "skeleton, motion evaluation, triggers"),
    ("render.model", "L1", &["render.model", "render.texture", "render.uvscroll", "world.map"], "model, map and texture draw"),
    ("render.gs", "L1", &["ps2.vif", "ps2.gs"], "VIF and GS packets, DMA (candidates for not-needed:hw)"),
    ("player.sora", "L2", &["game.sora", "game.player", "game.sora.dodge"], "Sora's controller, actions, growth abilities"),
    ("player.camera", "L2", &["app"], "camera types, blockers, look-around"),
    ("combat.attack", "L3", &["game.combat"], "attacks, hit detection, hit stop, clash"),
    ("combat.damage", "L3", &["game.combat"], "damage, HP, status effects"),
    ("combat.reaction", "L3", &["game.combat", "game.sora.abilities"], "hit reactions and reaction commands"),
    ("combat.prize", "L3", &["game.prizes"], "prizes and drops"),
    ("combat.level", "L3", &["game.sora.abilities"], "experience, levels, bonus rewards"),
    ("combat.ability", "L3", &["game.sora.abilities"], "abilities incl. passives"),
    ("fx.pax", "L3", &["fx.effects"], "the PAX effect runtime"),
    ("fx.draw", "L3", &["fx.lens"], "effect drawing, lens and distortion"),
    ("magic", "L4", &["game.sora.magic"], "spells, Magic Lock-on, MP"),
    ("drive", "L5", &["game.sora.abilities"], "drive forms, Anti rules, form levels"),
    ("limit", "L5", &["game.sora.abilities"], "limit scenes"),
    ("summon", "L5", &["game.summons"], "summons, their limits, the summon task"),
    ("party", "L6", &["game.party"], "party members, world partners, member swap"),
    ("party.friend_ai", "L6", &["game.friend_ai"], "friend AI"),
    ("script.bdx", "L7", &["game.ai_bdx"], "BDX script interpreter and host functions"),
    ("enemy", "L7", &["game.enemies"], "enemy objects and bosses"),
    ("npc", "L7", &["world.talk"], "NPCs, talk"),
    ("ui.hud", "L8", &["ui.image", "game.items"], "HUD: gauges, command menu, banners"),
    ("ui.msg", "L8", &["ui.msg"], "message text and fonts"),
    ("ui.menu", "L8", &["ui.menu"], "pause menu and its pages"),
    ("ui.menu.journal", "L8", &["ui.journal"], "Jiminy's Journal"),
    ("ui.menu.shop", "L8", &["ui.shop"], "shops and synthesis"),
    ("ui.menu.save", "L8", &["ui.save", "ui.load"], "save and load screens, save points"),
    ("ui.menu.gameover", "L8", &["game.gameover"], "game over"),
    ("ui.menu.report", "L8", &["ui.battle_report"], "BATTLE REPORT"),
    ("ui.tutorial", "L8", &["ui.tutorial"], "tutorial windows"),
    ("ui.title", "L8", &["app"], "title screen, new game, config (overlay ovl1)"),
    ("audio", "L9", &["audio"], "sound driver, sounds, voices, BGM"),
    ("event.player", "L10", &["game.events", "game.intro"], "event player, battle intro, event camera"),
    ("event.movie", "L10", &["app"], "PSS movie player (overlay ovl2, ovl_movie.x [inferred])"),
    ("world.minigame", "L11", &["game.struggle"], "other player classes, minigames, missions"),
    ("worldmap", "L12", &["app"], "world map"),
    ("gummi.flight", "L12", &[], "Gummi flight (overlay ovl0, ovl_gumibattle.x)"),
    ("gummi.menu", "L12", &[], "Gummi menu and editor (overlay ovl3, ovl_gumimenu.x)"),
    ("sdk", "-", &[], "Sony runtime and libc: not-needed:sdk unless ported"),
    ("unsorted", "-", &[], "not classified yet"),
];

const STEPS: &[&str] = &["L1", "L2", "L3", "L4", "L5", "L6", "L7", "L8", "L9", "L10", "L11", "L12", "-"];

/// Doc file stems (or folder names under `docs/systems/`) -> system. Docs not listed seed nothing.
const DOC_SYSTEMS: &[(&str, &str)] = &[
    ("abilities", "combat.ability"),
    ("audio", "audio"),
    ("bdx", "script.bdx"),
    ("camera", "player.camera"),
    ("drive-forms", "drive"),
    ("effects-damage", "fx.pax"),
    ("enemies", "enemy"),
    ("enemy-ai", "enemy"),
    ("events", "event.player"),
    ("footsteps", "player.sora"),
    ("gummi", "gummi.flight"),
    ("hit-reaction", "combat.reaction"),
    ("hit-stop", "combat.attack"),
    ("hud", "ui.hud"),
    ("items", "ui.hud"),
    ("limits-reactions", "limit"),
    ("magic", "magic"),
    ("menus", "ui.menu"),
    ("movement", "player.sora"),
    ("moveset-selection", "player.sora"),
    ("party", "party"),
    ("prizes", "combat.prize"),
    ("progress", "core.save"),
    ("render", "render.model"),
    ("struggle", "world.minigame"),
    ("summons", "summon"),
    ("worldmap", "worldmap"),
];

/// Code path prefixes -> system, first match wins. Paths not listed seed nothing.
const CODE_SYSTEMS: &[(&str, &str)] = &[
    ("crates/kh2-app/src/abilities", "combat.ability"),
    ("crates/kh2-app/src/area", "core.area"),
    ("crates/kh2-app/src/assets.rs", "core.file"),
    ("crates/kh2-app/src/audio", "audio"),
    ("crates/kh2-app/src/camera", "player.camera"),
    ("crates/kh2-app/src/clash", "combat.attack"),
    ("crates/kh2-app/src/combat", "combat.attack"),
    ("crates/kh2-app/src/data.rs", "core.file"),
    ("crates/kh2-app/src/drive", "drive"),
    ("crates/kh2-app/src/enemy_host", "script.bdx"),
    ("crates/kh2-app/src/enemy", "enemy"),
    ("crates/kh2-app/src/event_player", "event.player"),
    ("crates/kh2-app/src/friend", "party.friend_ai"),
    ("crates/kh2-app/src/fx_lens", "fx.draw"),
    ("crates/kh2-app/src/fx", "fx.pax"),
    ("crates/kh2-app/src/hud", "ui.hud"),
    ("crates/kh2-app/src/intro", "event.player"),
    ("crates/kh2-app/src/items", "ui.hud"),
    ("crates/kh2-app/src/limit", "limit"),
    ("crates/kh2-app/src/magic", "magic"),
    ("crates/kh2-app/src/menu_journal", "ui.menu.journal"),
    ("crates/kh2-app/src/menu_shop", "ui.menu.shop"),
    ("crates/kh2-app/src/menu_save", "ui.menu.save"),
    ("crates/kh2-app/src/menu_gameover", "ui.menu.gameover"),
    ("crates/kh2-app/src/menu_ending", "ui.menu.report"),
    ("crates/kh2-app/src/menu", "ui.menu"),
    ("crates/kh2-app/src/mickey", "party"),
    ("crates/kh2-app/src/motor", "player.sora"),
    ("crates/kh2-app/src/npc", "npc"),
    ("crates/kh2-app/src/party", "party"),
    ("crates/kh2-app/src/player_reaction", "combat.reaction"),
    ("crates/kh2-app/src/prize", "combat.prize"),
    ("crates/kh2-app/src/progress", "core.save"),
    ("crates/kh2-app/src/reaction", "combat.reaction"),
    ("crates/kh2-app/src/revive_fade", "summon"),
    ("crates/kh2-app/src/savepoint", "ui.menu.save"),
    ("crates/kh2-app/src/schedule", "core.task"),
    ("crates/kh2-app/src/skel", "core.motion"),
    ("crates/kh2-app/src/sora", "player.sora"),
    ("crates/kh2-app/src/struggle", "world.minigame"),
    ("crates/kh2-app/src/summon", "summon"),
    ("crates/kh2-app/src/texanim", "render.model"),
    ("crates/kh2-app/src/world.rs", "core.collision"),
    ("crates/kh2-app/src/worldmap", "worldmap"),
    ("crates/kh2/tests/ability", "combat.ability"),
    ("crates/kh2/tests/party", "party"),
    ("crates/kh2/tests/summon", "summon"),
    ("crates/kh2-anim/", "core.motion"),
    ("crates/kh2-archive/", "core.file"),
    ("crates/kh2-iso/", "core.file"),
    ("crates/kh2-bar/", "core.file"),
    ("crates/kh2-model/", "render.model"),
    ("crates/kh2-map/", "core.collision"),
    ("crates/kh2-fx/", "fx.pax"),
    ("crates/kh2-audio/", "audio"),
    ("crates/kh2-bdx/", "script.bdx"),
    ("crates/kh2-ard/", "core.script"),
    ("crates/kh2-image/", "ui.hud"),
    ("crates/kh2-ps2/", "render.gs"),
];

/// The workspace's own tags (`functions_main.json` column `system`) -> system.
const TAG_SYSTEMS: &[(&str, &str)] = &[
    ("battle.limits", "limit"),
    ("battle.summons", "summon"),
    ("sys.treasure", "core.save"),
    ("battle.levelup", "combat.level"),
    ("battle.attack", "combat.attack"),
    ("sys.items", "core.save"),
    ("battle.bonus", "combat.level"),
    ("battle.drops", "combat.prize"),
    ("battle.magic", "magic"),
    ("battle.voice", "audio"),
    ("sys.commands", "ui.hud"),
    ("sys.skeleton", "core.motion"),
    ("battle.player", "player.sora"),
    ("battle.drive", "drive"),
    ("battle.stop", "combat.damage"),
    ("battle.enemy", "enemy"),
];

/// One overlay is one system.
const OVERLAY_SYSTEMS: [&str; 4] = ["gummi.flight", "ui.title", "event.movie", "gummi.menu"];
/// The first main-executable function that touches hardware registers: the Sony runtime starts here (FAC 1.3).
pub const SDK_START: u32 = 0x002f_8210;

const DOC: &str = "One row per function of the original executable (main) and its four overlays (ovl0..ovl3, all loaded at 0x1f18000): addr, image, size in bytes, the ledger system, status, evidence (what the tree shows, written by the scan), note. Addresses, sizes and statuses only: callers, callees and names stay in the private workspace (RULES A5). Statuses: unread, read (cited in docs/ on a line that does not say the function was not read), ported (a `/// orig 0x<addr>` doc comment line in non-test Rust code; overlays `/// orig ovlN 0x<addr>`), tested (the address in a hand-written #[test] function's name or doc comment), verified (the same for an oracle test: it runs the original through kh2-ee, calling Ee or the factory harness directly or through a helper of its file, e.g. the factory's orig_<addr>_matches_the_game; verified over tested over ported over read), not-needed:<sdk|debug|hw|dead|fm>. read/ported/tested/verified are only set by `kh2 ledger scan`; system, not-needed and note are hand-edited. `systems` lists the ledger systems: [id, ladder step of PLAN section 5, sheets/systems.json rows, what]. A systems.json row is closed by a `closed` column or a `validated` text starting with 'closed'; the preflight then rejects any unread or read function of a system mapped to it. Rebuild: `kh2 ledger import` (rows from ../kh2_ps2, seeding, scan); report: `kh2 ledger report`. Note `lv4`: a function start the typed project added (not in the workspace's function sheets).";

fn ps2_root() -> PathBuf {
    kh2_factory::ps2_root()
}

fn sheet_path() -> PathBuf {
    super::root().join("sheets").join("functions.json")
}

/// The call graph of the private workspace, by ledger key.
#[derive(Default)]
pub struct Graph {
    pub size: BTreeMap<Key, u32>,
    pub callers: HashMap<Key, BTreeSet<Key>>,
    pub callees: HashMap<Key, BTreeSet<Key>>,
    /// The workspace's own system tag.
    pub tag: HashMap<Key, String>,
    /// Function starts the typed project added.
    pub lv4: BTreeSet<Key>,
}

impl Graph {
    fn link(&mut self, from: Key, to: Key) {
        if from != to {
            self.callees.entry(from).or_default().insert(to);
            self.callers.entry(to).or_default().insert(from);
        }
    }
}

/// `001a0180` (main), `ovl0_7200e604::01f18098` (an overlay).
fn ps2_key(s: &str) -> Option<Key> {
    match s.split_once("::") {
        Some((im, a)) => Some((lg::image_index(im.split('_').next()?)?, u32::from_str_radix(a, 16).ok()?)),
        None => Some((0, u32::from_str_radix(s, 16).ok()?)),
    }
}

pub fn load_graph(ps2: &Path) -> Result<Graph, String> {
    let mut g = Graph::default();
    let mut ids: HashMap<String, Key> = HashMap::new();
    let mut edges: Vec<(String, String)> = Vec::new();
    for im in IMAGES {
        let p = ps2.join("sheet").join("code").join(format!("functions_{im}.json"));
        let j: Value = serde_json::from_slice(&std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?)
            .map_err(|e| format!("{}: {e}", p.display()))?;
        let cols: Vec<&str> = j["columns"].as_array().ok_or("no columns")?.iter().filter_map(Value::as_str).collect();
        let ci = |n: &str| cols.iter().position(|c| *c == n).ok_or(format!("{}: no column {n}", p.display()));
        let (id, addr, size, callees, system) = (ci("id")?, ci("addr")?, ci("size")?, ci("callees")?, ci("system")?);
        for r in j["rows"].as_array().ok_or("no rows")? {
            let k = r[addr].as_str().and_then(ps2_key).ok_or_else(|| format!("{}: bad addr {}", p.display(), r[addr]))?;
            // An overlay sheet row names its own image.
            let k = (lg::image_index(im).unwrap(), k.1);
            g.size.insert(k, r[size].as_u64().unwrap_or(4) as u32);
            if let Some(t) = r[system].as_str() {
                g.tag.insert(k, t.to_string());
            }
            let rid = r[id].as_str().unwrap_or_default().to_string();
            for c in r[callees].as_str().unwrap_or_default().split_whitespace() {
                edges.push((rid.clone(), c.to_string()));
            }
            ids.insert(rid, k);
        }
    }
    for (a, b) in edges {
        if let (Some(&a), Some(&b)) = (ids.get(&a), ids.get(&b)) {
            g.link(a, b);
        }
    }
    // The typed project: extra function starts and its own call lists.
    let p = ps2.join("raw").join("decomp_typed.jsonl");
    if let Ok(text) = std::fs::read_to_string(&p) {
        let mut later = Vec::new();
        for l in text.lines() {
            let Ok(v) = serde_json::from_str::<Value>(l) else { continue };
            let Some(k) = v["address"].as_str().and_then(ps2_key) else { continue };
            if let std::collections::btree_map::Entry::Vacant(e) = g.size.entry(k) {
                e.insert(v["size"].as_u64().unwrap_or(4).max(1) as u32);
                g.lv4.insert(k);
            }
            for c in v["callees"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                if let Some(t) = ps2_key(c) {
                    // A bare address called from an overlay is in that overlay when it lies in the overlay area.
                    let t = if c.contains("::") || k.0 == 0 || t.1 < 0x01f1_8000 { t } else { (k.0, t.1) };
                    later.push((k, t));
                }
            }
        }
        for (a, b) in later {
            if g.size.contains_key(&b) {
                g.link(a, b);
            }
        }
    }
    Ok(g)
}

fn load_sheet() -> Result<(kh2_sheets::Sheet, Vec<Row>), String> {
    let s = kh2_sheets::load(&sheet_path())?;
    let (rows, errs) = lg::rows(&s);
    if let Some(e) = errs.first() {
        return Err(format!("{e} ({} row errors)", errs.len()));
    }
    Ok((s, rows))
}

fn system_of_doc(path: &str) -> Option<&'static str> {
    let parts: Vec<&str> = path.split('/').collect();
    let stem = parts.last()?.trim_end_matches(".md");
    let dirs = parts.iter().rev().skip(1).copied();
    std::iter::once(stem).chain(dirs).find_map(|n| DOC_SYSTEMS.iter().find(|(d, _)| *d == n).map(|(_, s)| *s))
}

fn system_of_code(path: &str) -> Option<&'static str> {
    CODE_SYSTEMS.iter().find(|(p, _)| path.starts_with(p)).map(|(_, s)| *s)
}

/// The system with the most citing files, and the runner-ups when there are several.
fn majority(files: Option<&BTreeSet<String>>, by: fn(&str) -> Option<&'static str>) -> Option<(&'static str, Vec<(&'static str, usize)>)> {
    let mut n: BTreeMap<&'static str, usize> = BTreeMap::new();
    for f in files? {
        if let Some(s) = by(f) {
            *n.entry(s).or_default() += 1;
        }
    }
    let mut v: Vec<(&str, usize)> = n.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let best = v.first()?.0;
    Some((best, v))
}

/// Seeds the system of every row whose system is empty (FAC/PLAN rules, see the module doc). Returns conflict lines.
pub fn seed(rows: &mut [Row], g: &Graph, c: &Cites) -> Vec<String> {
    let mut conflicts = Vec::new();
    let mut sys: HashMap<Key, String> = rows.iter().filter(|r| !r.system.is_empty()).map(|r| (r.key(), r.system.clone())).collect();
    let fixed: BTreeSet<Key> = sys.keys().copied().collect();
    for r in rows.iter_mut().filter(|r| r.system.is_empty()) {
        let k = r.key();
        let tag = g.tag.get(&k).map(String::as_str);
        if r.image > 0 {
            sys.insert(k, OVERLAY_SYSTEMS[r.image as usize - 1].into());
            continue;
        }
        if r.addr >= SDK_START || tag == Some("sdk") {
            sys.insert(k, "sdk".into());
            if r.status == "unread" {
                r.status = "not-needed:sdk".into();
            }
            if let Some(f) = c.orig.get(&k).or(c.code.get(&k)) {
                conflicts.push(format!("{}: Sony runtime, but cited in code ({}); kept sdk", fmt_addr(r.addr), f.iter().next().unwrap()));
            }
            continue;
        }
        let doc = majority(c.docs.get(&k), system_of_doc);
        let code = majority(c.code.get(&k), system_of_code);
        let pick = doc.as_ref().map(|d| d.0).or(code.as_ref().map(|d| d.0)).or_else(|| {
            tag.and_then(|t| TAG_SYSTEMS.iter().find(|(x, _)| *x == t).map(|(_, s)| *s))
        });
        if let Some((best, v)) = &doc
            && v.len() > 1
        {
            conflicts.push(format!("{}: docs of several systems {v:?}; chose {best}", fmt_addr(r.addr)));
        }
        if let (Some((d, _)), Some((cs, _))) = (&doc, &code)
            && d != cs
        {
            conflicts.push(format!("{}: docs say {d}, code says {cs}; chose {d}", fmt_addr(r.addr)));
        }
        if let Some(p) = pick {
            sys.insert(k, p.into());
        }
    }
    propagate(rows, g, &mut sys, &fixed);
    for r in rows.iter_mut().filter(|r| r.system.is_empty()) {
        r.system = sys.get(&r.key()).cloned().unwrap_or_else(|| "unsorted".into());
    }
    conflicts
}

/// The longest run of unknown functions the neighbour rule fills.
const MAX_GAP: usize = 16;

fn is_shared(s: &str) -> bool {
    s == "core" || s == "sdk"
}

/// Call-graph and neighbour propagation until nothing changes. `sys` holds the systems known so far.
pub fn propagate(rows: &[Row], g: &Graph, sys: &mut HashMap<Key, String>, _fixed: &BTreeSet<Key>) {
    let keys: Vec<Key> = rows.iter().map(Row::key).collect();
    let empty = BTreeSet::new();
    loop {
        let mut changed = false;
        // Callers all in one system (every caller known).
        loop {
            let mut n = 0;
            for &k in &keys {
                if sys.contains_key(&k) {
                    continue;
                }
                let callers = g.callers.get(&k).unwrap_or(&empty);
                if callers.is_empty() {
                    continue;
                }
                let s: BTreeSet<&str> = callers.iter().filter_map(|c| sys.get(c).map(String::as_str)).collect();
                if callers.iter().all(|c| sys.contains_key(c)) && s.len() == 1 {
                    let one = s.into_iter().next().unwrap().to_string();
                    sys.insert(k, one);
                    n += 1;
                }
            }
            if n == 0 {
                break;
            }
            changed = true;
        }
        // Shared helpers: called from several systems, calling nothing outside core/sdk.
        for &k in &keys {
            if sys.contains_key(&k) {
                continue;
            }
            let callers = g.callers.get(&k).unwrap_or(&empty);
            let s: BTreeSet<&str> = callers.iter().filter_map(|c| sys.get(c).map(String::as_str)).filter(|s| !is_shared(s)).collect();
            let callees = g.callees.get(&k).unwrap_or(&empty);
            let local = callees.iter().all(|c| sys.get(c).is_some_and(|s| is_shared(s)));
            if s.len() >= 2 && local {
                sys.insert(k, "core".into());
                changed = true;
            }
        }
        // Callees (outside core/sdk) all in one system.
        for &k in &keys {
            if sys.contains_key(&k) {
                continue;
            }
            let callees = g.callees.get(&k).unwrap_or(&empty);
            let s: BTreeSet<&str> = callees.iter().filter_map(|c| sys.get(c).map(String::as_str)).filter(|s| !is_shared(s)).collect();
            if s.len() == 1 && callees.iter().all(|c| sys.contains_key(c)) {
                let one = s.into_iter().next().unwrap().to_string();
                sys.insert(k, one);
                changed = true;
            }
        }
        // Neighbours: the nearest known functions before and after (same image) agree.
        let mut by_image: BTreeMap<u8, Vec<Key>> = BTreeMap::new();
        for &k in &keys {
            by_image.entry(k.0).or_default().push(k);
        }
        for v in by_image.values_mut() {
            v.sort();
            let known: Vec<Option<String>> = v.iter().map(|k| sys.get(k).filter(|s| !is_shared(s)).cloned()).collect();
            let mut prev: Vec<Option<&String>> = Vec::with_capacity(v.len());
            let mut last = None;
            for s in &known {
                prev.push(last);
                if s.is_some() {
                    last = s.as_ref();
                }
            }
            let mut next: Vec<Option<&String>> = vec![None; v.len()];
            let mut last = None;
            for i in (0..v.len()).rev() {
                next[i] = last;
                if known[i].is_some() {
                    last = known[i].as_ref();
                }
            }
            // Only short gaps: a long run of unknown functions between two of one system is more likely another unit.
            let mut i = 0;
            while i < v.len() {
                if sys.contains_key(&v[i]) {
                    i += 1;
                    continue;
                }
                let j = (i..v.len()).find(|&j| sys.contains_key(&v[j])).unwrap_or(v.len());
                if j - i <= MAX_GAP
                    && let (Some(a), Some(b)) = (prev[i], next[i])
                    && a == b
                {
                    for k in &v[i..j] {
                        sys.insert(*k, a.clone());
                    }
                    changed = true;
                }
                i = j;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Applies the tree's statuses. Returns (changed rows, demotions: rows the tree no longer supports).
pub fn apply_scan(rows: &mut [Row], c: &Cites, demote: bool) -> (usize, Vec<String>) {
    let mut changed = 0;
    let mut lost = Vec::new();
    for r in rows.iter_mut() {
        let (st, ev) = lg::derive(r, c);
        let rank = |s: &str| match s {
            "verified" => 4,
            "tested" => 3,
            "ported" => 2,
            _ => 0,
        };
        if rank(&st) < rank(&r.status) {
            lost.push(format!("{} {}: was {}, the tree now gives {st}", IMAGES[r.image as usize], fmt_addr(r.addr), r.status));
            if !demote {
                continue;
            }
        }
        if st != r.status || ev != r.evidence {
            r.status = st;
            r.evidence = ev;
            changed += 1;
        }
    }
    (changed, lost)
}

fn save(sheet: &mut kh2_sheets::Sheet, rows: &[Row]) -> Result<(), String> {
    lg::store(sheet, rows);
    sheet.save().map_err(|e| e.to_string())
}

fn cmd_import(args: &[String]) -> Result<(), String> {
    let reseed = args.iter().any(|a| a == "--reseed");
    let ps2 = ps2_root();
    let g = load_graph(&ps2)?;
    let path = sheet_path();
    let (mut sheet, old) = if path.exists() {
        load_sheet()?
    } else {
        let text = "{\"sheet\":\"functions\",\"key\":\"addr\",\"strut\":\"ledger\",\"columns\":[],\"types\":[],\"rows\":[]}";
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        let s = kh2_sheets::load(&path)?;
        (s, Vec::new())
    };
    sheet.meta.insert("doc".into(), Value::String(DOC.into()));
    sheet.meta.insert("hand".into(), json!(["system", "note", "status (not-needed only)"]));
    sheet.meta.insert("images".into(), json!(IMAGES));
    sheet.meta.insert("statuses".into(), json!(lg::STATUSES));
    sheet.meta.insert("systems".into(), Value::Array(SYSTEMS.iter().map(|(i, s, r, w)| json!([i, s, r, w])).collect()));
    let old: HashMap<Key, Row> = old.into_iter().map(|r| (r.key(), r)).collect();
    let mut rows: Vec<Row> = g
        .size
        .iter()
        .map(|(&k, &size)| {
            let mut r = old.get(&k).cloned().unwrap_or(Row {
                addr: k.1,
                image: k.0,
                size,
                system: String::new(),
                status: "unread".into(),
                evidence: String::new(),
                note: String::new(),
            });
            r.size = size;
            if g.lv4.contains(&k) && r.note.is_empty() {
                r.note = "lv4".into();
            }
            if reseed || r.system == "unsorted" {
                r.system.clear();
                if r.status.starts_with("not-needed:sdk") {
                    r.status = "unread".into();
                }
            }
            r
        })
        .collect();
    let dropped = old.keys().filter(|k| !g.size.contains_key(k)).count();
    let c = lg::scan_tree(&super::root(), &lg::index(&rows));
    let conflicts = seed(&mut rows, &g, &c);
    let (_, lost) = apply_scan(&mut rows, &c, true);
    save(&mut sheet, &rows)?;
    let cpath = super::local().join("ledger-conflicts.txt");
    std::fs::write(&cpath, conflicts.join("\n") + "\n").map_err(|e| e.to_string())?;
    println!(
        "imported {} functions ({} lv4 starts, {} rows of the old sheet no longer in the workspace and dropped); {} conflicts -> {}",
        rows.len(),
        g.lv4.len(),
        dropped,
        conflicts.len(),
        cpath.display()
    );
    for l in lost {
        println!("demoted: {l}");
    }
    Ok(())
}

fn cmd_scan(args: &[String]) -> Result<(), String> {
    let demote = args.iter().any(|a| a == "--demote");
    let (mut sheet, mut rows) = load_sheet()?;
    let c = lg::scan_tree(&super::root(), &lg::index(&rows));
    let before: HashMap<Key, String> = rows.iter().map(|r| (r.key(), r.status.clone())).collect();
    let (changed, lost) = apply_scan(&mut rows, &c, demote);
    for l in &lost {
        println!("{}: {l}", if demote { "demoted" } else { "ERROR" });
    }
    if !lost.is_empty() && !demote {
        return Err(format!("{} ported/verified/tested rows have no code reference any more; rerun with --demote to accept", lost.len()));
    }
    save(&mut sheet, &rows)?;
    let back = rows.iter().filter(|r| r.status == "unread" && before.get(&r.key()).is_some_and(|s| s == "read")).count();
    println!("scan: {changed} rows changed, {back} went from read back to unread");
    Ok(())
}

#[derive(Default, Clone, Copy)]
struct Tally {
    /// Functions and bytes per status: unread, read, ported, tested, verified, not needed.
    n: [usize; 6],
    b: [u64; 6],
    code: usize,
}

impl Tally {
    fn add(&mut self, r: &Row, code_cited: bool) {
        let i = match r.status.as_str() {
            "unread" => 0,
            "read" => 1,
            "ported" => 2,
            "tested" => 3,
            "verified" => 4,
            _ => 5,
        };
        self.n[i] += 1;
        self.b[i] += r.size as u64;
        if code_cited && i < 2 {
            self.code += 1;
        }
    }
    fn plus(&mut self, o: &Tally) {
        for i in 0..6 {
            self.n[i] += o.n[i];
            self.b[i] += o.b[i];
        }
        self.code += o.code;
    }
    fn line(&self, name: &str) -> String {
        let fns: usize = self.n.iter().sum();
        let bytes: u64 = self.b.iter().sum();
        let kb = |b: u64| b as f64 / 1024.0;
        let cell = |i: usize| format!("{:>5} {:>6.1}", self.n[i], kb(self.b[i]));
        let pct = |b: u64| if bytes > 0 { b as f64 * 100.0 / bytes as f64 } else { 0.0 };
        format!(
            "{name:<18} {fns:>6} {:>7.1} | {} | {} | {} | {} | {} | {} | {:>5} | {:>5.1}% | {:>5.1}%",
            kb(bytes),
            cell(0),
            cell(1),
            cell(2),
            cell(3),
            cell(4),
            cell(5),
            self.code,
            pct(self.b[4] + self.b[5]),
            pct(self.b[3])
        )
    }
}

pub fn report(rows: &[Row], c: &Cites, only: Option<&str>) -> String {
    let mut by: BTreeMap<&str, Tally> = BTreeMap::new();
    for r in rows {
        if let Some(o) = only
            && !(r.system == o || r.system.starts_with(&format!("{o}.")))
        {
            continue;
        }
        by.entry(r.system.as_str()).or_default().add(r, c.code.contains_key(&r.key()));
    }
    let step = |s: &str| SYSTEMS.iter().find(|x| x.0 == s).map(|x| x.1).unwrap_or("-");
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<18} {:>6} {:>7} | {:^12} | {:^12} | {:^12} | {:^12} | {:^12} | {:^12} | {:>5} | {:>6} | {:>6}",
        "system", "fns", "KB", "unread", "read", "ported", "tested", "verified", "not-needed", "code", "done", "tested"
    );
    let _ = writeln!(
        out,
        "{:<18} {:>6} {:>7} | {:^12} | {:^12} | {:^12} | {:^12} | {:^12} | {:^12} | {:>5} | {:>6} | {:>6}",
        "", "", "", "fns KB", "fns KB", "fns KB", "fns KB", "fns KB", "fns KB", "", "bytes", "bytes"
    );
    let mut all = Tally::default();
    for st in STEPS {
        let mut sum = Tally::default();
        let mut lines = Vec::new();
        for (s, t) in by.iter().filter(|(s, _)| step(s) == *st) {
            lines.push(t.line(&format!("  {s}")));
            sum.plus(t);
        }
        if lines.is_empty() {
            continue;
        }
        let _ = writeln!(out, "{}", sum.line(&format!("step {st}")));
        if only.is_some() || by.len() > 1 {
            for l in lines {
                let _ = writeln!(out, "{l}");
            }
        }
        all.plus(&sum);
    }
    let _ = writeln!(out, "{}", all.line("all"));
    let _ = writeln!(out, "done = share of bytes verified (an oracle test ran the original) or not needed; tested = share of bytes with a hand-written test only, shown apart");
    let _ = writeln!(out, "code = functions cited in Rust code that are still unread or read (ports without a `/// orig` line yet)");
    out
}

fn cmd_report(args: &[String]) -> Result<(), String> {
    let only = args.iter().position(|a| a == "--system").and_then(|i| args.get(i + 1)).map(String::as_str);
    let (_, rows) = load_sheet()?;
    let c = lg::scan_tree(&super::root(), &lg::index(&rows));
    print!("{}", report(&rows, &c, only));
    Ok(())
}

/// Clusters of a system: runs of its functions in address order within one image, broken by any function of another
/// system. Returns (first address, members).
fn clusters<'a>(rows: &'a [Row], system: &str) -> Vec<(Key, Vec<&'a Row>)> {
    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort_by_key(|r| r.key());
    let mut out: Vec<(Key, Vec<&Row>)> = Vec::new();
    let mut open = false;
    for r in sorted {
        if r.system == system {
            if !open || out.last().is_some_and(|c| c.0.0 != r.image) {
                out.push((r.key(), Vec::new()));
            }
            out.last_mut().unwrap().1.push(r);
            open = true;
        } else {
            open = false;
        }
    }
    out
}

fn short(set: Option<&BTreeSet<String>>, n: usize) -> String {
    let Some(s) = set else { return "-".into() };
    let mut v: Vec<String> = s.iter().take(n).map(|p| p.rsplit('/').next().unwrap_or(p).to_string()).collect();
    if s.len() > n {
        v.push(format!("+{}", s.len() - n));
    }
    v.join(" ")
}

pub struct BriefIn<'a> {
    pub rows: &'a [Row],
    pub graph: &'a Graph,
    pub cites: &'a Cites,
    /// Factory class and argument summary per function (`A leaf a0 a1 f12`), when the executable is at hand.
    pub class: &'a dyn Fn(&Row) -> Option<String>,
}

pub fn brief(input: &BriefIn, system: &str, cluster: Option<&str>) -> Result<String, String> {
    const LIMIT: usize = 3900;
    let by_key: HashMap<Key, &Row> = input.rows.iter().map(|r| (r.key(), r)).collect();
    let all = clusters(input.rows, system);
    if all.is_empty() {
        return Err(format!("no function has system {system}"));
    }
    let unread = |c: &[&Row]| c.iter().filter(|r| r.status == "unread").count();
    let mut out = String::new();
    let Some(sel) = cluster else {
        let mut v: Vec<&(Key, Vec<&Row>)> = all.iter().filter(|c| unread(&c.1) > 0).collect();
        v.sort_by_key(|c| std::cmp::Reverse(c.1.iter().filter(|r| r.status == "unread").map(|r| r.size).sum::<u32>()));
        let _ = writeln!(out, "system {system}: {} clusters, {} with unread functions (largest first)", all.len(), v.len());
        for (i, (k, m)) in v.iter().enumerate() {
            let ub: u32 = m.iter().filter(|r| r.status == "unread").map(|r| r.size).sum();
            let last = m.last().unwrap();
            let line = format!(
                "  {} {}..{}: {} functions, {} unread ({} bytes)\n",
                IMAGES[k.0 as usize],
                fmt_addr(k.1),
                fmt_addr(last.addr + last.size),
                m.len(),
                unread(m),
                ub
            );
            if out.len() + line.len() > LIMIT {
                let _ = writeln!(out, "  ... {} more clusters", v.len() - i);
                break;
            }
            out.push_str(&line);
        }
        let _ = writeln!(out, "next: kh2 ledger brief {system} <cluster address>");
        return Ok(out);
    };
    let (lo, hi) = match sel.split_once('-') {
        Some((a, b)) => (lg::parse_addr(a).ok_or("bad range")?, lg::parse_addr(b).ok_or("bad range")?),
        None => {
            let a = lg::parse_addr(sel).ok_or("bad cluster address")?;
            let c = all
                .iter()
                .find(|c| c.1.iter().any(|r| r.addr == a) || c.0.1 == a)
                .ok_or_else(|| format!("no cluster of {system} holds {sel}"))?;
            let last = c.1.last().unwrap();
            (c.0.1, last.addr + last.size)
        }
    };
    let fns: Vec<&Row> =
        all.iter().flat_map(|c| c.1.iter().copied()).filter(|r| r.addr >= lo && r.addr < hi && r.status == "unread").collect();
    let bytes: u32 = fns.iter().map(|r| r.size).sum();
    let _ = writeln!(out, "WP brief skeleton: system {system}, cluster {}..{}: {} unread functions, {bytes} bytes", fmt_addr(lo), fmt_addr(hi), fns.len());
    let _ = writeln!(out, "Per function: addr size class | callers (system/status) | callees | cited in");
    let tag = |k: &Key| {
        by_key.get(k).map(|r| {
            let st = match r.status.as_str() {
                "unread" => "u",
                "read" => "r",
                "ported" => "p",
                "tested" => "t",
                "verified" => "v",
                _ => "n",
            };
            let sys = if r.system == system { String::new() } else { format!("{}/", r.system) };
            format!("{}({sys}{st})", fmt_addr(r.addr))
        })
    };
    let list = |s: Option<&BTreeSet<Key>>, n: usize| {
        let Some(s) = s else { return "-".to_string() };
        let mut v: Vec<String> = s.iter().take(n).filter_map(tag).collect();
        if s.len() > n {
            v.push(format!("+{}", s.len() - n));
        }
        if v.is_empty() { "-".into() } else { v.join(" ") }
    };
    let mut files: BTreeMap<String, usize> = BTreeMap::new();
    for (shown, r) in fns.iter().enumerate() {
        let k = r.key();
        for n in input.graph.callers.get(&k).into_iter().flatten().chain(input.graph.callees.get(&k).into_iter().flatten()).chain([&k]) {
            for f in input.cites.code.get(n).into_iter().flatten() {
                if by_key.get(n).is_some_and(|x| x.system == system) {
                    *files.entry(f.clone()).or_default() += 1;
                }
            }
        }
        let cited: BTreeSet<String> =
            input.cites.docs.get(&k).into_iter().flatten().chain(input.cites.code.get(&k).into_iter().flatten()).cloned().collect();
        let line = format!(
            "- {} {}B {} | {} | {} | {}\n",
            fmt_addr(r.addr),
            r.size,
            (input.class)(r).unwrap_or_else(|| "-".into()),
            list(input.graph.callers.get(&k), 4),
            list(input.graph.callees.get(&k), 4),
            short(if cited.is_empty() { None } else { Some(&cited) }, 2)
        );
        if out.len() + line.len() > LIMIT - 500 {
            let _ = writeln!(out, "... {} more: narrow the range (kh2 ledger brief {system} <lo>-<hi>)", fns.len() - shown);
            break;
        }
        out.push_str(&line);
    }
    let mut owners: Vec<(String, usize)> = files.into_iter().collect();
    owners.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let krate = owners
        .iter()
        .find_map(|(f, _)| f.strip_prefix("crates/").and_then(|r| r.split('/').next()).map(String::from))
        .unwrap_or_else(|| "<crate>".into());
    let test = format!("crates/{krate}/tests/orig_{}_{:x}.rs", system.replace('.', "_"), lo);
    let globs: Vec<String> = owners.iter().take(5).map(|(f, _)| f.clone()).chain([test, "sheets/functions.json".into()]).collect();
    let _ = writeln!(out, "Ownership (suggested): {}", globs.join(", "));
    let _ = writeln!(out, "Per function: `cargo run -p kh2-factory -- brief <addr>`, port with `/// orig <addr>`, `factory gen` into the test file.");
    Ok(out)
}

fn cmd_brief(args: &[String]) -> Result<(), String> {
    let spec = args.iter().any(|a| a == "--spec");
    let pos: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let system = pos.first().ok_or("ledger brief <system> [cluster|lo-hi] [--spec]")?;
    let (_, rows) = load_sheet()?;
    let g = load_graph(&ps2_root())?;
    let c = lg::scan_tree(&super::root(), &lg::index(&rows));
    let image = kh2_factory::exe().and_then(|e| kh2_factory::Image::from_elf(&e));
    let probe = std::cell::RefCell::new(kh2_factory::scan::Probe::new());
    let scan = |r: &Row| {
        let img = image.as_ref().filter(|_| r.image == 0)?;
        Some(kh2_factory::scan::scan(r.addr, r.size, &|pc| img.word(pc), &mut probe.borrow_mut()))
    };
    let class = |r: &Row| {
        let s = scan(r)?;
        let args: Vec<String> = (0..8).filter(|i| s.int_args & (1 << i) != 0).map(|i| ["a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3"][i].to_string()).collect();
        let fl: Vec<String> = (0..8).filter(|i| s.float_args & (1 << i) != 0).map(|i| format!("f{}", 12 + i)).collect();
        Some(format!("{}{} {}", s.class(), if s.leaf() { " leaf" } else { "" }, [args, fl].concat().join(" ")).trim_end().to_string())
    };
    let input = BriefIn { rows: &rows, graph: &g, cites: &c, class: &class };
    let out = brief(&input, system, pos.get(1).map(|s| s.as_str()))?;
    print!("{out}");
    if spec {
        println!("\nInferred factory specs: run `cargo run -p kh2-factory -- brief <addr>` per function (needs KH2_EXE or the disc).");
    }
    Ok(())
}

/// The verdict of [`merge_check`].
#[derive(Debug, Default, PartialEq)]
pub struct Verdict {
    /// Why the package needs a reviewer (empty: it qualifies, given its tests pass with the disc).
    pub reasons: Vec<String>,
    /// (package, test target) of every oracle test file in the diff.
    pub tests: Vec<(String, String)>,
}

const WIRING: &[&str] = &[
    "add_systems",
    "add_plugins",
    "add_event",
    "add_message",
    "init_resource",
    "insert_resource",
    "impl Plugin",
    "configure_sets",
    "#[derive(Component",
    "#[derive(Resource",
    "unsafe",
];

/// Decides from `git diff --name-status` and `git diff -U0` output whether a package is factory-only (FAC 2.1 merge
/// rule): only ports with `/// orig` lines, generated oracle tests (`factory gen` header, at least 1,000 cases, no
/// extra early return) for exactly those addresses, ledger rows, and the dev-dependency on kh2-factory.
pub fn merge_check(name_status: &str, diff: &str) -> Verdict {
    let mut v = Verdict::default();
    let mut ported: BTreeSet<u32> = BTreeSet::new();
    let mut tested: BTreeSet<u32> = BTreeSet::new();
    for l in name_status.lines().filter(|l| !l.trim().is_empty()) {
        let mut it = l.split_whitespace();
        let (st, path) = (it.next().unwrap_or(""), it.last().unwrap_or(""));
        if st.starts_with('D') {
            v.reasons.push(format!("{path}: deleted"));
        }
        if st.starts_with('R') {
            v.reasons.push(format!("{path}: renamed"));
        }
    }
    // Split the diff per file.
    let mut files: Vec<(String, Vec<&str>)> = Vec::new();
    for l in diff.lines() {
        if let Some(p) = l.strip_prefix("+++ b/") {
            files.push((p.to_string(), Vec::new()));
        } else if l.starts_with("+++ ") || l.starts_with("--- ") || l.starts_with("diff --git") || l.starts_with("index ") {
            continue;
        } else if let Some(f) = files.last_mut() {
            f.1.push(l);
        }
    }
    for (path, lines) in &files {
        let added: Vec<&str> = lines.iter().filter_map(|l| l.strip_prefix('+')).collect();
        let removed = lines.iter().filter(|l| l.starts_with('-')).count();
        let file = path.rsplit('/').next().unwrap_or(path);
        if path == "sheets/functions.json" {
            continue;
        }
        if path == "Cargo.lock" {
            if removed > 0 || added.iter().any(|a| a.trim() != "\"kh2-factory\",") {
                v.reasons.push("Cargo.lock: changes other than the kh2-factory dependency".into());
            }
            continue;
        }
        if file == "Cargo.toml" {
            if removed > 0 || added.iter().any(|a| !(a.trim().is_empty() || a.trim() == "[dev-dependencies]" || a.trim_start().starts_with("kh2-factory"))) {
                v.reasons.push(format!("{path}: changes other than the kh2-factory dev-dependency"));
            }
            continue;
        }
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() < 4 || parts[0] != "crates" || !path.ends_with(".rs") {
            v.reasons.push(format!("{path}: not a port, oracle test or ledger file"));
            continue;
        }
        if parts[2] == "tests" && file.starts_with("orig_") {
            v.tests.push((parts[1].to_string(), file.trim_end_matches(".rs").to_string()));
            if removed > 0 {
                v.reasons.push(format!("{path}: lines removed from a generated test"));
            }
            let text = added.join("\n");
            if !text.contains("(generated by `factory gen`") {
                v.reasons.push(format!("{path}: no `factory gen` header"));
            }
            let mut fuzzes = 0;
            for (i, _) in text.match_indices("fuzz(&") {
                fuzzes += 1;
                let rest = &text[i..];
                let n: Option<u64> = rest.split(',').nth(1).and_then(|x| x.trim().replace('_', "").parse().ok());
                if n.is_none_or(|n| n < 1000) {
                    v.reasons.push(format!("{path}: a fuzz call with fewer than 1,000 cases"));
                }
            }
            if fuzzes == 0 {
                v.reasons.push(format!("{path}: no fuzz call"));
            }
            let returns = text.matches("return").count();
            let allowed = text.matches("else { return }").count();
            if returns > allowed {
                v.reasons.push(format!("{path}: an early return beyond the generated `else {{ return }}`"));
            }
            for a in added.iter() {
                if let Some(r) = a.trim_start().strip_prefix("fn orig_")
                    && let Some(h) = r.split("_matches_the_game").next()
                    && let Ok(x) = u32::from_str_radix(h, 16)
                {
                    tested.insert(x);
                }
            }
            continue;
        }
        if parts[2] != "src" {
            v.reasons.push(format!("{path}: not a port, oracle test or ledger file"));
            continue;
        }
        if ["plugins.rs", "main.rs", "schedule.rs", "build.rs"].contains(&file) {
            v.reasons.push(format!("{path}: wiring file"));
            continue;
        }
        if removed > 0 {
            v.reasons.push(format!("{path}: {removed} lines removed or changed (a factory package only adds)"));
        }
        if file == "lib.rs" || file == "mod.rs" {
            if added.iter().any(|a| {
                let t = a.trim();
                !(t.is_empty() || ((t.starts_with("pub mod ") || t.starts_with("mod ")) && t.ends_with(';')) || t.starts_with("//"))
            }) {
                v.reasons.push(format!("{path}: changes other than module lines"));
            }
            continue;
        }
        for a in &added {
            if let Some(w) = WIRING.iter().find(|w| a.contains(*w)) {
                v.reasons.push(format!("{path}: wiring or unsafe code ({w})"));
            }
        }
        // Every added function carries an `/// orig` line in the doc block right above it.
        let mut doc_orig = false;
        for l in lines {
            let Some(a) = l.strip_prefix('+') else {
                doc_orig = false;
                continue;
            };
            let t = a.trim_start();
            let is_fn = t.starts_with("fn ") || t.starts_with("pub fn ") || t.starts_with("pub(crate) fn ");
            if let Some((_, x)) = lg::orig_line(t) {
                ported.insert(x);
                doc_orig = true;
            } else if is_fn {
                if !doc_orig {
                    v.reasons.push(format!("{path}: function without an `/// orig` line: {}", t.trim_end_matches('{').trim()));
                }
                doc_orig = false;
            } else if !(t.is_empty() || t.starts_with("///") || t.starts_with("#[")) {
                doc_orig = false;
            }
        }
    }
    for a in ported.difference(&tested) {
        v.reasons.push(format!("{}: ported, but no generated oracle test in the diff", fmt_addr(*a)));
    }
    for a in tested.difference(&ported) {
        v.reasons.push(format!("{}: oracle test, but no `/// orig` port in the diff", fmt_addr(*a)));
    }
    if v.tests.is_empty() {
        v.reasons.push("no generated oracle test in the diff".into());
    }
    v
}

fn git(args: &[&str]) -> Result<String, String> {
    let o = std::process::Command::new("git").current_dir(super::root()).args(args).output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&o.stderr)));
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

fn cmd_merge_check(args: &[String]) -> Result<(), String> {
    let run = args.iter().any(|a| a == "--run");
    let range = args.iter().find(|a| !a.starts_with("--")).ok_or("ledger merge-check <git diff range, e.g. integration...wp/X> [--run]")?;
    let ns = git(&["diff", "--name-status", range])?;
    let diff = git(&["diff", "-U0", "--no-color", range])?;
    let mut v = merge_check(&ns, &diff);
    for (pkg, t) in &v.tests {
        let cmd = format!("KH2_REQUIRE_DISC=1 cargo test -p {pkg} --test {t}");
        if run {
            let st = std::process::Command::new("cargo")
                .current_dir(super::root())
                .env("KH2_REQUIRE_DISC", "1")
                .args(["test", "-p", pkg, "--test", t])
                .status()
                .map_err(|e| e.to_string())?;
            println!("{cmd}: {}", if st.success() { "green" } else { "FAILED" });
            if !st.success() {
                v.reasons.push(format!("{t}: failed with the disc required"));
            }
        } else {
            println!("must pass: {cmd}");
        }
    }
    for r in &v.reasons {
        println!("reason: {r}");
    }
    if v.reasons.is_empty() {
        println!("MERGE-CHECK: QUALIFIES{} (factory ports with oracle tests only; no reviewer needed)", if run { "" } else { " if the tests above pass" });
        Ok(())
    } else {
        println!("MERGE-CHECK: REVIEWER NEEDED ({} reasons)", v.reasons.len());
        Err("not a factory-only package".into())
    }
}

pub fn run(args: &mut Vec<String>) -> Result<(), String> {
    let sub = if args.is_empty() { String::new() } else { args.remove(0) };
    match sub.as_str() {
        "import" => cmd_import(args),
        "scan" => cmd_scan(args),
        "report" => cmd_report(args),
        "brief" => cmd_brief(args),
        "merge-check" => cmd_merge_check(args),
        _ => Err("ledger import [--reseed] | scan [--demote] | report [--system s] | brief <system> [cluster|lo-hi] | merge-check <range> [--run]".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(addr: u32, system: &str) -> Row {
        Row { addr, image: 0, size: 16, system: system.into(), status: "unread".into(), evidence: String::new(), note: String::new() }
    }

    #[test]
    fn propagation_follows_callers_shares_helpers_and_fills_neighbours() {
        // 0x100 (magic) calls 0x110; 0x110 calls 0x120; 0x200 (party) and 0x100 both call 0x300 (a leaf);
        // 0x400 (no caller) calls 0x200 only; 0x500 sits between two magic functions.
        let mut rows = vec![
            row(0x100, "magic"),
            row(0x110, ""),
            row(0x120, ""),
            row(0x200, "party"),
            row(0x300, ""),
            row(0x400, ""),
            row(0x480, "magic"),
            row(0x500, ""),
            row(0x520, "magic"),
            row(0x600, ""),
        ];
        let mut g = Graph::default();
        for (a, b) in [(0x100, 0x110), (0x110, 0x120), (0x100, 0x300), (0x200, 0x300), (0x400, 0x200)] {
            g.link((0, a), (0, b));
        }
        let conflicts = seed(&mut rows, &g, &Cites::default());
        assert!(conflicts.is_empty());
        let sys: Vec<&str> = rows.iter().map(|r| r.system.as_str()).collect();
        assert_eq!(sys, ["magic", "magic", "magic", "party", "core", "party", "magic", "magic", "magic", "unsorted"]);
    }

    #[test]
    fn seeding_prefers_docs_then_code_and_marks_the_sdk() {
        let mut rows = vec![row(0x1a0000, ""), row(0x1b0000, ""), row(0x300000, "")];
        let mut c = Cites::default();
        c.docs.entry((0, 0x1a0000)).or_default().insert("docs/magic.md".into());
        c.code.entry((0, 0x1a0000)).or_default().insert("crates/kh2-app/src/party.rs".into());
        c.code.entry((0, 0x1b0000)).or_default().insert("crates/kh2-app/src/summon.rs".into());
        let conflicts = seed(&mut rows, &Graph::default(), &c);
        assert_eq!(rows[0].system, "magic");
        assert_eq!(rows[1].system, "summon");
        assert_eq!((rows[2].system.as_str(), rows[2].status.as_str()), ("sdk", "not-needed:sdk"));
        assert_eq!(conflicts, vec!["0x1a0000: docs say magic, code says party; chose magic".to_string()]);
    }

    #[test]
    fn report_counts_bytes_per_status_and_step() {
        let mut rows = vec![row(0x100, "magic"), row(0x200, "magic"), row(0x300, "sdk"), row(0x400, "magic")];
        rows[1].status = "verified".into();
        rows[2].status = "not-needed:sdk".into();
        rows[2].size = 32;
        rows[3].status = "tested".into();
        let r = report(&rows, &Cites::default(), None);
        let all = r.lines().find(|l| l.starts_with("all")).unwrap();
        assert!(all.contains("     4     0.1"), "{all}");
        // 16 + 32 of 80 bytes are verified or not needed, 16 are tested only: shown apart.
        assert!(all.trim_end().ends_with("|  60.0% |  20.0%"), "{all}");
        assert!(r.lines().any(|l| l.starts_with("step L4") && l.trim_end().ends_with("|  33.3% |  33.3%")), "{r}");
    }

    #[test]
    fn brief_lists_unread_functions_of_a_cluster_with_callers_and_owners() {
        let rows = vec![row(0x100, "magic"), row(0x110, "magic"), row(0x200, "party"), row(0x300, "magic")];
        let mut g = Graph::default();
        g.link((0, 0x200), (0, 0x110));
        let mut c = Cites::default();
        c.code.entry((0, 0x100)).or_default().insert("crates/kh2-app/src/magic.rs".into());
        let class = |_: &Row| Some("A leaf a0".to_string());
        let input = BriefIn { rows: &rows, graph: &g, cites: &c, class: &class };
        let list = brief(&input, "magic", None).unwrap();
        assert!(list.contains("2 clusters, 2 with unread"), "{list}");
        let b = brief(&input, "magic", Some("0x110")).unwrap();
        assert!(b.contains("cluster 0x100..0x120: 2 unread functions, 32 bytes"), "{b}");
        assert!(b.contains("- 0x110 16B A leaf a0 | 0x200(party/u) | - | -"), "{b}");
        assert!(b.contains("Ownership (suggested): crates/kh2-app/src/magic.rs, crates/kh2-app/tests/orig_magic_100.rs, sheets/functions.json"), "{b}");
        assert!(b.len() < 4096);
    }

    #[test]
    fn merge_check_accepts_only_factory_packages() {
        let ns = "A\tcrates/kh2-fx/tests/orig_fx_1e0000.rs\nM\tcrates/kh2-fx/src/clock.rs\nM\tsheets/functions.json\n";
        let good = "diff --git a/crates/kh2-fx/src/clock.rs b/crates/kh2-fx/src/clock.rs\n--- a/crates/kh2-fx/src/clock.rs\n+++ b/crates/kh2-fx/src/clock.rs\n@@ -10,0 +11,4 @@\n+/// orig 0x1e0000: the tick.\n+pub fn tick(x: u32) -> u32 {\n+    x + 1\n+}\n\
                    diff --git a/crates/kh2-fx/tests/orig_fx_1e0000.rs b/crates/kh2-fx/tests/orig_fx_1e0000.rs\n--- /dev/null\n+++ b/crates/kh2-fx/tests/orig_fx_1e0000.rs\n@@ -0,0 +1,5 @@\n+//! Oracle fuzz test for the original function at 0x1e0000 (generated by `factory gen`; the spec is the static\n+#[test]\n+fn orig_1e0000_matches_the_game() {\n+    let Some(st) = fuzz(&SPEC, 4000, |c| Out::int(tick(c.int(0) as u32) as i64)) else { return };\n+}\n";
        let v = merge_check(ns, good);
        assert_eq!(v.reasons, Vec::<String>::new());
        assert_eq!(v.tests, vec![("kh2-fx".to_string(), "orig_fx_1e0000".to_string())]);
        // A helper without an orig line, a wiring call and too few cases each need the reviewer.
        let bad = good
            .replace("+/// orig 0x1e0000: the tick.\n", "+/// The tick.\n")
            .replace("+    x + 1\n", "+    app.add_systems(x);\n")
            .replace("4000", "100");
        let v = merge_check(ns, &bad);
        assert_eq!(v.reasons.len(), 4, "{:?}", v.reasons);
        assert!(v.reasons.iter().any(|r| r.contains("without an `/// orig` line")));
        assert!(v.reasons.iter().any(|r| r.contains("add_systems")));
        assert!(v.reasons.iter().any(|r| r.contains("fewer than 1,000")));
        assert!(v.reasons.iter().any(|r| r.contains("oracle test, but no `/// orig` port")));
        // Any other file needs the reviewer.
        let v = merge_check("M\tdocs/magic.md\n", "+++ b/docs/magic.md\n+text\n");
        assert!(v.reasons.iter().any(|r| r.contains("docs/magic.md: not a port")));
    }
}
