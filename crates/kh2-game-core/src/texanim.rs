//! Texture animation at run time: TEXA sprite regions and UVSC UV scrolls of maps and models.
//!
//! Each spawned model whose texture footer holds animations gets a [`TexAnim`] on its root (map,
//! sky) or body (characters). Every 60 Hz tick advances the game's players exactly as
//! FUN_0011e7f8 does with the field delta (1.0 per field): `kh2_model::texanim` holds the ports.
//! When a TEXA's sprite changes, its texture image is rebuilt from the base pixels with every
//! showing sprite copied in (the game re-uploads the base and then the sprites each frame,
//! FUN_0011f560 / FUN_00121a78); UV scrolls move the materials' UV transforms.
//!
//! Debug: `KH2_TEXLOG=1` logs per drawn mesh its texture, size, GS region and animations at load,
//! and every sprite change; `KH2_TEXTINT=<i,j,..>` draws those texture indexes plain magenta;
//! `KH2_TEXA_PIN=<texa>:<sprite>` holds one sprite on screen.

use std::sync::Arc;

use bevy::prelude::*;
use kh2_model::texanim::{Animations, TexaPlayer, UvscPlayer};

use crate::assets::region_affine;

pub struct TexAnimPlugin;

impl Plugin for TexAnimPlugin {
    fn build(&self, app: &mut App) {
        // After the pose step, so this tick's motion frame selects range-triggered animations.
        app.add_systems(FixedUpdate, tick.after(crate::schedule::Sim::Pose).run_if(crate::schedule::sim_running));
    }
}

fn texlog() -> bool {
    std::env::var("KH2_TEXLOG").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// True if `KH2_TEXTINT` names texture index `i`.
pub fn tinted(i: usize) -> bool {
    std::env::var("KH2_TEXTINT").is_ok_and(|v| v.split(',').any(|s| s.trim().parse() == Ok(i)))
}

/// `KH2_TEXA_PIN=<texa>:<sprite>`: show that sprite in every model's TEXA number `texa` (a look check).
fn pinned(texa: usize) -> Option<usize> {
    let v = std::env::var("KH2_TEXA_PIN").ok()?;
    let (a, s) = v.split_once(':')?;
    (a.trim().parse() == Ok(texa)).then(|| s.trim().parse().ok()).flatten()
}

/// `KH2_TEXLOG`: one line per drawn mesh of a model.
pub fn log_model(model: &kh2_model::Model) {
    if !texlog() {
        return;
    }
    for (i, m) in model.meshes.iter().enumerate() {
        let Some(t) = model.textures.get(m.texture) else { continue };
        let texa = model.anims.texa.iter().filter(|a| a.texture == m.texture).count();
        let uvsc = model.anims.uvsc.iter().find(|u| m.uv_scroll != 0 && u.index + 1 == m.uv_scroll as u32);
        info!(
            "texlog mesh {i}: texture {} {}x{} region u {:?} v {:?} texa {texa} uvsc {:?}",
            m.texture,
            t.width,
            t.height,
            t.region[0],
            t.region[1],
            uvsc.map(|u| (u.index, u.speed_u, u.speed_v))
        );
    }
}

/// A texture that TEXA sprites are copied into.
struct Target {
    texture: usize,
    handle: Handle<Image>,
    /// The cropped base image and the crop's origin and size.
    base: Vec<u8>,
    origin: [u32; 2],
    size: [u32; 2],
}

/// A material whose UVs scroll.
struct Scroll {
    material: Handle<StandardMaterial>,
    /// Index into `Animations::uvsc`.
    uvsc: usize,
    texture: kh2_model::Texture,
    span: [u32; 2],
}

#[derive(Component)]
pub struct TexAnim {
    anims: Arc<Animations>,
    texa: Vec<TexaPlayer>,
    shown: Vec<Option<usize>>,
    uvsc: Vec<UvscPlayer>,
    targets: Vec<Target>,
    scrolls: Vec<Scroll>,
    tick: u64,
    /// The TEXA slot a motion range trigger requested last field (for the log).
    slot: Option<u32>,
}

impl TexAnim {
    /// The animation state for a spawned model, or None when it has nothing to animate.
    /// `materials` lists (texture, uv_scroll, material) for every material made for its meshes.
    pub fn new(model: &kh2_model::Model, textures: &[Handle<Image>], materials: &[(usize, u8, Handle<StandardMaterial>)]) -> Option<TexAnim> {
        let anims = &model.anims;
        let scrolls: Vec<Scroll> = materials
            .iter()
            .filter(|(_, s, _)| *s != 0)
            .filter_map(|(t, s, mat)| {
                let texture = model.textures.get(*t)?;
                let uvsc = anims.uvsc.iter().position(|u| u.index + 1 == *s as u32)?;
                let geometry = kh2_model::Texture { rgba: Vec::new(), ..texture.clone() };
                Some(Scroll { material: mat.clone(), uvsc, texture: geometry, span: texture.uv_span? })
            })
            .collect();
        if anims.texa.is_empty() && scrolls.is_empty() {
            return None;
        }
        let mut targets: Vec<Target> = Vec::new();
        for a in anims.texa.iter().filter(|a| a.in_place) {
            if targets.iter().any(|t| t.texture == a.texture) {
                continue;
            }
            let (Some(t), Some(h)) = (model.textures.get(a.texture), textures.get(a.texture)) else { continue };
            let [ru, rv] = t.region;
            targets.push(Target { texture: a.texture, handle: h.clone(), base: crate::assets::crop(t), origin: [ru.start, rv.start], size: [ru.len, rv.len] });
        }
        Some(TexAnim {
            texa: anims.texa.iter().map(TexaPlayer::new).collect(),
            shown: vec![None; anims.texa.len()],
            uvsc: vec![UvscPlayer::default(); anims.uvsc.len()],
            anims: Arc::new(anims.clone()),
            targets,
            scrolls,
            tick: 0,
            slot: None,
        })
    }

    /// The game's `rand()` (FUN_003117a8): the one stream every user shares (`crate::rng`).
    fn rand(&mut self) -> u32 {
        crate::rng::rand()
    }

    /// FUN_0011e850: every TEXA that has `slot` in its slot table plays that animation this field.
    fn play_slot(&mut self, slot: u32) {
        let anims = self.anims.clone();
        for (i, t) in anims.texa.iter().enumerate() {
            let mut p = std::mem::take(&mut self.texa[i]);
            p.play_slot(t, slot, &mut || self.rand());
            self.texa[i] = p;
        }
    }

    /// One field: advance every player; returns the textures whose sprites changed.
    fn advance(&mut self) -> Vec<usize> {
        self.tick += 1;
        let anims = self.anims.clone();
        let mut dirty = Vec::new();
        for (i, t) in anims.texa.iter().enumerate() {
            let mut p = std::mem::take(&mut self.texa[i]);
            p.update(t, 1.0, &mut || self.rand());
            let now = pinned(i).filter(|&s| s < t.sprites.len()).or(p.sprite(t));
            self.texa[i] = p;
            if now != self.shown[i] {
                if texlog() {
                    info!("texlog tick {}: texa {i} (texture {}) sprite {:?} -> {:?}", self.tick, t.texture, self.shown[i], now);
                }
                self.shown[i] = now;
                if !dirty.contains(&t.texture) {
                    dirty.push(t.texture);
                }
            }
        }
        for (p, s) in self.uvsc.iter_mut().zip(&anims.uvsc) {
            p.update(s, 1.0);
        }
        dirty
    }

    /// The image of target `k` with every showing sprite copied in (clipped to the crop).
    fn compose(&self, k: usize) -> Vec<u8> {
        let tg = &self.targets[k];
        let mut img = tg.base.clone();
        let (cw, ch) = (tg.size[0] as i64, tg.size[1] as i64);
        for (i, a) in self.anims.texa.iter().enumerate() {
            let Some(s) = self.shown[i].filter(|_| a.texture == tg.texture && a.in_place) else { continue };
            let Some(px) = a.sprites.get(s) else { continue };
            for y in 0..a.height as i64 {
                let dy = a.y as i64 + y - tg.origin[1] as i64;
                if !(0..ch).contains(&dy) {
                    continue;
                }
                for x in 0..a.width as i64 {
                    let dx = a.x as i64 + x - tg.origin[0] as i64;
                    if !(0..cw).contains(&dx) {
                        continue;
                    }
                    let (d, o) = (4 * (dx + dy * cw) as usize, 4 * (x + y * a.width as i64) as usize);
                    img[d..d + 4].copy_from_slice(&px[o..o + 4]);
                }
            }
        }
        img
    }
}

/// Motion range trigger 23: while its frame range covers the motion's frame, the model plays the
/// TEXA animation its first parameter names by slot. The game's range dispatcher FUN_00188bd8
/// calls the handler FUN_00188030 for every range containing the current frame (end 0xffff = open),
/// and case 0x17 calls FUN_00192c80 → FUN_0011e850 → FUN_00133058 on every TEXA whose slot table
/// has that slot. Outside the range the next update returns to the default animation.
pub const RANGE_TEXA: u8 = 23;

fn tick(
    mut q: Query<(&mut TexAnim, Option<&crate::skel::Animator>)>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (mut a, anim) in &mut q {
        let mut slot_now = None;
        if let Some((tr, f)) = anim.and_then(|m| m.triggers().map(|t| (t, m.gframe()))) {
            for r in tr.ranges.iter().filter(|r| r.id == RANGE_TEXA && r.start as f32 <= f && (r.end == -1 || f < r.end as f32)) {
                if let Some(&slot) = r.params.first() {
                    a.play_slot(slot as u16 as u32);
                    slot_now = Some(slot as u16 as u32);
                }
            }
        }
        if slot_now != a.slot && texlog() {
            info!("texlog tick {}: motion range trigger {RANGE_TEXA} slot {:?} -> {slot_now:?} ({})", a.tick, a.slot, anim.map_or("-", |m| m.tag()));
        }
        a.slot = slot_now;
        let dirty = a.advance();
        for k in 0..a.targets.len() {
            if dirty.contains(&a.targets[k].texture) {
                let data = a.compose(k);
                if let Some(mut img) = images.get_mut(&a.targets[k].handle) {
                    img.data = Some(data);
                }
            }
        }
        for s in &a.scrolls {
            let p = a.uvsc[s.uvsc];
            let off = p.uv(s.span, s.texture.width, s.texture.height);
            let want = region_affine(&s.texture, off);
            if materials.get(&s.material).is_some_and(|m| m.uv_transform != want)
                && let Some(mut m) = materials.get_mut(&s.material)
            {
                m.uv_transform = want;
            }
        }
    }
}
