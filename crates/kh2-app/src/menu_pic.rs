//! The item pictures of the Items and Equipment pages (docs/menus.md 16.4).
//!
//! What the original does (`SLUS_210.05`): the item window of a list page (`FUN_0029ed50`, built from
//! `FUN_0026b1c8` on pages 2 and 5 to 9 and `FUN_0025bad0` on page 0xb) also shows the picture of the highlighted
//! item. The picture id is the u16 at `+0x14` of the item row (`FUN_00283520(item + 0x14)` stores it in
//! `DAT_0036d160`); `FUN_00283578` waits for the cursor to rest (`DAT_0036d174` counts down from the float at
//! 0x3823c4) and then asks the picture cache (`FUN_002a07c8`) for it; the cache loads `itempic/item-%03d.imd`
//! (the format string at 0x3829c0, `FUN_002a0ba0` through `FUN_00167d20`, an asynchronous file read) into one of its
//! slots, and `FUN_002830f8` draws the slot's image as a sprite (`FUN_00138948`) scaled to the window group's width
//! (`FUN_00326f10(width, 100.0)`: percent). While the picture is not loaded the window shows its frame only.
//!
//! The port reads the `.imd` at once on first use (the disc read is synchronous) and keeps the texture; the hold-off
//! of the cursor is not modelled (the original's delay is the float at 0x3823c4, [GAP] its value was not read).

use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::assets::Disc;
use crate::menu_gfx::{self, MenuScene, Pic};

/// What the pages ask for: per node, the picture id and the rectangle (relative to the node's origin).
#[derive(Resource, Default)]
pub struct Pictures {
    /// Picture id -> index into `MenuScene::texs` (`None`: the file is missing or unreadable).
    cache: BTreeMap<u16, Option<usize>>,
    /// Node -> (picture id, rectangle); `None` clears the node.
    wanted: BTreeMap<usize, Option<(u16, [f32; 4])>>,
}

impl Pictures {
    /// The page shows `picture` in `node` inside `rect`.
    pub fn show(&mut self, node: usize, picture: u16, rect: [f32; 4]) {
        self.wanted.insert(node, Some((picture, rect)));
    }

    /// The page shows no picture in `node`.
    pub fn clear(&mut self, node: usize) {
        self.wanted.insert(node, None);
    }
}

/// The file name of a picture (`itempic/item-%03d.imd`).
pub fn file_name(picture: u16) -> String {
    format!("itempic/item-{picture:03}.imd")
}

pub struct PicturesPlugin;

impl Plugin for PicturesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Pictures>().add_systems(Update, apply.before(crate::schedule::Frame::Popups));
    }
}

fn apply(mut pics: ResMut<Pictures>, mut scene: ResMut<MenuScene>, disc: Res<Disc>, mut images: ResMut<Assets<Image>>) {
    if pics.wanted.is_empty() {
        return;
    }
    let wanted: Vec<(usize, Option<(u16, [f32; 4])>)> = pics.wanted.iter().map(|(&k, &v)| (k, v)).collect();
    for (node, want) in wanted {
        let Some(n) = scene.nodes.get(node) else { continue };
        match want {
            None => {
                if n.pic.is_some() {
                    scene.nodes[node].pic = None;
                }
            }
            Some((id, rect)) => {
                let tex = match pics.cache.get(&id) {
                    Some(t) => *t,
                    None => {
                        let t = disc.read(&file_name(id)).and_then(|b| kh2_image::imgd::decode(&b)).map(|img| {
                            scene.texs.push(menu_gfx::Tex::new(&img, &mut images));
                            info!("menu: item picture {id} loaded ({}x{})", img.width, img.height);
                            scene.texs.len() - 1
                        });
                        if t.is_none() {
                            warn!("menu: item picture {id} ({}) is not readable", file_name(id));
                        }
                        pics.cache.insert(id, t);
                        t
                    }
                };
                let pic = tex.map(|tex| Pic { tex, rect, v_end: 1.0 });
                if scene.nodes[node].pic != pic {
                    scene.nodes[node].pic = pic;
                }
            }
        }
    }
}
