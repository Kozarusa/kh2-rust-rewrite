//! `factory scan` classifies every function of the executable and its overlays (writes `local/factory/scan.tsv`);
//! `factory brief <addr>` prints one function's scan and the oracle spec inferred from it; `factory gen <addr> <port>
//! <out.rs>` writes an oracle fuzz test that calls the Rust function path `<port>`; `factory bench` measures kh2-ee. Statuses
//! (ported, verified) come from `kh2 ledger scan`.
//!
//! Inputs: the executable (`KH2_EXE`, or the disc via `KH2_ISO` / `local/iso.txt`), the function sheets and overlay
//! images of the reverse-engineering workspace (`KH2_PS2`, default `../kh2_ps2`). Output stays under `local/`.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use kh2_factory::scan::{self, Probe, Scan};
use kh2_factory::{FnRow, Image};

fn load() -> (Image, Vec<(FnRow, usize)>, Vec<Image>) {
    let elf = kh2_factory::exe().expect("no executable: set KH2_EXE or KH2_ISO");
    let main = Image::from_elf(&elf).expect("elf");
    let mut rows = Vec::new();
    let mut ovls = Vec::new();
    for r in kh2_factory::sheet("main").expect("functions_main.json (set KH2_PS2)") {
        rows.push((r, 0));
    }
    for i in 0..4 {
        let Some(sheet) = kh2_factory::sheet(&format!("ovl{i}")) else { continue };
        let dir = kh2_factory::ps2_root().join("input").join("ovl");
        let bin = std::fs::read_dir(&dir)
            .ok()
            .and_then(|rd| rd.flatten().find(|e| e.file_name().to_string_lossy().starts_with(&format!("ovl{i}_"))))
            .and_then(|e| std::fs::read(e.path()).ok());
        let Some(bin) = bin else { continue };
        // Overlays load at 0x1f18000 (one at a time); the main executable stays mapped for calls into it.
        let mut segs = vec![(0x01f1_8000, bin)];
        segs.extend(main.segs.iter().cloned());
        ovls.push(Image { segs, data_end: main.data_end });
        let k = ovls.len();
        rows.extend(sheet.into_iter().map(|r| (r, k)));
    }
    (main, rows, ovls)
}

fn hexset(s: &std::collections::BTreeSet<u32>) -> String {
    s.iter().map(|a| format!("{a:x}")).collect::<Vec<_>>().join(" ")
}

fn scan_one(img: &Image, addr: u32, size: u32, probe: &mut Probe) -> Scan {
    scan::scan(addr, size, &|pc| img.word(pc), probe)
}

fn cmd_scan() {
    let (main, rows, ovls) = load();
    let starts: HashSet<u32> = rows.iter().filter(|r| r.1 == 0).map(|r| r.0.addr).collect();
    let (code, docs) = kh2_factory::citations(&starts);
    eprintln!("cited: {} in code, {} in docs, {} either", code.len(), docs.len(), code.union(&docs).count());
    let mut probe = Probe::new();
    let mut unimpl = String::from("image	addr	pc	word
");
    let t0 = std::time::Instant::now();
    let mut out = String::from(
        "image\taddr\tsize\tsheet_callees\tcalls\ttail\tjalr\tjr_ind\tsyscall\tcop0\tcache\tvu0\tvu_flags\tunimpl\t\
         st_stack\tst_arg\tst_gstatic\tst_gindexed\tst_gobject\tst_hw\tst_unk\t\
         ld_stack\tld_arg\tld_gstatic\tld_gindexed\tld_gobject\tld_hw\tld_unk\t\
         glob_st\tglob_ld\tint_args\tfloat_args\tret\tfields\targ_indexed\tcited_code\tcited_docs\tsystem\n",
    );
    for (r, k) in &rows {
        let img = if *k == 0 { &main } else { &ovls[*k - 1] };
        let s = scan_one(img, r.addr, r.size, &mut probe);
        if s.unimpl > 0 {
            for pc in (r.addr..r.addr + r.size).step_by(4) {
                if let Some(w) = img.word(pc)
                    && !probe.supported(w)
                {
                    writeln!(unimpl, "{}	{:x}	{pc:x}	{w:08x}", r.image, r.addr).unwrap();
                }
            }
        }
        let a = |x: &scan::Access| format!("{}\t{}\t{}\t{}\t{}\t{}\t{}", x.stack, x.arg, x.glob_static, x.glob_indexed, x.glob_object, x.hw, x.unk);
        let main_img = *k == 0;
        writeln!(
            out,
            "{}\t{:x}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.image,
            r.addr,
            r.size,
            r.callees.len(),
            hexset(&s.calls),
            hexset(&s.tail),
            s.jalr,
            s.jr_ind,
            s.syscall,
            s.cop0,
            s.cache,
            s.vu0,
            s.vu_flags,
            s.unimpl,
            a(&s.st),
            a(&s.ld),
            hexset(&s.glob_st),
            hexset(&s.glob_ld),
            s.int_args,
            s.float_args,
            s.ret,
            s.fields.len(),
            s.arg_indexed,
            (main_img && code.contains(&r.addr)) as u8,
            (main_img && docs.contains(&r.addr)) as u8,
            r.system.clone().unwrap_or_default(),
        )
        .unwrap();
    }
    let dir = kh2_factory::repo_root().join("local").join("factory");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("scan.tsv"), out).unwrap();
    std::fs::write(dir.join("unimpl.tsv"), unimpl).unwrap();
    eprintln!("scanned {} functions in {:.1} s -> local/factory/scan.tsv", rows.len(), t0.elapsed().as_secs_f64());
}

fn find(rows: &[(FnRow, usize)], addr: u32) -> &(FnRow, usize) {
    rows.iter().find(|r| r.0.addr == addr && r.1 == 0).unwrap_or_else(|| panic!("no function at {addr:#x}"))
}

/// The oracle spec inferred from a scan, as Rust source.
fn spec_src(s: &Scan) -> String {
    let nargs = 8 - (s.int_args as u32).leading_zeros().saturating_sub(24);
    let mut args = Vec::new();
    for a in 0..nargs as u8 {
        let fields: Vec<_> = s.fields.iter().filter(|(k, _)| k.0 == a && k.1 == -1).collect();
        if fields.is_empty() {
            let consts: Vec<String> = s.consts.iter().filter(|c| c.unsigned_abs() < 0x10000).map(|c| c.to_string()).collect();
            args.push(format!("Arg::Int(&[{}])", consts.join(", ")));
        } else {
            let size = fields.iter().map(|(k, f)| k.2 + f.width as i32).max().unwrap_or(4).max(4);
            let size = (size + 15) & !15;
            let fl: Vec<String> = fields
                .iter()
                .filter(|(k, _)| k.2 >= 0)
                .map(|(k, f)| format!("({:#x}, {}, Kind::{})", k.2, f.width, if f.pointer { "Ptr" } else if f.float { "Float" } else { "Int" }))
                .collect();
            let subs: Vec<String> = fields
                .iter()
                .filter(|(_, f)| f.pointer)
                .map(|(k, _)| {
                    let sub: Vec<_> = s.fields.iter().filter(|(kk, _)| kk.0 == a && kk.1 == k.2).collect();
                    let ss = sub.iter().map(|(kk, f)| kk.2 + f.width as i32).max().unwrap_or(16).max(4);
                    let sf: Vec<String> = sub
                        .iter()
                        .filter(|(kk, _)| kk.2 >= 0)
                        .map(|(kk, f)| format!("({:#x}, {}, Kind::{})", kk.2, f.width, if f.float { "Float" } else { "Int" }))
                        .collect();
                    format!("({:#x}, Block {{ size: {:#x}, fields: &[{}], subs: &[] }})", k.2, (ss + 15) & !15, sf.join(", "))
                })
                .collect();
            args.push(format!("Arg::Ptr(Block {{ size: {size:#x}, fields: &[{}], subs: &[{}] }})", fl.join(", "), subs.join(", ")));
        }
    }
    let nf = 8 - (s.float_args as u32).leading_zeros().saturating_sub(24);
    // The decompiler's signature knows the return type better than "f0 or v0 written" (temporaries use both).
    let sig = std::fs::read_to_string(kh2_factory::ps2_root().join("decomp").join("main").join(format!("{:08x}.c", s.addr)))
        .ok()
        .and_then(|t| t.lines().find(|l| l.contains(&format!("FUN_{:08x}(", s.addr)) && !l.starts_with("//")).map(String::from));
    let ret = if let Some(sig) = &sig {
        let t = sig.split_whitespace().next().unwrap_or("");
        match t {
            "void" => "Ret::None",
            "float" => "Ret::Float",
            "undefined8" | "long" | "ulong" => "Ret::Int64",
            _ => "Ret::Int",
        }
    } else if s.ret & 4 != 0 && s.ret & 1 == 0 {
        "Ret::Float"
    } else if s.ret & 1 != 0 {
        "Ret::Int"
    } else {
        "Ret::None"
    };
    let globals: Vec<String> = s.glob_st.iter().map(|g| format!("({g:#x}, 4, true)")).collect();
    format!(
        "Spec {{\n    addr: {:#x},\n    args: &[{}],\n    floats: {nf},\n    ret: {ret},\n    globals: &[{}],\n    stubs: &[],\n    nulls: true,\n}}",
        s.addr,
        args.join(", "),
        globals.join(", ")
    )
}

fn cmd_brief(addr: u32) {
    let (main, rows, _) = load();
    let (r, _) = find(&rows, addr);
    let s = scan_one(&main, r.addr, r.size, &mut Probe::new());
    let ids: HashMap<&str, u32> = rows.iter().map(|(r, _)| (r.id.as_str(), r.addr)).collect();
    let names = |v: &[String]| v.iter().filter_map(|i| ids.get(i.as_str())).map(|a| format!("{a:#x}")).collect::<Vec<_>>().join(" ");
    println!("function {:#x} ({} bytes, {} instructions), system {:?}", r.addr, r.size, r.size / 4, r.system);
    println!("callers: {}", names(&r.callers));
    println!("callees: {} (scan: calls {:x?}, tail {:x?}, indirect calls {}, jump tables {})", names(&r.callees), s.calls, s.tail, s.jalr, s.jr_ind);
    println!("int args read: {:08b} (a0 = bit 0), float args read: {:08b} (f12 = bit 0), returns v0/v1/f0: {:03b}", s.int_args, s.float_args, s.ret);
    println!("stores: {:?}", s.st);
    println!("loads:  {:?}", s.ld);
    println!("globals stored {:x?}, loaded {:x?}", s.glob_st, s.glob_ld);
    println!("vu0 {} (flag reads {}), unimplemented in kh2-ee {}, syscall {}, cop0 {}", s.vu0, s.vu_flags, s.unimpl, s.syscall, s.cop0);
    for (k, f) in &s.fields {
        let at = if k.1 < 0 { format!("arg{}+{:#x}", k.0, k.2) } else { format!("arg{}->[{:#x}]+{:#x}", k.0, k.1, k.2) };
        println!("  field {at}: {f:?}");
    }
    println!("decomp (local only): {}", kh2_factory::ps2_root().join("decomp_typed").join("main").join(format!("{:08x}.c", r.addr)).display());
    println!("\n{}", spec_src(&s));
}

fn cmd_gen(addr: u32, port: &str, out: &str) {
    let (main, rows, _) = load();
    let (r, _) = find(&rows, addr);
    let s = scan_one(&main, r.addr, r.size, &mut Probe::new());
    let spec = spec_src(&s);
    let nargs = 8 - (s.int_args as u32).leading_zeros().saturating_sub(24);
    let nf = 8 - (s.float_args as u32).leading_zeros().saturating_sub(24);
    let mut call = Vec::new();
    let mut blocks = 0;
    for a in 0..nargs as u8 {
        if s.fields.keys().any(|k| k.0 == a && k.1 == -1) {
            call.push(format!("c.block({blocks})"));
            blocks += 1;
        } else {
            call.push(format!("c.int({a})"));
        }
    }
    for f in 0..nf {
        call.push(format!("c.float({f})"));
    }
    let wrap = if spec.contains("Ret::Float") {
        format!("Out::float({port}({}))", call.join(", "))
    } else if spec.contains("Ret::Int") {
        format!("Out::int({port}({}) as i64)", call.join(", "))
    } else {
        format!("{{\n        {port}({});\n        Out::none()\n    }}", call.join(", "))
    };
    let src = format!(
        "//! Oracle fuzz test for the original function at {addr:#x} (generated by `factory gen`; the spec is the static\n\
         //! scan's inference: fix it here if the original faults or writes past a block). The port is `{port}`.\n\
         //! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.\n\n\
         use kh2_factory::fuzz::*;\n\n\
         const SPEC: Spec = {spec};\n\n\
         #[test]\n\
         fn orig_{addr:06x}_matches_the_game() {{\n    \
         let Some(st) = fuzz(&SPEC, 4000, |c| {wrap}) else {{ return }};\n    \
         eprintln!(\"{{}} cases equal, {{}} interpreter steps in {{:.3}} s\", st.cases, st.steps, st.secs);\n\
         }}\n"
    );
    std::fs::write(out, src).unwrap();
    eprintln!("wrote {out}");
}

fn cmd_bench() {
    let elf = kh2_factory::exe().expect("executable");
    let mut ee = kh2_ee::Ee::new();
    ee.load_elf(&elf).unwrap();
    // A tight loop: memset-like work through the game's own code would need its address; use a synthetic loop in the
    // interpreter instead (addiu/bne), then a real routine: the Euler rotation at 0x11b568 (FPU and VU0 work).
    let code = [0x2408_0000u32, 0x3409_ffff, 0x2508_0001, 0x1509_fffe, 0, 0x03e0_0008, 0];
    for (i, w) in code.iter().enumerate() {
        ee.w32(0x0010_0000 - 0x100 + 4 * i as u32, *w);
    }
    let t = std::time::Instant::now();
    let s0 = ee.steps;
    for _ in 0..200 {
        ee.call(0x0010_0000 - 0x100, &[], &[], 0x01ff_0000).unwrap();
    }
    let dt = t.elapsed().as_secs_f64();
    println!("synthetic loop: {} steps in {:.3} s = {:.1} M instructions/s", ee.steps - s0, dt, (ee.steps - s0) as f64 / dt / 1e6);
    let (m, a) = (0x0190_0000u32, 0x0190_0100u32);
    let t = std::time::Instant::now();
    let s0 = ee.steps;
    for i in 0..100_000 {
        for k in 0..16 {
            ee.wf32(m + 4 * k, if k % 5 == 0 { 1.0 } else { 0.0 });
        }
        for k in 0..3 {
            ee.wf32(a + 4 * k, (i as f32 * 0.001) + k as f32);
        }
        ee.call(0x0011_b568, &[m as u64, a as u64], &[], 0x01ff_0000).unwrap();
    }
    let dt = t.elapsed().as_secs_f64();
    println!("0x11b568 x 100000: {} steps in {:.3} s = {:.1} M instructions/s ({:.1} us a call)", ee.steps - s0, dt, (ee.steps - s0) as f64 / dt / 1e6, dt * 10.0);
    let t = std::time::Instant::now();
    for _ in 0..20 {
        let mut e = kh2_ee::Ee::new();
        e.load_elf(&elf).unwrap();
    }
    println!("Ee::new + load_elf: {:.1} ms each", t.elapsed().as_secs_f64() * 50.0);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let hex = |s: &str| u32::from_str_radix(s.trim_start_matches("0x").replace('_', "").as_str(), 16).expect("hex address");
    match args.first().map(String::as_str) {
        Some("scan") => cmd_scan(),
        Some("brief") => cmd_brief(hex(&args[1])),
        Some("gen") => cmd_gen(hex(&args[1]), &args[2], &args[3]),
        Some("bench") => cmd_bench(),
        _ => eprintln!("usage: factory scan | brief <addr> | gen <addr> <rust::port::path> <out.rs> | bench (statuses: kh2 ledger scan)"),
    }
}
