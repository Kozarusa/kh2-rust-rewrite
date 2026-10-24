//! Oracle tests of rows the L1 close-out found without a test named after their address (WP-L1-P27b). kh2-ee runs the original
//! from the user's own executable.
//!
//! * The prop leaves `0x15ff50` (colour scale) and the three empty virtual slots `0x15f488`, `0x15ffb8`, `0x160ff8`: the factory
//!   fuzz on a random prop block; the empty ones must leave every byte of it alone.
//! * The group kinds 7 and 9 of the area (`kh2_ard::kinds`): `0x199e68` (the touch request into the global request object, with
//!   the real `0x1ccb10` running) and `0x19a000` (the unit streaming). For `0x19a000` the callees that belong to the object list
//!   and the unit spawner are hooks that log what the group asks for: the object lookup `0x16b0d8`, the defeated-unit cache
//!   `0x1942e0`, the position getter `0x16b960`, the facing getter `0x16a890`, the spawn `0x1982c8` and the unit's method at
//!   vtable `+0x20`; the port's `UnitHost` answers from the same case and logs the same. The vector code of the direction and
//!   the absolute-value leaf `0x300188` run as the game has them.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ard::kinds::{self, StreamLimits, UnitHost};
use kh2_ard::{Entity, Group};
use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::*;
use kh2_model::draw::props as pure;
use std::cell::RefCell;
use std::rc::Rc;

const PROP_BLOCK: Block = Block { size: 0x2b0, fields: &[(0x1a4, 1, Kind::Int), (0x1a5, 1, Kind::Int), (0x1a6, 1, Kind::Int), (0x2a8, 1, Kind::Int), (0x2a9, 1, Kind::Int), (0x2aa, 1, Kind::Int)], subs: &[] };

/// orig 0x15ff50: the three colour bytes of a prop scaled by its base colour (`value * base >> 7`).
#[test]
fn orig_15ff50_matches_the_game() {
    static SPEC: Spec = Spec {
        addr: 0x15ff50,
        args: &[Arg::Ptr(PROP_BLOCK), Arg::Int(&[0x80, 0xff, 0x100]), Arg::Int(&[0x80, 0xff, 0x100]), Arg::Int(&[0x80, 0xff, 0x100])],
        floats: 0,
        ret: Ret::None,
        globals: &[],
        stubs: &[],
        nulls: true,
    };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let (r, g, b) = (c.int(1) as u32, c.int(2) as u32, c.int(3) as u32);
        pure::colour_scale(c.block(0), r, g, b);
        Out::none()
    }) else { return };
    eprintln!("{} cases equal", st.cases);
}

fn empty_slot(addr: u32) {
    static ARGS: [Arg; 2] = [Arg::Ptr(PROP_BLOCK), Arg::Int(&[0x1234_5678])];
    let spec = Spec { addr, args: &ARGS, floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: true };
    let Some(st) = fuzz(&spec, 2000, |_| Out::none()) else { return };
    eprintln!("{} cases equal", st.cases);
}

/// orig 0x15f488: an empty virtual slot of the prop (nothing of the prop block changes).
#[test]
fn orig_15f488_matches_the_game() {
    kh2_game_core::props::empty_15f488();
    empty_slot(0x15f488);
}

/// orig 0x15ffb8: an empty virtual slot of the prop.
#[test]
fn orig_15ffb8_matches_the_game() {
    kh2_game_core::props::empty_15ffb8();
    empty_slot(0x15ffb8);
}

/// orig 0x160ff8: the empty destructor slot of the motion holder.
#[test]
fn orig_160ff8_matches_the_game() {
    kh2_game_core::props::empty_160ff8();
    empty_slot(0x160ff8);
}

// ---------------------------------------------------------------------------------------------------------------------
// Group kinds 7 and 9
// ---------------------------------------------------------------------------------------------------------------------

fn game(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(ee)
}

fn a(e: &Ee, k: usize) -> u32 {
    e.gpr[reg::A0 + k] as u32
}

fn group_of(units: &[(u16, [f32; 3])], header: [u8; 0x2c]) -> Group {
    Group {
        kind: 9,
        id: 1,
        entities: units.iter().map(|&(serial, position)| Entity { object: 1, position, rotation: [0.0; 3], spawn_kind: 0, spawn_arg: 0, serial, raw: [0; 0x40] }).collect(),
        header,
        activators: Vec::new(),
        paths: Vec::new(),
        returns: Vec::new(),
        signals: Vec::new(),
    }
}

/// orig 0x199e68: kind 7 touch: the group header's word `+0x1c` and bit 0 of the player's data word `+0x28` go into the request
/// object `DAT_003601d0` points at (nothing happens when it is null) through the real `0x1ccb10`.
#[test]
fn orig_199e68_matches_the_game() {
    let Some(mut ee) = game("0x199e68") else { return };
    const REC: u32 = 0x0180_0000;
    const HDR: u32 = 0x0180_1000;
    const PLAYER: u32 = 0x0180_2000;
    const PDATA: u32 = 0x0180_3000;
    const REQ: u32 = 0x0180_4000;
    let mut r = Rng::new(0x199e68);
    let (mut flagged, mut plain, mut null) = (0, 0, 0);
    for n in 0..2000 {
        let hdr: Vec<u8> = (0..0x2c).map(|_| r.word() as u8).collect();
        let word28 = r.word() as u32;
        let req: Vec<u8> = (0..0x40).map(|_| r.word() as u8).collect();
        let player = PLAYER;
        let use_null = r.below(8) == 0;
        ee.write(HDR, &hdr);
        ee.w32(REC + 8, HDR);
        ee.w32(PLAYER + 0x6c, PDATA);
        ee.w32(PDATA + 0x28, word28);
        ee.write(REQ, &req);
        ee.w32(0x3601d0, if use_null { 0 } else { REQ });
        ee.call(0x199e68, &[REC as u64, player as u64], &[], STACK).expect("FUN_00199e68");
        let got = ee.read::<0x40>(REQ);
        let mut want = req.clone();
        let header: [u8; 0x2c] = hdr.clone().try_into().unwrap();
        let g = group_of(&[], header);
        kinds::touch_request(&g, if use_null { None } else { Some(&mut want[..]) }, player, word28);
        assert_eq!(got.to_vec(), want, "case {n}");
        if use_null {
            null += 1;
        } else if word28 & 1 != 0 {
            flagged += 1;
        } else {
            plain += 1;
        }
    }
    assert!(flagged > 100 && plain > 100 && null > 50, "{flagged} flagged, {plain} plain, {null} null");
}

const GROUP_REC: u32 = 0x0181_0000;
const GROUP_HDR: u32 = 0x0181_1000;
const POS_PLAYER: u32 = 0x0182_0000;
const POS_OBJ: u32 = 0x0182_1000;
const VT: u32 = 0x0183_0000;
const OBJ_BASE: u32 = 0x0184_0000;
const HOOK_UNIT_METHOD: u32 = 0x0010_2000;

/// One case of the unit streaming: where the player and the units are, which units live, which are cached.
#[derive(Clone)]
struct Case {
    player: [f32; 3],
    facing: [f32; 3],
    radius: f32,
    units: Vec<(u16, [f32; 3])>,
    live: Vec<Option<[f32; 3]>>,
    cached: Vec<bool>,
}

struct World {
    case: Case,
    limits: StreamLimits,
    log: Vec<(u8, u32)>,
}

impl UnitHost for World {
    fn player_position(&self) -> [f32; 3] {
        self.case.player
    }
    fn player_facing(&self) -> [f32; 3] {
        self.case.facing
    }
    fn find_object(&mut self, serial: u16) -> Option<u32> {
        let i = self.case.units.iter().position(|u| u.0 == serial)?;
        self.case.live[i].map(|_| OBJ_BASE + 0x100 * i as u32)
    }
    fn is_cached(&mut self, serial: u16) -> bool {
        let i = self.case.units.iter().position(|u| u.0 == serial).unwrap();
        self.case.cached[i]
    }
    fn object_position(&self, obj: u32) -> [f32; 3] {
        self.case.live[((obj - OBJ_BASE) / 0x100) as usize].unwrap()
    }
    fn appear(&mut self, entry: usize) {
        self.log.push((0, entry as u32));
    }
    fn disappear(&mut self, obj: u32) {
        self.log.push((1, obj));
    }
}

fn rf(r: &mut Rng, span: f32) -> f32 {
    ((r.word() >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0) * span
}

fn make_case(r: &mut Rng) -> Case {
    let n = 1 + r.below(6) as usize;
    let player = [rf(r, 4000.0), rf(r, 300.0), rf(r, 4000.0)];
    let mut f = [rf(r, 1.0), if r.below(3) == 0 { rf(r, 0.5) } else { 0.0 }, rf(r, 1.0)];
    let len = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt().max(0.01);
    f = [f[0] / len, f[1] / len, f[2] / len];
    let radius = 300.0 + (r.word() % 3000) as f32;
    let mut units = Vec::new();
    let (mut live, mut cached) = (Vec::new(), Vec::new());
    for i in 0..n {
        let span = radius * 1.6;
        let pos = [player[0] + rf(r, span), rf(r, 300.0), player[2] + rf(r, span)];
        units.push((0x100 + i as u16, pos));
        live.push((r.below(2) == 0).then(|| [player[0] + rf(r, span), rf(r, 300.0), player[2] + rf(r, span)]));
        cached.push(r.below(5) == 0);
    }
    Case { player, facing: f, radius, units, live, cached }
}

fn f32s(v: &[f32; 3]) -> Vec<u8> {
    let mut o: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
    o.extend_from_slice(&0u32.to_le_bytes());
    o
}

/// orig 0x19a000: kind 9 streams its units in and out by distance and facing (see `kinds::stream_units`).
#[test]
fn orig_19a000_matches_the_game() {
    let Some(mut ee) = game("0x19a000") else { return };
    let limits = StreamLimits { near: ee.f32_at(0x37b310), facing: ee.f32_at(0x37b314) };
    let state: Rc<RefCell<Option<Case>>> = Rc::new(RefCell::new(None));
    let log: Rc<RefCell<Vec<(u8, u32)>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let st = state.clone();
        ee.hooks.insert(0x16b0d8, move |e: &mut Ee| {
            let serial = a(e, 0) as u16;
            let c = st.borrow();
            let c = c.as_ref().unwrap();
            let i = c.units.iter().position(|u| u.0 == serial).unwrap();
            e.gpr[reg::V0] = if c.live[i].is_some() { (OBJ_BASE + 0x100 * i as u32) as u128 } else { 0 };
        });
    }
    {
        let st = state.clone();
        ee.hooks.insert(0x1942e0, move |e: &mut Ee| {
            let serial = a(e, 1) as u16;
            let c = st.borrow();
            let c = c.as_ref().unwrap();
            let i = c.units.iter().position(|u| u.0 == serial).unwrap();
            e.gpr[reg::V0] = c.cached[i] as u128;
        });
    }
    {
        let st = state.clone();
        ee.hooks.insert(0x16b960, move |e: &mut Ee| {
            let who = a(e, 0);
            let c = st.borrow();
            let c = c.as_ref().unwrap();
            if (OBJ_BASE..OBJ_BASE + 0x1000).contains(&who) {
                let i = ((who - OBJ_BASE) / 0x100) as usize;
                e.write(POS_OBJ, &f32s(&c.live[i].unwrap()));
                e.gpr[reg::V0] = POS_OBJ as u128;
            } else {
                e.write(POS_PLAYER, &f32s(&c.player));
                e.gpr[reg::V0] = POS_PLAYER as u128;
            }
        });
    }
    {
        let st = state.clone();
        ee.hooks.insert(0x16a890, move |e: &mut Ee| {
            let c = st.borrow();
            let f = c.as_ref().unwrap().facing;
            e.write(a(e, 0), &f32s(&f));
            e.gpr[reg::V0] = a(e, 0) as u128;
        });
    }
    {
        let l = log.clone();
        ee.hooks.insert(0x1982c8, move |e: &mut Ee| {
            l.borrow_mut().push((0, (a(e, 0) - (GROUP_HDR + 0x2c)) / 0x40));
        });
    }
    {
        let l = log.clone();
        ee.hooks.insert(HOOK_UNIT_METHOD, move |e: &mut Ee| {
            l.borrow_mut().push((1, a(e, 0)));
        });
    }
    ee.w32(VT + 0x20, HOOK_UNIT_METHOD);
    ee.w32(0x0010_2000, 0x03e0_0008);
    ee.w32(0x0010_2004, 0);
    let mut r = Rng::new(0x19a000);
    let (mut appeared, mut gone, mut stayed) = (0, 0, 0);
    for n in 0..3000 {
        let case = make_case(&mut r);
        // The record: header pointer at +8, entity count at header +4, radius at +0x1c, the 0x40-byte entities from +0x2c
        // (position at +4, serial at +0x1e).
        ee.w32(GROUP_REC + 8, GROUP_HDR);
        let mut hdr = vec![0u8; 0x2c + 0x40 * case.units.len()];
        hdr[4..6].copy_from_slice(&(case.units.len() as u16).to_le_bytes());
        hdr[0x1c..0x20].copy_from_slice(&case.radius.to_le_bytes());
        for (i, (serial, pos)) in case.units.iter().enumerate() {
            let o = 0x2c + 0x40 * i;
            hdr[o + 4..o + 16].copy_from_slice(&f32s(pos)[..12]);
            hdr[o + 0x1e..o + 0x20].copy_from_slice(&serial.to_le_bytes());
        }
        ee.write(GROUP_HDR, &hdr);
        for i in 0..case.units.len() {
            ee.w32(OBJ_BASE + 0x100 * i as u32, VT);
        }
        *state.borrow_mut() = Some(case.clone());
        log.borrow_mut().clear();
        ee.call(0x19a000, &[GROUP_REC as u64], &[], STACK).expect("FUN_0019a000");
        let got = log.borrow().clone();
        let mut w = World { case: case.clone(), limits, log: Vec::new() };
        let g = group_of(&case.units, [0; 0x2c]);
        let lim = w.limits;
        kinds::stream_units(&mut w, &g, case.radius, lim);
        assert_eq!(got, w.log, "case {n}");
        appeared += got.iter().filter(|e| e.0 == 0).count();
        gone += got.iter().filter(|e| e.0 == 1).count();
        stayed += case.units.len() - got.len().min(case.units.len());
    }
    assert!(appeared > 500 && gone > 500 && stayed > 500, "{appeared} appeared, {gone} gone, {stayed} unchanged");
}
