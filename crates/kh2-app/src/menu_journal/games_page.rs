//! The node glue of the Quests page (9) and the Minigames page (10) of Jiminy's Journal (docs/menus.md 20.10, WP-MN14):
//! `FUN_002b5e50` / `FUN_002b4b18` build the page, `FUN_002b6920` / `FUN_002b5698` fill the ten rows in view, `FUN_002b6c20` /
//! `FUN_002b5aa8` mark them seen. The data and the tests of what a row shows are in `games.rs`.

use super::games::{self, Row as GRow};
use super::*;

/// The page's literal groups of the list sequence: the two layers (`FUN_002b5e50`: 99 and 0x6b, the second holds the scroll bar's
/// track), a world's header row (100), a quest group's header (0x66), a quest row (0x68, 0x69 once done) and a minigame row
/// (0x6a).
pub const LAYER: usize = 0x63;
pub const BAR_LAYER: usize = 0x6b;
pub const WORLD_ROW: usize = 100;
pub const GROUP_ROW: usize = 0x66;
pub const QUEST_ROW: [usize; 2] = [0x68, 0x69];
pub const GAME_ROW: usize = 0x6a;
/// The help lines (`FUN_002b6c00`, `FUN_002b5a88`).
pub const HELP_QUEST: u16 = 0x48f6;
pub const HELP_GAME: u16 = 0x48f7;
/// Rows in view (ten list objects).
pub const ROWS: usize = 10;
/// The font of the rows (`+0x1a0 = 3`) and their ink (`+0x198 = 0x60000008`, as page 3's text).
const FONT: usize = 3;
const INK: [u8; 4] = [0x08, 0x00, 0x00, 0x60];
/// The marks `FUN_001918a0` writes for the apostrophe and the double quote of the time format: its table turns them into the bytes
/// 0x57 and 0x61, glyphs 55 and 65.
const APOSTROPHE: u16 = 55;
const QUOTE: u16 = 65;

/// The objects of the two pages: the two layers, and per row the text object, its tag and (minigames) the value text.
#[derive(Default, Clone, Copy)]
pub struct Nodes {
    pub layers: [usize; 2],
    pub rows: [usize; ROWS],
    pub tags: [usize; ROWS],
    pub values: [usize; ROWS],
}

impl Nodes {
    pub fn all(&self) -> Vec<usize> {
        let mut v = self.layers.to_vec();
        v.extend(self.rows);
        v.extend(self.tags);
        v.extend(self.values);
        v
    }
}

/// What the quest and minigame tests read, copied from the save when the journal opens (the minigame records and the record
/// bits the quest records name).
#[derive(Default, Clone)]
pub struct Snap {
    pub bits: std::collections::HashSet<u16>,
    pub minis: std::collections::HashMap<u16, (u8, u32)>,
    /// The moogle's first record word has a bit set (`FUN_002b47c8`), which lists page 0xc's category 9.
    pub moogle: bool,
    /// The defeated-enemy counters by NeoStatus id (`Progress::enemy_defeats_all`) and the reaction commands' use counts by
    /// counter slot (`ReactionUses`), copied when the journal opens.
    pub kills: std::collections::HashMap<u32, u32>,
    pub uses: std::collections::HashMap<u16, u16>,
}

impl Snap {
    /// Reads the bits and records the lists name from `p`.
    pub fn read(p: &Progress, quests: &[games::Quest], minis: &[games::Minigame]) -> Snap {
        let mut s = Snap { moogle: p.moogle.known != 0, kills: p.enemy_defeats_all().collect(), uses: p.reaction_uses_all().collect(), ..Snap::default() };
        for q in quests {
            for id in [q.id, q.done] {
                if p.record_bit(id) {
                    s.bits.insert(id);
                }
            }
            s.minis.insert(q.game, p.minigame(q.game));
        }
        for m in minis {
            s.minis.insert(m.game, p.minigame(m.game));
        }
        s
    }
}

/// Runs `f` with the game the quest tests read (`games::Game`).
pub fn with_game<R>(j: &Journal, test: &dyn Fn(u16) -> bool, f: impl FnOnce(&games::Game) -> R) -> R {
    let bit = |b: u16| j.snap.bits.contains(&b);
    let mini = |g: u16| j.snap.minis.get(&g).copied().unwrap_or((0, 0));
    f(&games::Game { story: test, bit: &bit, mini: &mini, all: test(ALL_WORLDS_FLAG) })
}

/// A label of the rows' kind: font 3, ink, drawn from the object's anchor (`FUN_00139888` cleared the group offset).
pub(super) fn ink(scene: &MenuScene, node: usize, group: usize, tokens: Vec<Token>, align: u8) -> Option<Label> {
    let g = scene.nodes[node].seq.and_then(|s| scene.seqs.get(s)).and_then(|s| s.groups.get(group)).copied().unwrap_or_default();
    let size = if g.size > 0 { g.size as f32 } else { TEXT_SIZE };
    let world = scene.text.as_ref()?.world;
    Some(Label { tokens, style: Style { font: FONT, size: Some(size), align, world, color: INK, ..Style::default() }, offset: [0.0; 2] })
}

/// A message's tokens without the end mark.
fn plain(j: &Journal, scene: &MenuScene, id: u16) -> Vec<Token> {
    tokens(j, scene, id).map(|t| t.into_iter().filter(|t| *t != Token::End).collect()).unwrap_or_default()
}

/// A number the way `FUN_001918a0` writes it: the digits are glyphs 1 to 10.
fn number(n: u32, width: usize) -> Vec<Token> {
    format!("{n:0width$}").bytes().map(|b| Token::Glyph(u16::from(b - b'0') + 1)).collect()
}

/// A minigame row's value (`FUN_002b5698`): the tokens of `games::Value`.
fn value_tokens(j: &Journal, scene: &MenuScene, v: &games::Value) -> Vec<Token> {
    match *v {
        games::Value::NoScore => plain(j, scene, games::MSG_NO_SCORE),
        games::Value::Time { unit, m, s, f } => {
            let mut t = plain(j, scene, unit);
            t.extend(number(m, 2));
            t.push(Token::Glyph(APOSTROPHE));
            t.extend(number(s, 2));
            t.push(Token::Glyph(QUOTE));
            t.extend(number(f, 2));
            t
        }
        games::Value::Score { n, msg, msg_first } => {
            let (mut t, digits) = (plain(j, scene, msg), number(n, 1));
            if msg_first {
                t.extend(digits);
                t
            } else {
                let mut d = digits;
                d.extend(t);
                d
            }
        }
    }
}

/// The text of a world's header row (`FUN_002a3070(world, 0)` and `FUN_002a3120`): its name, or question marks while the world's
/// cell is not open.
fn world_title(j: &Journal, scene: &MenuScene, w: u16, test: &dyn Fn(u16) -> bool) -> Vec<Token> {
    let Some(rec) = j.worlds.iter().find(|r| r.id as i16 as u16 == w) else { return Vec::new() };
    let t = plain(j, scene, rec.title(test));
    if test(rec.flag) { t } else { treasure_page::masked(&t) }
}

/// What a row shows, worked out when the page is built: its group, its text, the key of its new tag and the value text of a
/// minigame row.
#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub group: usize,
    pub label: Vec<Token>,
    pub key: Option<(Cat, usize)>,
    pub value: Option<Vec<Token>>,
}

/// The entries of every row of the list (`FUN_002b6920` / `FUN_002b5698`).
fn entries(j: &Journal, scene: &MenuScene, quests: bool, rows: &[GRow], test: &dyn Fn(u16) -> bool) -> Vec<Entry> {
    rows.iter()
        .map(|&row| match row {
            GRow::World(w) => Entry { group: WORLD_ROW, label: world_title(j, scene, w, test), ..Entry::default() },
            GRow::Group(g) => {
                let t = plain(j, scene, g);
                // The group's name shows while some quest of it is known (`FUN_002b6fb8`: state 1 or 2).
                let shown = with_game(j, test, |game| j.quests.iter().any(|q| q.group == g && matches!(games::quest_state(q, game), games::OPEN | games::DONE)));
                Entry { group: GROUP_ROW, label: if shown { t } else { treasure_page::masked(&t) }, ..Entry::default() }
            }
            GRow::Item(i) if quests => {
                let q = j.quests[i];
                let st = with_game(j, test, |game| games::quest_state(&q, game));
                let t = plain(j, scene, q.name);
                let listed = st == games::LISTED;
                Entry {
                    group: QUEST_ROW[usize::from(st == games::DONE)],
                    label: if listed { treasure_page::masked(&t) } else { t },
                    key: (!listed).then_some((Cat::Quest, i)),
                    value: None,
                }
            }
            GRow::Item(i) => {
                let m = j.minis[i];
                let (state, word) = j.snap.minis.get(&m.game).copied().unwrap_or((0, 0));
                let t = plain(j, scene, m.name);
                Entry {
                    group: GAME_ROW,
                    label: if state < 2 { treasure_page::masked(&t) } else { t },
                    key: (state != 0).then_some((Cat::Minigame, i)),
                    value: Some(value_tokens(j, scene, &games::value(&m, state, word))),
                }
            }
        })
        .collect()
}

/// Builds page 9 or 10.
pub fn build(j: &mut Journal, scene: &mut MenuScene, page: u16, world: i8, test: &dyn Fn(u16) -> bool) {
    let quests = page == 9;
    let from_c = j.open.as_ref().is_some_and(|o| o.from_c);
    category_frame(j, scene, if quests { 5 } else { 6 });
    let list = j.seq_base + SEQ_LIST;
    let n = j.nodes.games;
    for (k, g) in [LAYER, BAR_LAYER].into_iter().enumerate() {
        scene.nodes[n.layers[k]].visible = true;
        scene.nodes[n.layers[k]].off = [0.0; 2];
        scene.set_group(n.layers[k], g);
    }
    let rows = with_game(j, test, |g| if quests { games::quest_rows(&j.quests, g) } else { games::game_rows(&j.minis, g) });
    let list_entries = entries(j, scene, quests, &rows, test);
    let count = games::positions(rows.len());
    let top = if from_c { 0 } else { games::first_row(&rows, world, count) };
    let row_group = if quests { QUEST_ROW[0] } else { GAME_ROW };
    let group = |g: usize| scene.seqs.get(list).and_then(|s| s.groups.get(g)).copied().unwrap_or_default();
    let pitch = group(row_group).unk1c;
    // `FUN_00237618` / `FUN_00237638` read the group's x / y.
    let anchor = [group(row_group).x, group(row_group).y];
    for k in 0..ROWS {
        scene.nodes[n.rows[k]].off = [0.0, (k as i32 * pitch) as f32];
        scene.nodes[n.rows[k]].label_anchor = Some([0.0; 2]);
        // The value text of a minigame row hangs from the row at its group's x / y, right aligned, and the tag from the value
        // (`FUN_002b52c0`); a quest row's tag hangs from the row at the same place (`FUN_002b6650`).
        scene.nodes[n.values[k]].off = [anchor[0] as f32, anchor[1] as f32];
        scene.nodes[n.values[k]].label_anchor = Some([0.0; 2]);
        scene.nodes[n.tags[k]].parent = Some(if quests { n.rows[k] } else { n.values[k] });
        scene.nodes[n.tags[k]].off = if quests { [anchor[0] as f32, anchor[1] as f32] } else { [0.0; 2] };
    }
    // The scroll bar (`FUN_00299878(.., total = max(rows, 10), visible 10, parent = the second layer)`).
    let track = group(BAR_LAYER);
    let body = scene.seqs.get(list).and_then(|s| s.groups.get(j.tables.bar[2])).map_or(1, |g| g.size.max(1));
    let bar = Bar::new(track.size, body, rows.len().max(ROWS) as i32, ROWS as i32);
    info!(
        "journal: page {page:#x} (FUN_002b5e50 / FUN_002b4b18): world {world}, {} rows {:?}, {count} positions, first row {top}, pitch {pitch}, anchor {anchor:?}",
        rows.len(),
        &rows[..rows.len().min(24)]
    );
    let o = j.open.as_mut().unwrap();
    (o.page, o.world, o.top, o.count, o.bar, o.gentries) = (page, world, top, count, Some(bar), list_entries);
    let layer = j.nodes.games.layers[1];
    place_bar(j, scene, layer, BAR_LAYER);
    view(j, scene);
}

/// The ten rows from the first one in view, their tags and the thumb. The rows in view are marked seen first (`FUN_002b6c20`,
/// `FUN_002b5aa8`), so a tag in view reads "seen".
pub fn view(j: &mut Journal, scene: &mut MenuScene) {
    let Some(o) = j.open.as_ref() else { return };
    let (quests, top, shown) = (o.page == 9, o.top, o.gentries.iter().skip(o.top).take(ROWS).cloned().collect::<Vec<_>>());
    let n = j.nodes.games;
    let tags = if quests { j.tables.quest_tag } else { j.tables.game_tag };
    for k in 0..ROWS {
        let e = shown.get(k);
        for id in [n.rows[k], n.tags[k]] {
            scene.nodes[id].visible = e.is_some();
        }
        scene.nodes[n.values[k]].visible = e.is_some_and(|e| e.value.is_some());
        scene.nodes[n.rows[k]].label = None;
        scene.nodes[n.values[k]].label = None;
        let Some(e) = e else { continue };
        scene.set_group(n.rows[k], e.group);
        scene.nodes[n.rows[k]].label = ink(scene, n.rows[k], e.group, e.label.clone(), 0);
        if let Some(v) = &e.value {
            scene.set_group(n.values[k], STORY_ROW);
            scene.nodes[n.values[k]].label = ink(scene, n.values[k], GAME_ROW, v.clone(), 1);
        }
        scene.set_group(n.tags[k], STORY_ROW);
        if let Some((cat, key)) = e.key {
            if j.marks.is_new(cat, key) {
                j.marks.see(cat, key);
            }
            if let Some(t) = j.marks.tag(cat, key) {
                scene.set_group(n.tags[k], tags[usize::from(t == Tag::Seen)]);
            }
        }
    }
    bar_pos(j, scene, top);
}
