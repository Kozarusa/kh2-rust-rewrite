//! Tests against the user's disc (skipped without KH2_ISO or local/iso.txt): the system effects
//! that use the ported programs `pppKeShpTail3X` (element 0x10), `pppSnoMdlDttNorm` (element 0x12)
//! and `pppKeMvYpEff` (element 0x13, the summon arrival) run for a few hundred ticks and draw.
//!
//! An element's DPX entry is found by the entry's number word, not by its place in the list
//! (`FUN_001d9720`; `Pax::dpx_effect`). `00effect.bar` numbers its 14 entries 0..5 and 11..18, so the
//! earlier versions of these tests (which indexed by place with ids 11, 12 and 13) ran the entries
//! numbered 16, 17 and 18: those are the entries of the elements 0x10, 0x12 and 0x13.

use std::sync::Arc;

use glam::{Mat4, Vec3};
use kh2_fx::{Effect, Env, Pax};

fn system_pax() -> Option<Pax> {
    let path = kh2_factory::disc_path("disc")?;
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
        // The game finds the DPX entry by its number word, not by its place (FUN_001d9720).
        let fx = pax.dpx_effect(el.effect).unwrap_or_else(|| panic!("id {id}: no DPX entry numbered {}", el.effect));
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

/// Program kinds (names) used by the DPX entry element `id` names, and the ones without a handler.
fn kinds_of(pax: &Pax, id: u16) -> (Vec<&'static str>, Vec<&'static str>) {
    let el = pax.elements_for(id).next().expect("element");
    let fx = pax.dpx_effect(el.effect).expect("DPX entry");
    let pd = &pax.dpds[fx.dpd].particles[fx.particle_data];
    let mut kinds: Vec<u32> = pd.groups.iter().flat_map(|g| g.programs.iter().map(|p| p.kind)).collect();
    kinds.sort_unstable();
    kinds.dedup();
    let names = kinds.iter().map(|&k| kh2_fx::ppp::name(k)).collect();
    let missing = kinds.iter().filter(|&&k| kh2_fx::unported(k)).map(|&k| kh2_fx::ppp::name(k)).collect();
    (names, missing)
}

/// `00effect.bar` numbers its 14 DPX entries 0..5 and 11..18, so every element resolves by number
/// (`Pax::dpx_effect`) and the elements 0xe..0x13 name numbers past the entry count (by place they
/// would resolve to nothing). Element 11 (0xb) names the entry numbered 11 (place 6), not the Tail3X
/// entry numbered 16 that the place lookup gave it.
#[test]
fn system_dpx_entries_resolve_by_number() {
    let Some(pax) = system_pax() else { return };
    let numbers: Vec<u32> = pax.effects.iter().map(|f| f.number).collect();
    assert_eq!(numbers, [0, 1, 2, 3, 4, 5, 11, 12, 13, 14, 15, 16, 17, 18]);
    for el in &pax.elements {
        let f = pax.dpx_effect(el.effect).unwrap_or_else(|| panic!("element {:#x}: no entry numbered {}", el.id, el.effect));
        println!("element {:#x}: number {} -> place {} (particle data {})", el.id, el.effect, pax.effects.iter().position(|x| x.number == f.number).unwrap(), f.particle_data);
        assert!(el.id < 0xe || el.effect as usize >= pax.effects.len(), "element {:#x} names number {} past the {} entries", el.id, el.effect, pax.effects.len());
    }
    let (names, _) = kinds_of(&pax, 0xb);
    assert!(!names.contains(&"pppKeShpTail3X"), "element 0xb is not the Tail3X effect: {names:?}");
}

#[test]
fn tail3_draws_a_tail() {
    let Some(pax) = system_pax() else { return };
    let (names, _) = kinds_of(&pax, 0x10);
    assert!(names.contains(&"pppKeShpTail3X"), "element 0x10 uses Tail3X: {names:?}");
    let (drew, most, _) = run(&pax, 0x10, 120);
    println!("effect 0x10 (pppKeShpTail3X): drew in {drew} frames, up to {most} vertices");
    assert!(drew > 0 && most > 4);
}

#[test]
fn sno_distortion_runs() {
    let Some(pax) = system_pax() else { return };
    let (names, _) = kinds_of(&pax, 0x12);
    assert!(names.contains(&"pppSnoMdlDttNorm"), "element 0x12 uses Sno: {names:?}");
    let (drew, most, distort) = run(&pax, 0x12, 120);
    println!("effect 0x12 (pppSnoMdlDttNorm): drew in {drew} frames, up to {most} vertices, distortion frames {distort}");
    assert!(distort > 0, "no distortion batch was produced");
}

/// The summon arrival effect (system effect 0x13, element 19 of `00effect.bar`, DPX entry numbered 18): a
/// looping 60-frame effect of 20 groups; every program kind it uses has a handler, and it draws every
/// frame once its objects are born (group 8, the `pppKeMvYpEff` user, is born by `pppKeBornRnd5`).
#[test]
fn summon_arrival_0x13_draws() {
    let Some(pax) = system_pax() else { return };
    let el = pax.elements_for(0x13).next().expect("element 0x13");
    let fx = pax.dpx_effect(el.effect).expect("DPX entry numbered 18");
    let pd = &pax.dpds[fx.dpd].particles[fx.particle_data];
    println!("effect 0x13: element effect word {} -> dpd {} particle data {} life {} flags {:#x}, {} groups", el.effect, fx.dpd, fx.particle_data, pd.life / kh2_fx::pax::TICK, pd.flags, pd.groups.len());
    assert_eq!((el.effect, pd.flags & 1), (18, 1), "entry 18, looping");
    let (names, missing) = kinds_of(&pax, 0x13);
    println!("effect 0x13 program kinds: {names:?}; without a handler: {missing:?}");
    assert!(names.contains(&"pppKeMvYpEff"));
    assert!(missing.is_empty(), "program kinds without a handler: {missing:?}");
    let (drew, most, _) = run(&pax, 0x13, 300);
    println!("effect 0x13 (summon arrival): drew in {drew} frames, up to {most} vertices");
    assert!(drew >= 290 && most > 4, "the summon arrival effect must draw every frame (drew {drew})");
}
