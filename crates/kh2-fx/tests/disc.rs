//! Tests against the user's disc (skipped without KH2_ISO or local/iso.txt): the system effects
//! that use the ported programs `pppKeShpTail3X` (id 11), `pppSnoMdlDttNorm` (id 12) and
//! `pppKeMvYpEff` (id 13) run for a few hundred ticks and draw.

use std::sync::Arc;

use glam::{Mat4, Vec3};
use kh2_fx::{Effect, Env, Pax};

fn system_pax() -> Option<Pax> {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))?;
    if !std::path::Path::new(&path).exists() {
        return None;
    }
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    let file = a.read("00effect.bar").ok()?;
    let entries = kh2_bar::parse(&file).ok()?;
    Pax::parse(entries.iter().find(|e| e.kind == 18)?.data).ok()
}

/// Runs every element of effect `id` for `ticks` frames with the bind moving along +x; returns
/// (frames that drew anything, most vertices in one frame, frames with distortion batches).
fn run(pax: &Pax, id: u16, ticks: usize) -> (usize, usize, usize) {
    let (mut drew, mut most, mut distort) = (0, 0, 0);
    for el in pax.elements_for(id) {
        let fx = &pax.effects[el.effect as usize];
        let mut e = Effect::new(Arc::new(pax.dpds[fx.dpd].clone()), fx.dpd, fx.particle_data).expect("effect");
        for t in 0..ticks {
            let bind = Mat4::from_translation(Vec3::new(t as f32 * 4.0, 0.0, 0.0));
            let view = Mat4::from_translation(Vec3::new(0.0, 0.0, 400.0));
            let env = Env { bind, view, view_inv: view.inverse(), ..Default::default() };
            let draws = e.tick(&env);
            let verts: usize = draws.iter().map(|b| b.verts.len()).sum();
            assert!(draws.iter().all(|b| b.verts.iter().all(|v| v.pos.is_finite())), "id {id} tick {t}: non-finite vertex");
            if verts > 0 {
                drew += 1;
            }
            if draws.iter().any(|b| !b.offsets.is_empty()) {
                distort += 1;
            }
            most = most.max(verts);
            if e.done {
                break;
            }
        }
    }
    (drew, most, distort)
}

#[test]
fn tail3_draws_a_tail() {
    let Some(pax) = system_pax() else { return };
    let (drew, most, _) = run(&pax, 11, 120);
    println!("effect 11 (pppKeShpTail3X): drew in {drew} frames, up to {most} vertices");
    assert!(drew > 0 && most > 4);
}

#[test]
fn sno_distortion_runs() {
    let Some(pax) = system_pax() else { return };
    let (drew, most, distort) = run(&pax, 12, 120);
    println!("effect 12 (pppSnoMdlDttNorm): drew in {drew} frames, up to {most} vertices, distortion frames {distort}");
    assert!(distort > 0, "no distortion batch was produced");
}

#[test]
fn mv_yp_eff_runs() {
    let Some(pax) = system_pax() else { return };
    let (drew, most, _) = run(&pax, 13, 120);
    println!("effect 13 (pppKeMvYpEff): drew in {drew} frames, up to {most} vertices");
}
