//! Oracle tests for the core leaves of `kh2_game_core::leaves_b` (WP-L1-P11b, originals between 0x14afc9 and 0x236a80). kh2-ee runs
//! each original from the user's executable on random blocks and globals (the harness of `kh2_factory::fuzz`), the port runs on copies
//! of the same bytes, and every block byte, global byte and return value is compared (1,000 to 6,000 cases per function). Tests that
//! need an input the generator cannot draw (a global that points at a block, a loop count that must stay small, two states of a list
//! tail) lay it out by hand and use `fuzz_with` or the interpreter directly; each says so.
//!
//! Stand-ins: none. Every original runs for real with no callee replaced. Globals that point at objects are set to a block's address by
//! the `setup` closure of the test before the first case.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::Ee;
use kh2_factory::fuzz::*;
use kh2_game_core::leaves_b::*;

const SEED: u64 = 0x6b68_3272;

fn r32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn w32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

/// An interpreter with the executable loaded, or `None` without a disc.
fn game(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(ee)
}

/// The fuzz loop with a setup step that writes globals (pointers to the arena, small indices) before the first case.
fn run(spec: &Spec, cases: u32, setup: impl FnOnce(&mut Ee), port: impl FnMut(&mut Case) -> Out) {
    let Some(mut ee) = game(&format!("oracle test of {:#x}", spec.addr)) else { return };
    setup(&mut ee);
    let st = fuzz_with(&mut ee, spec, cases, SEED, port);
    eprintln!("{:#x}: {} cases equal, {} interpreter steps in {:.3} s", spec.addr, st.cases, st.steps, st.secs);
}

const fn blk(size: u32, fields: &'static [FieldSpec]) -> Block {
    Block { size, fields, subs: &[] }
}

const fn spec(addr: u32, args: &'static [Arg], floats: u8, ret: Ret, globals: &'static [(u32, u32, bool)]) -> Spec {
    Spec { addr, args, floats, ret, globals, stubs: &[], nulls: false }
}

fn out_bool(b: bool) -> Out {
    Out::int(b as i64)
}

// ---- the five rectangle setters (a 64-bit store through the first argument) --------------------------------------------------

macro_rules! rect_setter {
    ($test:ident, $addr:expr, $off:expr, $size:expr, $port:path) => {
        #[test]
        fn $test() {
            const S: Spec = spec($addr, &[Arg::Ptr(blk($size, &[($off, 8, Kind::Int)])), Arg::Int(&[])], 0, Ret::None, &[]);
            run(&S, 2000, |_| {}, |c| {
                let v = c.ints[1];
                $port(c.block(0), v);
                Out::none()
            });
        }
    };
}
rect_setter!(orig_14b350_matches_the_game, 0x14b350, 0xc0, 0xd0, rect_set_gs_prim);
rect_setter!(orig_14b378_matches_the_game, 0x14b378, 0x40, 0x50, rect_set_gs_frame);
rect_setter!(orig_14b388_matches_the_game, 0x14b388, 0x80, 0x90, rect_set_gs_test);
rect_setter!(orig_14b398_matches_the_game, 0x14b398, 0x60, 0x70, rect_set_gs_tex0);
rect_setter!(orig_14b3a0_matches_the_game, 0x14b3a0, 0x70, 0x80, rect_set_gs_tex1);

// ---- address arithmetic and constants ------------------------------------------------------------------------------------

#[test]
fn orig_14e370_matches_the_game() {
    const S: Spec = spec(0x14e370, &[Arg::Int(&[])], 0, Ret::Int, &[]);
    run(&S, 2000, |_| {}, |c| Out::int(offset_2f0(c.int(0)) as i64));
}

#[test]
fn orig_1a9420_matches_the_game() {
    const S: Spec = spec(0x1a9420, &[Arg::Int(&[])], 0, Ret::Int, &[]);
    run(&S, 2000, |_| {}, |c| Out::int(font_entry_addr(c.int(0)) as i64));
}

#[test]
fn orig_1ddff8_matches_the_game() {
    const S: Spec = spec(0x1ddff8, &[Arg::Int(&[]), Arg::Int(&[]), Arg::Int(&[])], 0, Ret::Int, &[]);
    run(&S, 2000, |_| {}, |c| Out::int(cell_addr(c.int(0), c.int(1), c.int(2)) as i64));
}

#[test]
fn orig_1b3d68_matches_the_game() {
    const S: Spec = spec(0x1b3d68, &[], 0, Ret::Int, &[(0x1d5d6b0, 4, true)]);
    run(&S, 2000, |_| {}, |c| Out::int(global_1d5d6b0_plus_18(r32(c.global(0), 0)) as i64));
}

#[test]
fn orig_2302c8_matches_the_game() {
    const S: Spec = spec(0x2302c8, &[], 0, Ret::Int, &[(0x36c200, 4, true)]);
    run(&S, 2000, |_| {}, |c| Out::int(musical_target_image(r32(c.global(0), 0)) as i64));
}

#[test]
fn orig_1cf330_matches_the_game() {
    const S: Spec = spec(0x1cf330, &[], 0, Ret::Int, &[(0x360400, 4, true)]);
    run(&S, 2000, |_| {}, |c| Out::int(sora_object(r32(c.global(0), 0)) as i64));
}

/// The function with no input: the interpreter runs it once (the fuzz loop refuses a test whose every case is equal).
#[test]
fn orig_15ea40_matches_the_game() {
    let Some(mut ee) = game("oracle test of 0x15ea40") else { return };
    let v = ee.call(0x15ea40, &[], &[], STACK).expect("0x15ea40");
    assert_eq!(v as u32, map_pax_addr());
}

/// The empty function: it returns and writes nothing outside the stack.
#[test]
fn orig_1d13f0_matches_the_game() {
    let Some(mut ee) = game("oracle test of 0x1d13f0") else { return };
    let before = ee.ram.clone();
    ee.call(0x1d13f0, &[], &[], STACK).expect("0x1d13f0");
    let touched = (0..ee.ram.len()).filter(|&i| before[i] != ee.ram[i] && !(STACK - 0x10000..STACK + 0x100).contains(&(i as u32))).count();
    assert_eq!(touched, 0, "the empty function writes nothing");
    nop_1d13f0();
}

// ---- single-byte globals ---------------------------------------------------------------------------------------------------

#[test]
fn orig_15c9a8_matches_the_game() {
    const S: Spec = spec(0x15c9a8, &[], 0, Ret::None, &[(0x359038, 4, true)]);
    run(&S, 1000, |_| {}, |c| {
        clear_byte_359038(&mut c.global(0)[0]);
        Out::none()
    });
}
#[test]
fn orig_15eda0_matches_the_game() {
    const S: Spec = spec(0x15eda0, &[], 0, Ret::None, &[(0x35a80d, 4, true)]);
    run(&S, 1000, |_| {}, |c| {
        map_clear_pax_ready(&mut c.global(0)[0]);
        Out::none()
    });
}
#[test]
fn orig_1612a0_matches_the_game() {
    const S: Spec = spec(0x1612a0, &[], 0, Ret::None, &[(0x35a872, 4, true)]);
    run(&S, 1000, |_| {}, |c| {
        camera_set_no_kick(&mut c.global(0)[0]);
        Out::none()
    });
}
#[test]
fn orig_1b62d8_matches_the_game() {
    const S: Spec = spec(0x1b62d8, &[], 0, Ret::None, &[(0x35f1d1, 4, true)]);
    run(&S, 1000, |_| {}, |c| {
        clear_byte_35f1d1(&mut c.global(0)[0]);
        Out::none()
    });
}
#[test]
fn orig_1d3a58_matches_the_game() {
    const S: Spec = spec(0x1d3a58, &[], 0, Ret::None, &[(0x360770, 4, true)]);
    run(&S, 1000, |_| {}, |c| {
        clear_byte_360770(&mut c.global(0)[0]);
        Out::none()
    });
}
#[test]
fn orig_15ea30_matches_the_game() {
    const S: Spec = spec(0x15ea30, &[], 0, Ret::Int, &[(0x35a80c, 1, true)]);
    run(&S, 1000, |_| {}, |c| Out::int(map_pax_exists(c.global(0)[0]) as i64));
}
#[test]
fn orig_222058_matches_the_game() {
    const S: Spec = spec(0x222058, &[], 0, Ret::Int, &[(0x36bcd9, 1, true)]);
    run(&S, 1000, |_| {}, |c| Out::int(byte_36bcd9(c.global(0)[0]) as i64));
}
#[test]
fn orig_224b50_matches_the_game() {
    const S: Spec = spec(0x224b50, &[], 0, Ret::Int, &[(0x33e830, 1, true)]);
    // The byte is 0xb in about one case of 256, so more cases than the others.
    run(&S, 6000, |_| {}, |c| out_bool(event_is_lm(c.global(0)[0])));
}

// ---- word globals ---------------------------------------------------------------------------------------------------------------

#[test]
fn orig_2236c0_matches_the_game() {
    const S: Spec = spec(0x2236c0, &[], 0, Ret::None, &[(0x36b9cc, 8, true)]);
    run(&S, 1000, |_| {}, |c| {
        event_read_wait_init(c.global(0));
        Out::none()
    });
}
#[test]
fn orig_223780_matches_the_game() {
    const S: Spec = spec(0x223780, &[], 0, Ret::None, &[(0x36ba20, 16, true)]);
    run(&S, 1000, |_| {}, |c| {
        event_clear_3(c.global(0));
        Out::none()
    });
}
#[test]
fn orig_1dd950_matches_the_game() {
    const S: Spec = spec(0x1dd950, &[Arg::Int(&[])], 0, Ret::None, &[(0x360a70, 8, true)]);
    run(&S, 1000, |_| {}, |c| {
        let v = c.ints[0] as u32;
        set_360a70(c.global(0), v);
        Out::none()
    });
}
#[test]
fn orig_16c088_matches_the_game() {
    const S: Spec = spec(0x16c088, &[Arg::Int(&[])], 0, Ret::None, &[(0x522180, 16, true)]);
    run(&S, 1000, |_| {}, |c| {
        let v = c.ints[0] as u32;
        assert_list_init(c.global(0), v);
        Out::none()
    });
}
#[test]
fn orig_15a210_matches_the_game() {
    const S: Spec = spec(
        0x15a210,
        &[Arg::Ptr(blk(0x20, &[(0, 4, Kind::Int), (4, 4, Kind::Int), (8, 4, Kind::Int), (0xc, 4, Kind::Int), (0x10, 4, Kind::Int)]))],
        0,
        Ret::None,
        &[(0x358b40, 0x44, true)],
    );
    run(&S, 1000, |_| {}, |c| {
        let t = c.blocks[0].clone();
        target_camera_change(&t, c.global(0));
        Out::none()
    });
}
#[test]
fn orig_15c6d0_matches_the_game() {
    const S: Spec = spec(0x15c6d0, &[Arg::Int(&[]), Arg::Int(&[]), Arg::Int(&[]), Arg::Int(&[])], 0, Ret::None, &[(0x520600, 4, true)]);
    run(&S, 1000, |_| {}, |c| {
        let a: Vec<u8> = (0..4).map(|i| c.ints[i] as u8).collect();
        set_colour_520600(c.global(0), a[0], a[1], a[2], a[3]);
        Out::none()
    });
}
#[test]
fn orig_16f260_matches_the_game() {
    const S: Spec = spec(0x16f260, &[Arg::Range(0, 15), Arg::Int(&[])], 0, Ret::None, &[(0x1c88358, 64, true)]);
    run(&S, 2000, |_| {}, |c| {
        let (i, v) = (c.int(0), c.ints[1] as u32);
        store_slot_1c88358(c.global(0), i, v);
        Out::none()
    });
}


/// The cursor index at 0x35bd90 is set by `setup` to 0, 1, 7 and 15 in four runs (the original indexes a table of 16 words with it and
/// does not check); the table at 0x1c88340 and the pointer word at 0x35bd84 are random.
#[test]
fn orig_16ff18_matches_the_game() {
    const S: Spec = spec(0x16ff18, &[], 0, Ret::None, &[(0x1c88340, 64, true), (0x35bd84, 4, true), (0x35bd90, 4, false)]);
    for idx in [0u32, 1, 7, 15] {
        run(&S, 500, move |ee| ee.w32(0x35bd90, idx), |c| {
            let table = c.globals[0].clone();
            let mut ptr = r32(&c.globals[1], 0);
            let mut i = r32(&c.globals[2], 0);
            fix_free(&mut ptr, &mut i, &table);
            w32(&mut c.globals[1], 0, ptr);
            w32(&mut c.globals[2], 0, i);
            Out::none()
        });
    }
}

/// The queue count at 0x1c89b40 is set to 0..=3 by `setup` (entries 0x1c89b30..0x1c89b3f hold four words).
#[test]
fn orig_18b6c8_matches_the_game() {
    const S: Spec = spec(0x18b6c8, &[Arg::Int(&[0x1c89a70, 0x1c89aa0])], 0, Ret::None, &[(0x1c89b30, 16, true), (0x1c89b40, 4, false)]);
    for n in 0u32..4 {
        run(&S, 500, move |ee| ee.w32(0x1c89b40, n), |c| {
            let addr = c.int(0);
            let mut count = r32(&c.globals[1], 0);
            let mut entries = c.globals[0].clone();
            slot_queue_push(&mut entries, &mut count, addr);
            c.globals[0] = entries;
            w32(&mut c.globals[1], 0, count);
            Out::none()
        });
    }
}

#[test]
fn orig_1adfd0_matches_the_game() {
    const S: Spec = spec(0x1adfd0, &[], 0, Ret::None, &[(0x1cb8290, 48, true)]);
    run(&S, 1000, |_| {}, |c| {
        slot_table_clear(c.global(0));
        Out::none()
    });
}

/// The slot table words are drawn from 0..=5 and the search value from -1..=6, so every slot and the miss case are reached.
#[test]
fn orig_1ae388_matches_the_game() {
    let Some(mut ee) = game("oracle test of 0x1ae388") else { return };
    let mut rng = Rng::new(7);
    let mut hits = [0u32; 4];
    for n in 0..3000 {
        let mut g = [0u8; 36];
        for s in 0..3 {
            for k in 0..3 {
                w32(&mut g, s * 12 + k * 4, rng.below(6) as u32);
            }
        }
        ee.write(0x1cb8290, &g);
        let a0 = rng.below(8) as i32 - 1;
        let game = ee.call(0x1ae388, &[a0 as i64 as u64], &[], STACK).unwrap() as u32;
        let mine = slot_table_find(&g, a0);
        assert_eq!(game, mine as u32, "case {n}: value {a0}");
        hits[(mine + 1) as usize] += 1;
    }
    assert!(hits.iter().all(|&h| h > 100), "every outcome is reached: {hits:?}");
}

/// Equal and unequal ring words in turn (random words are never equal).
#[test]
fn orig_1a6b30_matches_the_game() {
    let Some(mut ee) = game("oracle test of 0x1a6b30") else { return };
    let mut rng = Rng::new(9);
    let mut equal = 0;
    for n in 0..2000 {
        let a = rng.word() as u32;
        let b = if n % 2 == 0 { a } else { a.wrapping_add(1 + rng.below(3) as u32) };
        let mut g = [0u8; 8];
        w32(&mut g, 0, a);
        w32(&mut g, 4, b);
        ee.write(0x1c95598, &g);
        let game = ee.call(0x1a6b30, &[], &[], STACK).unwrap() as u32;
        assert_eq!(game, ring_is_empty(&g) as u32, "case {n}: {a:#x} {b:#x}");
        equal += game;
    }
    assert_eq!(equal, 1000);
}

#[test]
fn orig_1a6b50_matches_the_game() {
    const S: Spec = spec(0x1a6b50, &[], 0, Ret::Int, &[(0x1c95598, 8, true)]);
    run(&S, 3000, |_| {}, |c| Out::int(ring_distance(c.global(0)) as i64));
}

// ---- object getters, setters and constructors -------------------------------------------------------------------------------

/// Three levels (gauge, the actor it points at, the actor's status block); the null branch of the port (gauge +0x34 null) is the
/// original's first test and is not reached because the generator draws a non-null pointer there (`nulls: false`).
#[test]
fn orig_155b78_matches_the_game() {
    const C: Block = blk(0x1b0, &[(0x1af, 1, Kind::Int)]);
    const B: Block = Block { size: 0x490, fields: &[(0x48c, 4, Kind::Ptr)], subs: &[(0x48c, C)] };
    const A: Block = Block { size: 0x40, fields: &[(0x34, 4, Kind::Ptr)], subs: &[(0x34, B)] };
    const S: Spec = spec(0x155b78, &[Arg::Ptr(A)], 0, Ret::Int, &[]);
    run(&S, 6000, |_| {}, |c| {
        let st = c.deref(1, 0x48c).map(|i| c.blocks[i].clone());
        out_bool(drive_meter_seq_is_4(st.as_deref()))
    });
    assert!(!drive_meter_seq_is_4(None));
}

#[test]
fn orig_157dd8_matches_the_game() {
    const S: Spec = spec(0x157dd8, &[Arg::Ptr(blk(0x40, &[(0x2c, 4, Kind::Int), (0x30, 4, Kind::Int)]))], 0, Ret::Int, &[]);
    run(&S, 2000, |_| {}, |c| Out::int(gauge_party_group(&c.blocks[0]) as i64));
}

#[test]
fn orig_16ecf8_matches_the_game() {
    const S: Spec = spec(0x16ecf8, &[Arg::Ptr(blk(0x30, &[(0x28, 1, Kind::Int)]))], 0, Ret::Int, &[]);
    run(&S, 2000, |_| {}, |c| out_bool(flag_28_set(&c.blocks[0])));
}

#[test]
fn orig_19b380_matches_the_game() {
    const S: Spec = spec(0x19b380, &[Arg::Ptr(blk(0x30, &[(0x28, 4, Kind::Int)]))], 0, Ret::None, &[]);
    run(&S, 1000, |_| {}, |c| {
        lockon_reset_player(c.block(0));
        Out::none()
    });
}

#[test]
fn orig_1b3f68_matches_the_game() {
    const S: Spec = spec(
        0x1b3f68,
        &[Arg::Ptr(blk(0x10, &[(0, 4, Kind::Int), (4, 4, Kind::Int), (8, 4, Kind::Int)])), Arg::Int(&[]), Arg::Int(&[]), Arg::Int(&[])],
        0,
        Ret::None,
        &[],
    );
    run(&S, 1000, |_| {}, |c| {
        let (a, b, d) = (c.ints[1] as u32, c.ints[2] as u32, c.ints[3] as u32);
        set_words_3(c.block(0), a, b, d);
        Out::none()
    });
}

#[test]
fn orig_1d3710_matches_the_game() {
    const S: Spec = spec(0x1d3710, &[Arg::Ptr(blk(0x10, &[(0, 4, Kind::Int), (4, 4, Kind::Int), (8, 4, Kind::Int)]))], 0, Ret::None, &[]);
    run(&S, 1000, |_| {}, |c| {
        clear_words_3(c.block(0));
        Out::none()
    });
}

#[test]
fn orig_1b60b8_matches_the_game() {
    const S: Spec = spec(0x1b60b8, &[Arg::Ptr(blk(0x10, &[(8, 4, Kind::Int)])), Arg::Int(&[])], 0, Ret::None, &[]);
    run(&S, 1000, |_| {}, |c| {
        let n = c.int(1);
        cursor_advance(c.block(0), n);
        Out::none()
    });
}

#[test]
fn orig_1bb338_matches_the_game() {
    const S: Spec = spec(0x1bb338, &[Arg::Ptr(blk(0x990, &[(0x974, 16, Kind::Int), (0x84c, 4, Kind::Int)]))], 0, Ret::None, &[]);
    run(&S, 1000, |_| {}, |c| {
        clear_slots_974(c.block(0));
        Out::none()
    });
}

#[test]
fn orig_1c40b8_matches_the_game() {
    const S: Spec = spec(0x1c40b8, &[Arg::Ptr(blk(0x60, &[(0x54, 4, Kind::Int)]))], 0, Ret::None, &[]);
    run(&S, 1000, |_| {}, |c| {
        set_handler_54(c.block(0));
        Out::none()
    });
}

macro_rules! effect_flag {
    ($test:ident, $addr:expr, $port:path) => {
        #[test]
        fn $test() {
            const S: Spec = spec(
                $addr,
                &[Arg::Ptr(blk(0x40, &[(8, 4, Kind::Int), (0x30, 4, Kind::Int), (0x34, 4, Kind::Int)])), Arg::Int(&[]), Arg::Int(&[])],
                0,
                Ret::None,
                &[],
            );
            run(&S, 1000, |_| {}, |c| {
                let (a, b) = (c.ints[1] as u32, c.ints[2] as u32);
                $port(c.block(0), a, b);
                Out::none()
            });
        }
    };
}
effect_flag!(orig_1da138_matches_the_game, 0x1da138, effect_set_param_40000);
effect_flag!(orig_1da158_matches_the_game, 0x1da158, effect_set_param_80000);

#[test]
fn orig_1da338_matches_the_game() {
    const S: Spec = spec(0x1da338, &[Arg::Ptr(blk(0x10, &[(8, 4, Kind::Int)]))], 0, Ret::None, &[]);
    run(&S, 1000, |_| {}, |c| {
        effect_set_no_draw(c.block(0));
        Out::none()
    });
}

#[test]
fn orig_1da350_matches_the_game() {
    const S: Spec = spec(0x1da350, &[Arg::Ptr(blk(0x10, &[(8, 4, Kind::Int)]))], 0, Ret::None, &[]);
    run(&S, 1000, |_| {}, |c| {
        effect_reset_no_draw(c.block(0));
        Out::none()
    });
}

#[test]
fn orig_1db410_matches_the_game() {
    const S: Spec = spec(0x1db410, &[Arg::Ptr(blk(0x20, &[(0, 28, Kind::Int)]))], 0, Ret::None, &[]);
    run(&S, 1000, |_| {}, |c| {
        pax_init(c.block(0));
        Out::none()
    });
}

#[test]
fn orig_2331d0_matches_the_game() {
    const S: Spec = spec(0x2331d0, &[Arg::Ptr(blk(0x1a0, &[(0, 4, Kind::Int), (0x38, 8, Kind::Int), (0x194, 4, Kind::Int)]))], 0, Ret::None, &[]);
    run(&S, 1000, |_| {}, |c| {
        image_init(c.block(0));
        Out::none()
    });
}

#[test]
fn orig_233200_matches_the_game() {
    const S: Spec = spec(0x233200, &[Arg::Ptr(blk(0x10, &[(0, 4, Kind::Int)]))], 0, Ret::None, &[]);
    run(&S, 1000, |_| {}, |c| {
        image_set_table(c.block(0));
        Out::none()
    });
}

#[test]
fn orig_233998_matches_the_game() {
    const S: Spec = spec(0x233998, &[Arg::Ptr(blk(0x20, &[(0, 24, Kind::Int)]))], 0, Ret::None, &[]);
    run(&S, 1000, |_| {}, |c| {
        record_init_233998(c.block(0));
        Out::none()
    });
}

#[test]
fn orig_234310_matches_the_game() {
    const S: Spec = spec(0x234310, &[Arg::Ptr(blk(0x60, &[(0x50, 4, Kind::Int)]))], 0, Ret::None, &[]);
    run(&S, 1000, |_| {}, |c| {
        clear_word_50(c.block(0));
        Out::none()
    });
}

#[test]
fn orig_234a40_matches_the_game() {
    const S: Spec = spec(0x234a40, &[Arg::Ptr(blk(0xa0, &[(0x9c, 4, Kind::Int)])), Arg::Int(&[])], 0, Ret::None, &[]);
    run(&S, 1000, |_| {}, |c| {
        let v = c.ints[1] as u32;
        set_word_9c(c.block(0), v);
        Out::none()
    });
}

#[test]
fn orig_2369d0_matches_the_game() {
    const S: Spec = spec(0x2369d0, &[Arg::Ptr(blk(0x100, &[(0xf8, 4, Kind::Int)]))], 0, Ret::Int, &[]);
    run(&S, 2000, |_| {}, |c| out_bool(sequence_draw_mode(&c.blocks[0])));
}

#[test]
fn orig_224448_matches_the_game() {
    const S: Spec = spec(0x224448, &[Arg::Ptr(blk(0x10, &[(0, 4, Kind::Int)]))], 0, Ret::Int, &[]);
    run(&S, 1000, |_| {}, |c| Out::int(event_get_int(&c.blocks[0]) as i64));
}

#[test]
fn orig_18cd90_matches_the_game() {
    const S: Spec = spec(0x18cd90, &[Arg::Ptr(blk(0x30, &[(0x2c, 1, Kind::Int), (0x2d, 1, Kind::Int)])), Arg::Int(&[3])], 0, Ret::None, &[]);
    run(&S, 2000, |_| {}, |c| {
        let m = c.ints[1] as u32;
        set_mode_and_mask(c.block(0), m);
        Out::none()
    });
}

#[test]
fn orig_18dc80_matches_the_game() {
    const REC: Block = blk(0x10, &[(4, 1, Kind::Int)]);
    const A: Block = Block { size: 0x30, fields: &[(0x28, 4, Kind::Ptr)], subs: &[(0x28, REC)] };
    const S: Spec = spec(0x18dc80, &[Arg::Ptr(A)], 0, Ret::Int, &[]);
    run(&S, 3000, |_| {}, |c| {
        let i = c.deref(0, 0x28).unwrap();
        out_bool(kind_is_2_or_3(&c.blocks[i]))
    });
}

#[test]
fn orig_1b5670_matches_the_game() {
    const S: Spec = spec(0x1b5670, &[Arg::Int(&[0, 2, 3]), Arg::Int(&[0, 1, 2, 3])], 0, Ret::Int, &[]);
    run(&S, 2000, |_| {}, |c| Out::int(code_to_slot(c.int(1)) as i64));
}

#[test]
fn orig_1a9438_matches_the_game() {
    const A: Block = Block { size: 0x20, fields: &[(0x1c, 4, Kind::Ptr)], subs: &[(0x1c, blk(0x40, &[]))] };
    const S: Spec = spec(0x1a9438, &[Arg::Ptr(A), Arg::Range(0, 0x3f)], 0, Ret::Int, &[]);
    run(&S, 2000, |_| {}, |c| {
        let buf = c.blocks[c.deref(0, 0x1c).unwrap()].clone();
        Out::int(byte_at_ptr_1c(&buf, c.int(1)) as i64)
    });
}

/// The global word at 0x1c958b4 is set to the address of the test's block (an unused second argument is the block).
#[test]
fn orig_1a9448_matches_the_game() {
    const S: Spec = spec(0x1a9448, &[Arg::Range(0, 0x3f), Arg::Ptr(blk(0x40, &[]))], 0, Ret::Int, &[(0x1c958b4, 4, false)]);
    run(&S, 2000, |ee| ee.w32(0x1c958b4, block_addr(0)), |c| {
        let buf = c.blocks[0].clone();
        Out::int(byte_at_global_buf(&buf, c.int(0)) as i64)
    });
}

#[test]
fn orig_191078_matches_the_game() {
    const S: Spec = spec(0x191078, &[Arg::Ptr(blk(0x40, &[])), Arg::Range(0, 0x20)], 0, Ret::Int, &[]);
    run(&S, 2000, |_| {}, |c| Out::int(halfword_at(&c.blocks[0], c.int(1) as usize) as i64));
}

#[test]
fn orig_233c38_matches_the_game() {
    const A: Block = Block { size: 0x10, fields: &[(8, 4, Kind::Int), (0xc, 4, Kind::Ptr)], subs: &[(0xc, blk(0x100, &[]))] };
    const S: Spec = spec(0x233c38, &[Arg::Ptr(A), Arg::Range(0, 0x1f)], 0, Ret::Int, &[]);
    run(&S, 2000, |_| {}, |c| {
        let t = c.blocks[c.deref(0, 0xc).unwrap()].clone();
        Out::int(base_plus_table_entry(&c.blocks[0], &t, c.int(1)) as i64)
    });
}

/// The stack count at +8 is drawn by hand (1..=8): a random count would index far outside memory.
#[test]
fn orig_1d8f90_matches_the_game() {
    let Some(mut ee) = game("oracle test of 0x1d8f90") else { return };
    let mut rng = Rng::new(11);
    let (obj, arr) = (block_addr(0), block_addr(1));
    for n in 0..2000 {
        let mut o: Vec<u8> = (0..0x20).map(|_| rng.word() as u8).collect();
        w32(&mut o, 8, 1 + rng.below(8) as u32);
        w32(&mut o, 0x10, arr);
        let a: Vec<u8> = (0..0x40).map(|_| rng.word() as u8).collect();
        ee.write(obj, &o);
        ee.write(arr, &a);
        let game = ee.call(0x1d8f90, &[obj as u64], &[], STACK).unwrap() as u32;
        let mine = stack_pop_scaled(&mut o, &a);
        assert_eq!(game, mine as u32, "case {n}");
        let after: Vec<u8> = (0..0x20).map(|k| ee.r8(obj + k)).collect();
        assert_eq!(after, o, "case {n}: object bytes");
    }
}

/// Four levels of pointers: the argument block holds a pointer to a block that holds a pointer to a block (+0x96c of which points at the
/// owner record whose +0x160 and +0x50c the original reads); the second argument is the draw record.
#[test]
fn orig_19cff8_matches_the_game() {
    const D: Block = blk(0x520, &[(0x160, 4, Kind::Int), (0x50c, 4, Kind::Int)]);
    const C: Block = Block { size: 0x980, fields: &[(0x96c, 4, Kind::Ptr)], subs: &[(0x96c, D)] };
    const B: Block = Block { size: 0x10, fields: &[(0, 4, Kind::Ptr)], subs: &[(0, C)] };
    const A: Block = Block { size: 0x10, fields: &[(0, 4, Kind::Ptr)], subs: &[(0, B)] };
    const S: Spec = spec(0x19cff8, &[Arg::Ptr(A), Arg::Ptr(blk(0x30, &[(0x1e, 1, Kind::Int), (0x22, 2, Kind::Int)]))], 0, Ret::Int, &[]);
    run(&S, 8000, |_| {}, |c| {
        let (flags, id) = (r32(&c.blocks[3], 0x160), r32(&c.blocks[3], 0x50c));
        Out::int(collision_draw_byte(&c.blocks[4], flags, id) as i64)
    });
}

#[test]
fn orig_19d5e8_matches_the_game() {
    const S: Spec = spec(0x19d5e8, &[Arg::Ptr(blk(0x20, &[(0, 28, Kind::Int)]))], 0, Ret::None, &[]);
    run(&S, 1000, |_| {}, |c| {
        blur_init(c.block(0));
        Out::none()
    });
}

#[test]
fn orig_236a80_matches_the_game() {
    const S: Spec = spec(
        0x236a80,
        &[Arg::Ptr(blk(0x130, &[(0xa8, 1, Kind::Int), (0x124, 4, Kind::Float), (0x128, 4, Kind::Float), (0x12c, 4, Kind::Float)]))],
        1,
        Ret::None,
        &[],
    );
    run(&S, 3000, |_| {}, |c| {
        let v = c.float(0);
        set_extent_z(c.block(0), v);
        Out::none()
    });
}

#[test]
fn orig_2349d0_matches_the_game() {
    const S: Spec = spec(
        0x2349d0,
        &[Arg::Ptr(blk(0x90, &[(0x2c, 4, Kind::Float), (0x30, 4, Kind::Float), (0x7c, 4, Kind::Float), (0x80, 4, Kind::Float), (0x84, 4, Kind::Float)]))],
        3,
        Ret::None,
        &[],
    );
    run(&S, 2000, |_| {}, |c| {
        let (a, b, d) = (c.float(0), c.float(1), c.float(2));
        set_scale3(c.block(0), a, b, d);
        Out::none()
    });
}

/// The two float constants the original loads from 0x37ae48 and 0x37ae4c are read from the executable's image (non-random globals).
#[test]
fn orig_191b58_matches_the_game() {
    const S: Spec = spec(
        0x191b58,
        &[Arg::Ptr(blk(0x20, &[(4, 4, Kind::Int), (8, 4, Kind::Int), (0xc, 4, Kind::Int), (0x14, 4, Kind::Int)]))],
        0,
        Ret::None,
        &[(0x37ae48, 4, false)],
    );
    run(&S, 1000, |_| {}, |c| {
        let init = r32(&c.globals[0], 0);
        ratio_reset(c.block(0), init);
        Out::none()
    });
}

#[test]
fn orig_191b78_matches_the_game() {
    const S: Spec = spec(
        0x191b78,
        &[Arg::Ptr(blk(0x20, &[(4, 4, Kind::Int), (8, 4, Kind::Int), (0x14, 4, Kind::Int)]))],
        0,
        Ret::None,
        &[(0x37ae4c, 4, false)],
    );
    run(&S, 1000, |_| {}, |c| {
        let init = r32(&c.globals[0], 0);
        ratio_restore(c.block(0), init);
        Out::none()
    });
}

#[test]
fn orig_191b98_matches_the_game() {
    const S: Spec = spec(0x191b98, &[Arg::Ptr(blk(0x10, &[(4, 4, Kind::Float), (8, 4, Kind::Float)]))], 0, Ret::Float, &[]);
    run(&S, 2000, |_| {}, |c| Out::float(ratio_product(&c.blocks[0])));
}

/// The scale that leaves a colour alone is the float at 0x37f2fc, read from the image; the scale input is drawn by the generator,
/// which includes 1.0 often.
#[test]
fn orig_2308d8_matches_the_game() {
    const S: Spec = spec(0x2308d8, &[Arg::Int(&[0, 256])], 1, Ret::Int, &[(0x37f2fc, 4, false)]);
    run(&S, 6000, |_| {}, |c| {
        let unity = f32::from_bits(r32(&c.globals[0], 0));
        Out::int(scale_alpha(c.float(0), c.ints[0] as u32, unity) as i64)
    });
}

// ---- linked lists --------------------------------------------------------------------------------------------------------------

fn handle(a: u32) -> u32 {
    if a == 0 { 0 } else { (a - ARENA) / SLOT + 1 }
}

/// Append to the list whose head and tail are the globals 0x522188 and 0x52218c, once with an empty list (both words zero) and once
/// with the tail at a second block. The link word (+0x28) is a pointer field, so the harness compares it as a handle.
#[test]
fn orig_16c808_matches_the_game() {
    const NODE: Block = blk(0x30, &[(0x28, 4, Kind::Ptr)]);
    const S: Spec = spec(0x16c808, &[Arg::Ptr(NODE), Arg::Ptr(NODE)], 0, Ret::None, &[(0x522188, 4, false), (0x52218c, 4, false)]);
    for (head, tail) in [(0u32, 0u32), (0x1234, block_addr(1))] {
        run(
            &S,
            1000,
            move |ee| {
                ee.w32(0x522188, head);
                ee.w32(0x52218c, tail);
            },
            |c| {
                let (mut h, mut t) = (r32(&c.globals[0], 0), r32(&c.globals[1], 0));
                let node = block_addr(0);
                let blocks = &mut c.blocks;
                list_append_522188(&mut h, &mut t, node, |addr, v| w32(&mut blocks[((addr - ARENA) / SLOT) as usize], 0x28, handle(v)));
                w32(&mut c.globals[0], 0, h);
                w32(&mut c.globals[1], 0, t);
                Out::none()
            },
        );
    }
}

/// A motion's sync list: the head (+0x94) and tail (+0x98) are pointer fields, the tail points at a second block seven times in eight
/// and is null otherwise (`nulls`); the generator hands handles to the port, which stores the new node's handle.
#[test]
fn orig_17eab8_matches_the_game() {
    const LINK: Block = blk(0xa0, &[(0x9c, 4, Kind::Ptr)]);
    const M: Block = Block { size: 0xa0, fields: &[(0x94, 4, Kind::Ptr), (0x98, 4, Kind::Ptr)], subs: &[(0x98, LINK)] };
    let s = Spec { nulls: true, ..spec(0x17eab8, &[Arg::Ptr(M), Arg::Ptr(LINK)], 0, Ret::None, &[]) };
    run(&s, 2000, |_| {}, |c| {
        let node = c.ints[1] as u32 + 1;
        let mut m = std::mem::take(&mut c.blocks[0]);
        let blocks = &mut c.blocks;
        motion_sync_append(&mut m, node, |addr, v| w32(&mut blocks[(addr - 1) as usize], 0x9c, v));
        c.blocks[0] = m;
        Out::none()
    });
}

// ---- text codes ----------------------------------------------------------------------------------------------------------------

/// Up to 8 codes (fields of the source block are 16-bit integers drawn small a quarter of the time, so the zero and the small-code
/// branches are reached); the destination block is compared whole, so bytes past the terminator must stay as they were.
#[test]
fn orig_1919c8_matches_the_game() {
    const SRC: Block = blk(
        0x40,
        &[(0, 2, Kind::Int), (2, 2, Kind::Int), (4, 2, Kind::Int), (6, 2, Kind::Int), (8, 2, Kind::Int), (10, 2, Kind::Int), (12, 2, Kind::Int), (14, 2, Kind::Int)],
    );
    const S: Spec = spec(0x1919c8, &[Arg::Ptr(SRC), Arg::Range(-1, 8), Arg::Ptr(blk(0x40, &[]))], 0, Ret::None, &[]);
    run(&S, 4000, |_| {}, |c| {
        let src: Vec<u16> = c.blocks[0].chunks(2).map(|h| u16::from_le_bytes([h[0], h[1]])).collect();
        let n = c.int(1);
        text_codes_to_bytes(&src, n, &mut c.blocks[1]);
        Out::none()
    });
}

// ---- vector helpers ------------------------------------------------------------------------------------------------------------

/// The divisor (255.0 in the executable) is the image's word at 0x3792c8 (a non-random global); the vector is compared as floats.
#[test]
fn orig_1530c0_matches_the_game() {
    const S: Spec = spec(
        0x1530c0,
        &[Arg::Ptr(blk(0x10, &[(0, 16, Kind::Float)])), Arg::Int(&[]), Arg::Int(&[]), Arg::Int(&[]), Arg::Int(&[])],
        0,
        Ret::Float,
        &[(0x3792c8, 4, false)],
    );
    run(&S, 2000, |_| {}, |c| {
        let d = f32::from_bits(r32(&c.globals[0], 0));
        let (r, g, b, a) = (c.ints[1] as u8, c.ints[2] as u8, c.ints[3] as u8, c.ints[4] as u8);
        Out::float(colour_bytes_to_unit(c.block(0), r, g, b, a, d))
    });
}

#[test]
fn orig_167078_matches_the_game() {
    const S: Spec = spec(
        0x167078,
        &[
            Arg::Ptr(blk(0x50, &[(0, 8, Kind::Int), (8, 8, Kind::Int), (0x10, 8, Kind::Int), (0x18, 8, Kind::Int), (0x20, 16, Kind::Float), (0x30, 16, Kind::Float), (0x40, 4, Kind::Int)])),
            Arg::Int(&[]),
        ],
        0,
        Ret::None,
        &[],
    );
    run(&S, 2000, |_| {}, |c| {
        let m = c.ints[1];
        mask_block_167078(c.block(0), m);
        Out::none()
    });
}

#[test]
fn orig_21fcd0_matches_the_game() {
    const V: Block = blk(0x10, &[(0, 16, Kind::Float)]);
    const S: Spec = spec(0x21fcd0, &[Arg::Ptr(V), Arg::Ptr(V)], 0, Ret::None, &[]);
    run(&S, 2000, |_| {}, |c| {
        let src = c.blocks[1].clone();
        vec3_copy(c.block(0), &src);
        Out::none()
    });
}

#[test]
fn orig_21ffb8_matches_the_game() {
    const S: Spec = spec(0x21ffb8, &[Arg::Ptr(blk(0x10, &[(0, 16, Kind::Float)]))], 0, Ret::Float, &[]);
    run(&S, 3000, |_| {}, |c| Out::float(vec3_length(&c.blocks[0])));
}

/// The entry count at +0 is drawn by hand (0..=4): a random count would run for ever; floats are finite draws of the generator.
#[test]
fn orig_1b5a18_matches_the_game() {
    let Some(mut ee) = game("oracle test of 0x1b5a18") else { return };
    let mut rng = Rng::new(13);
    let (buf, vec) = (block_addr(0), block_addr(1));
    for n in 0..2000 {
        let mut b: Vec<u8> = (0..0xc0).map(|_| rng.word() as u8).collect();
        for o in (0x20..0xc0).step_by(4) {
            b[o..o + 4].copy_from_slice(&rng.float().to_le_bytes());
        }
        w32(&mut b, 0, rng.below(5) as u32);
        let v: Vec<u8> = (0..4).flat_map(|_| rng.float().to_le_bytes()).collect();
        ee.write(buf, &b);
        ee.write(vec, &v);
        ee.call(0x1b5a18, &[buf as u64, vec as u64], &[], STACK).unwrap();
        add_vec_to_entries(&mut b, &v);
        for o in (0..0xc0).step_by(4) {
            let (g, m) = (ee.r32(buf + o as u32), r32(&b, o));
            let (gf, mf) = (f32::from_bits(g), f32::from_bits(m));
            assert!(g == m || (gf - mf).abs() <= 1e-5 * (1.0 + gf.abs().max(mf.abs())), "case {n}: +{o:#x}: game {gf} port {mf}");
        }
    }
}

/// The vector is copied to obj +0x10 and normalised; the original's return value (a VU register nobody reads) is not compared.
#[test]
fn orig_1824e0_matches_the_game() {
    const S: Spec = spec(
        0x1824e0,
        &[Arg::Ptr(blk(0x20, &[(0x10, 16, Kind::Float)])), Arg::Ptr(blk(0x10, &[(0, 16, Kind::Float)]))],
        1,
        Ret::None,
        &[],
    );
    run(&S, 3000, |_| {}, |c| {
        let src = c.blocks[1].clone();
        let w = c.float(0);
        normalize_into(c.block(0), &src, w);
        Out::none()
    });
}

/// The global at 0x359330 holds a pointer: in one run the address of the test's block, in the other null.
#[test]
fn orig_15db30_matches_the_game() {
    const S: Spec = spec(0x15db30, &[Arg::Ptr(blk(0x20, &[]))], 0, Ret::None, &[(0x359330, 4, false)]);
    for ptr in [block_addr(0), 0] {
        run(&S, 500, move |ee| ee.w32(0x359330, ptr), |c| {
            let set = r32(&c.globals[0], 0) != 0;
            global_object_set_flag_10(set.then(|| c.block(0)));
            Out::none()
        });
    }
}
