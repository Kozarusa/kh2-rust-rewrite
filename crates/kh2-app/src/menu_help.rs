//! The Help overlay, page 0x25 (docs/systems/ui/menus-pause.md 2.7 and docs/systems/ui/menus-new-help.md 16.3).
//!
//! What the original does (`SLUS_210.05`): a page that has a Help result (-7) calls `FUN_00292088(topic, return page)`:
//! it stores the return page, `FUN_00292b90(topic)` lists the table entries of that topic (the table at 0x382730: per
//! 8 bytes a pointer to the file stem, a story flag id `FUN_001802e0` tests (the tutorial was seen), the topic byte
//! and a flags byte; `-1` lists the unseen ones, a topic lists the seen ones), `FUN_00292de0` loads the first page's
//! file `menu/us/<stem>_<variant>.bar` (the format `%s_%c.bar` at 0x3826d8, the variant `r`, `o` or `s` from
//! `FUN_002928e8`'s story flags 0x8d0, 0x8d1, 0x8db, `s` once the story is past the prologue) and `FUN_002922a0`
//! puts up the title (group 0xf then 0x10, the topic's message from 0x382780) and the page counter (group 0x12 then
//! 0x13, message 0xcbd8 followed by `%d/%d`, the format at 0x3826b8). The handler `FUN_002920d8` waits for the confirm
//! (logical bit 4), plays the confirm sound (`FUN_0023e738(2, 1)`), fades the page out and loads the next one; after the
//! last page it marks the tutorials seen (`FUN_00292ae0`) and returns to the stored page (`FUN_002959d0`). No other key
//! does anything. The page is the single IMGD of the `.bar` (512 x 512, 8 bit), drawn by `FUN_0029e630` and the sprite
//! `FUN_002332e0` at the screen origin; groups 1 and 4 of the layout are the "next page" and "close" prompts
//! (`FUN_00292638`, shown while there is a next page of the same topic or not).
//!
//! The port: every entry of the topic is listed (the app has no tutorial flags: the baseline is a player who has seen
//! them), the variant is `s`. The prompts and the fade of the pages are not drawn [GAP].

use bevy::prelude::*;
use kh2_image::text::Style;

use super::item_pages::{label, tokens_of};
use super::{Menu, Phase};
use crate::assets::Disc;
use crate::audio::PlaySe;
use crate::menu_gfx::{self, MenuScene, Node, Pic};
use crate::menu_pages::Exe;

/// The page id (`FUN_002959d0(0x25, 0)`).
pub const PAGE: u16 = 0x25;
/// The table of tutorial pages: entry size and the position of the first entry (the name pointer is at +0).
const TABLE: u32 = 0x0038_2730;
const ENTRY: u32 = 8;
/// The topics' title messages (`0x382780 + 2 * topic`) and the counter's message and format.
const TITLES: u32 = 0x0038_2780;
const COUNTER_MESSAGE: u32 = 0x4bd8;
/// `camp.2ld` sequence 1: the title (enter, loop) and the counter (enter, loop).
const TITLE_GROUPS: [usize; 2] = [0xf, 0x10];
const COUNTER_GROUPS: [usize; 2] = [0x12, 0x13];
const SEQ_MAIN: usize = 1;
const CONFIRM: u32 = 2002;

/// What a page asks for with -7 (`FUN_00292088(topic, return page)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub topic: u8,
    pub ret: u16,
}

/// One tutorial page of the table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub stem: String,
    pub topic: u8,
    pub flags: u8,
}

/// The table at 0x382730: the entries up to the first whose flag id is 0.
pub fn read_table(exe: &Exe, stem: impl Fn(u32) -> Option<String>) -> Vec<Entry> {
    let mut out = Vec::new();
    for k in 0..16u32 {
        let at = TABLE + ENTRY * k;
        let flag = exe.u16(at + 4).unwrap_or(0);
        if flag == 0 {
            break;
        }
        let ptr = u32::from(exe.u16(at).unwrap_or(0)) | u32::from(exe.u16(at + 2).unwrap_or(0)) << 16;
        let (Some(stem), Some(topic), Some(flags)) = (stem(ptr), exe.u8(at + 6), exe.u8(at + 7)) else { break };
        out.push(Entry { stem, topic, flags });
    }
    out
}

/// `FUN_00292b90`: the pages of a topic in table order.
pub fn pages_of(table: &[Entry], topic: u8) -> Vec<usize> {
    table.iter().enumerate().filter(|(_, e)| e.topic == topic).map(|(i, _)| i).collect()
}

/// The file of a page (`%s_%c.bar`; the variant is `s`).
pub fn file_name(stem: &str) -> String {
    format!("menu/us/{stem}_s.bar")
}

/// The picture of a `.bar` page: its first IMGD entry.
pub fn page_image(bar: &[u8]) -> Option<kh2_image::imgd::Image> {
    let entries = kh2_bar::parse(bar).ok()?;
    entries.iter().find_map(|e| kh2_image::imgd::decode(e.data))
}

/// The state while the overlay is up.
struct Active {
    ret: u16,
    pages: Vec<usize>,
    at: usize,
    topic: u8,
}

#[derive(Resource, Default)]
pub struct Help {
    ready: bool,
    table: Vec<Entry>,
    /// The topics' title messages (`u16` at 0x382780 + 2 * topic, bit 15 ignored).
    titles: Vec<u32>,
    /// Texture index per table entry (loaded on first use).
    loaded: Vec<Option<Option<usize>>>,
    nodes: Option<Nodes>,
    active: Option<Active>,
}

struct Nodes {
    image: usize,
    title: usize,
    counter: usize,
}

pub struct HelpPlugin;

impl Plugin for HelpPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Help>().add_systems(Startup, load).add_systems(
            PreUpdate,
            input.before(super::item_pages::input).after(bevy::input::InputSystems).after(crate::harness::Keys),
        );
    }
}

fn load(disc: Res<Disc>, mut help: ResMut<Help>) {
    let Some(elf) = disc.read_root("SLUS_210.05") else { return };
    let Some(exe) = Exe::load(elf.clone()) else { return };
    // The name pointers point at NUL-terminated strings in the executable's data.
    let cstr = |ptr: u32| -> Option<String> {
        let bytes: Vec<u8> = (0..16u32).map_while(|k| exe.u8(ptr + k).filter(|&b| b != 0)).collect();
        (!bytes.is_empty()).then(|| String::from_utf8_lossy(&bytes).into_owned())
    };
    help.table = read_table(&exe, cstr);
    help.titles = (0..4).filter_map(|t| exe.u16(TITLES + 2 * t).map(|v| u32::from(v & 0x7fff))).collect();
    help.loaded = vec![None; help.table.len()];
    help.ready = !help.table.is_empty();
    if !help.ready {
        warn!("menu: the Help table is not readable; the Help overlay is off");
    }
}

#[allow(clippy::too_many_arguments)]
fn input(
    mut help: ResMut<Help>,
    mut menu: ResMut<Menu>,
    mut scene: ResMut<MenuScene>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut se: MessageWriter<PlaySe>,
    disc: Res<Disc>,
    mut images: ResMut<Assets<Image>>,
) {
    if !help.ready {
        return;
    }
    // The menu closed behind the overlay.
    if help.active.is_some() && matches!(menu.phase, Phase::Closing | Phase::Closed) {
        finish_nodes(&help, &mut scene);
        help.active = None;
        menu.help = None;
        return;
    }
    if help.active.is_none() {
        let Some(req) = menu.help.take() else { return };
        let pages = pages_of(&help.table, req.topic);
        if pages.is_empty() {
            return;
        }
        info!("menu: help topic {} ({} pages), return to page {:#x}", req.topic, pages.len(), req.ret);
        help.active = Some(Active { ret: req.ret, pages, at: 0, topic: req.topic });
        menu.page = PAGE;
        ensure_nodes(&mut help, &mut scene);
        show_page(&mut help, &mut scene, &disc, &mut images);
        keys.reset_all();
        return;
    }
    let confirm = keys.just_pressed(KeyCode::KeyJ);
    if confirm {
        crate::menu::menu_sound::play(&mut se, CONFIRM);
        let Some(a) = help.active.as_mut() else { return };
        a.at += 1;
        if a.at < a.pages.len() {
            show_page(&mut help, &mut scene, &disc, &mut images);
        } else {
            // After the last page: back to the page that asked (`FUN_002959d0(*DAT_0036d1a8, 0)`).
            let ret = a.ret;
            finish_nodes(&help, &mut scene);
            help.active = None;
            menu.page = ret;
            info!("menu: help done, page {ret:#x}");
        }
    }
    keys.reset_all();
}

fn ensure_nodes(help: &mut Help, scene: &mut MenuScene) {
    if help.nodes.is_some() {
        return;
    }
    let mut mk = |seq: Option<usize>| {
        let mut n = Node::new(seq);
        n.visible = false;
        scene.add(n)
    };
    let image = mk(None);
    let title = mk(Some(SEQ_MAIN));
    let counter = mk(Some(SEQ_MAIN));
    help.nodes = Some(Nodes { image, title, counter });
}

fn finish_nodes(help: &Help, scene: &mut MenuScene) {
    if let Some(n) = &help.nodes {
        for id in [n.image, n.title, n.counter] {
            scene.nodes[id].visible = false;
        }
    }
}

/// Loads (once) and shows the current page of the active topic with its title and counter.
fn show_page(help: &mut Help, scene: &mut MenuScene, disc: &Disc, images: &mut Assets<Image>) {
    let Some(a) = help.active.as_ref() else { return };
    let (entry, count, at, topic) = (a.pages[a.at], a.pages.len(), a.at, a.topic);
    if help.loaded[entry].is_none() {
        let tex = disc.read(&file_name(&help.table[entry].stem)).and_then(|b| page_image(&b)).map(|img| {
            scene.texs.push(menu_gfx::Tex::new(&img, images));
            scene.texs.len() - 1
        });
        if tex.is_none() {
            warn!("menu: help page {} is not readable", file_name(&help.table[entry].stem));
        }
        help.loaded[entry] = Some(tex);
    }
    let tex = help.loaded[entry].flatten();
    let Some(n) = help.nodes.as_ref() else { return };
    let (image, title, counter) = (n.image, n.title, n.counter);
    let Some(text) = scene.text.take() else { return };
    // The page picture over the whole screen.
    scene.nodes[image].visible = tex.is_some();
    scene.nodes[image].pic = tex.map(|tex| Pic { tex, rect: [0.0, 0.0, menu_gfx::SCREEN[0], menu_gfx::SCREEN[1]], v_end: menu_gfx::SCREEN[1] / 512.0 });
    // The title: the topic's message (0x382780 + 2 * topic).
    let title_msg = help.titles.get(usize::from(topic)).copied();
    scene.nodes[title].visible = true;
    scene.nodes[title].label_anchor = Some([0.0, 1.0]);
    scene.nodes[title].label = title_msg.map(|m| label(&text, tokens_of(&text, m), 1, 15.0, [0xe5, 0xe5, 0xe5, 0x80], 2, [0.0, 0.0]));
    // The counter: "PAGE" and "n/m".
    let mut tokens = tokens_of(&text, COUNTER_MESSAGE);
    tokens.push(kh2_image::text::Token::Space);
    tokens.extend(text.literal(&format!("{}/{}", at + 1, count), Style::default(), [0.0; 2]).tokens);
    scene.nodes[counter].visible = true;
    scene.nodes[counter].label_anchor = Some([0.0, 1.0]);
    scene.nodes[counter].label = Some(label(&text, tokens, 1, 15.0, [0xe5, 0xe5, 0xe5, 0x80], 2, [0.0, 0.0]));
    scene.text = Some(text);
    scene.set_group_then(title, TITLE_GROUPS[0], TITLE_GROUPS[1]);
    scene.set_group_then(counter, COUNTER_GROUPS[0], COUNTER_GROUPS[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Vec<Entry> {
        let e = |stem: &str, topic| Entry { stem: stem.into(), topic, flags: 0 };
        vec![e("at_010", 0), e("at_020", 0), e("mp_010", 1), e("ab_010", 2), e("ab_020", 2), e("ab_040", 2), e("sw_010", 3)]
    }

    #[test]
    fn a_topic_lists_its_pages_in_table_order() {
        let t = table();
        assert_eq!(pages_of(&t, 2), vec![3, 4, 5]);
        assert_eq!(pages_of(&t, 0), vec![0, 1]);
        assert!(pages_of(&t, 9).is_empty());
    }

    #[test]
    fn the_file_name_is_the_stem_and_the_story_variant() {
        assert_eq!(file_name("ab_010"), "menu/us/ab_010_s.bar");
    }

    /// The executable's table and the page images of the disc (skipped without `KH2_ISO`).
    #[test]
    fn the_disc_has_the_tutorial_pages_the_table_names() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let Some(elf) = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()) else { return };
        let exe = Exe::load(elf).expect("an ELF");
        let cstr = |ptr: u32| -> Option<String> {
            let bytes: Vec<u8> = (0..16u32).map_while(|k| exe.u8(ptr + k).filter(|&b| b != 0)).collect();
            (!bytes.is_empty()).then(|| String::from_utf8_lossy(&bytes).into_owned())
        };
        let t = read_table(&exe, cstr);
        assert_eq!(t.len(), 9, "{t:?}");
        assert_eq!(pages_of(&t, 2).len(), 3);
        for e in &t {
            let bar = a.read(&file_name(&e.stem)).unwrap_or_else(|_| panic!("{}", file_name(&e.stem)));
            let img = page_image(&bar).expect("an IMGD page");
            assert_eq!((img.width, img.height), (512, 512), "{}", e.stem);
        }
    }
}
