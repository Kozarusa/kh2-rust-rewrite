//! The user's executable boots through crt0 and libkernel on the Rust kernel (boot contract 1.1).
//! Skipped without the disc.

use std::cell::Cell;
use std::rc::Rc;

use kh2_ee::Ee;
use kh2_hle::kernel::Run;
use kh2_hle::{Machine, RunStop};

fn flag(m: &mut Machine, addr: u32) -> Rc<Cell<u64>> {
    let seen = Rc::new(Cell::new(u64::MAX));
    let s = seen.clone();
    m.ee.probes.insert(addr, move |ee: &mut Ee| {
        if s.get() == u64::MAX {
            s.set(ee.steps);
        }
    });
    seen
}

#[test]
fn the_executable_boots_to_the_iop_boot_call_on_the_rust_kernel() {
    let Some(elf) = kh2_factory::exe_or_skip("kernel boot") else { return };
    let mut m = Machine::new();
    let entry = m.load_elf(&elf).expect("elf");
    assert_eq!(entry, 0x10001c);
    m.install_usa_hooks();
    let libkernel_init = flag(&mut m, 0x321de8);
    let ctors = flag(&mut m, 0x32c470);
    let main = flag(&mut m, 0x101d68);
    let iop_boot = Rc::new(Cell::new(None));
    let seen = iop_boot.clone();
    m.ee.probes.insert(0x166ee8, move |ee: &mut Ee| {
        seen.set(Some(ee.steps));
        ee.request_stop();
    });
    let r = m.run(400_000_000).expect("no fault");
    let k = m.kernel.borrow();
    assert_eq!(r, RunStop::Requested, "stopped at the probe");
    assert!(iop_boot.get().is_some());
    assert!(libkernel_init.get() < ctors.get() && ctors.get() < main.get() && main.get() < iop_boot.get().unwrap());
    assert!(k.thread(1).is_some() && k.thread(2).is_some());
    // Thread 2 is made by libkernel's init (entry 0x3190a0) and is not started before `main` [V: this run; the trace
    // lists "create" for threads 1 and 2 and no "start"]. The boot reaches the IOP call well inside 20 million
    // instructions (measured: 14.3 million).
    let t2 = k.thread(2).expect("thread 2");
    assert_eq!((t2.run, t2.entry, t2.prio), (Run::Dormant, 0x3190a0, 0));
    assert_eq!(k.trace, vec![("create", 1), ("create", 2)]);
    let steps = iop_boot.get().unwrap();
    assert!((10_000_000..20_000_000).contains(&steps), "steps to the IOP boot call: {steps}");
    assert!(!k.intr.intc[11].is_empty(), "INTC 11 (alarm clock) registered");
    assert_eq!(k.intr.intc[11][0].addr, 0x3229e0);
    assert!(k.unimplemented.is_empty());
}
