//! High-level emulation (HLE) around `kh2-ee`: runs the user's own game executable headless as a
//! reference runtime. Nothing of the game is stored here; the executable and overlays are read from
//! the user's disc at run time.
//!
//! * [`kernel`]: the EE kernel in Rust (boot contract `HYB-boot` sections 2.1 to 2.3): the syscall
//!   trap, threads with a priority scheduler, semaphores, INTC and DMAC handler tables with interrupt
//!   delivery at instruction boundaries, and a virtual clock (VBlank at 59.94 Hz, timer 2
//!   overflow, `DelayThread` wake-ups).
//! * [`iop`]: the IOP side in Rust (HYB-4): hooks on the SIF RPC client functions, the servers
//!   (loadfile, the cdvd ids over the user's disc, the pad library, memory card, sound driver), the
//!   sound command ring decoder and the pad script. With the kernel it boots the executable to
//!   the request for `ovl_title.x`.
//! * [`hw`]: the memory-mapped register pages (HYB-5): DMA completion and interrupts, scratchpad
//!   copies, GS CSR, timer 0, the unimplemented-register check, and the movie hook.
//!
//! # What is HLE'd
//!
//! The 39 syscall numbers the traced boot-to-title and field runs reach, with the values the game
//! expects back, plus the remaining thread and semaphore calls of the SDK (`ExitThread`,
//! `TerminateThread`, `SleepThread`/`WakeupThread`, `SuspendThread`/`ResumeThread`,
//! `ReleaseWaitThread`, `DeleteThread`, `PollSema`, `EndOfHeap`, `GetMemorySize`). libkernel's
//! kernel patches (`SetSyscall`, the 0x5a and 0x83 slots, `GetEntryAddress`) are accepted and
//! ignored (D4): `SetSyscall` records the entry in a table in low RAM (0x70000) and the two
//! kernel-mode user routines libkernel installs are done in Rust (slot 0x5a copies memory, slot 0x83
//! searches it for a word), so its search for the kernel's syscall table (0x321cd8) finds the table. A syscall with no implementation is recorded in `Kernel::unimplemented` and
//! returns 0.
//!
//! # What is `[inferred]`
//!
//! The initial stack pointer returned by `SetupThread` (top of stack minus 0x2a0), the argument
//! block written for `main` (argc 1, a path string), the main thread's initial priority (0),
//! the language word of `GetOsdConfigParam` (English), the address of the interrupt stack and of
//! the thread exit stub in low RAM, the SIF register defaults, the `ThreadParam` and `SemaParam`
//! layouts (SDK headers), handler ordering for `AddIntcHandler`'s `next` argument, the timer 2
//! overflow period (8.79 Hz from a 16-bit counter at bus clock / 256) and CPI 1 for the virtual
//! clock. Each is labelled where it is written.

pub mod bringup;
pub mod hw;
pub mod iop;
pub mod kernel;

pub use kernel::{Kernel, Machine, RunStop};
