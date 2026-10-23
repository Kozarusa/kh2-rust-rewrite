//! The virtual clock (decision D3 of the boot contract). Time is the number of EE cycles since
//! reset, taken as retired instructions [inferred CPI 1] plus the cycles skipped while every
//! thread was blocked. Nothing reads the host clock, so runs are deterministic.

/// EE core clock in Hz (294.912 MHz).
pub const EE_HZ: u64 = 294_912_000;
/// Cycles between VBlank starts: 294.912 MHz / 59.94 Hz [inferred from the NTSC field rate].
pub const VBLANK_CYCLES: u64 = 4_920_120;
/// Cycles between timer 2 overflows: a 16-bit counter at bus clock / 256 (147.456 MHz / 256 /
/// 65536 = 8.79 Hz, observed 8.8 per second in a field run) [V count, inferred clock].
pub const T2_CYCLES: u64 = 65_536 * 256 * 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// INTC cause 2.
    VBlank,
    /// INTC cause 11 (the libkernel alarm clock).
    T2Overflow,
    /// A sleeping thread's `DelayThread` time is up.
    Wake(u32),
}

pub struct Clock {
    /// Cycles skipped while idle.
    pub skipped: u64,
    /// Pending events, kept sorted by time (insertion order within equal times).
    events: Vec<(u64, Event)>,
    pub vsyncs: u64,
    pub t2_overflows: u64,
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock {
    pub fn new() -> Clock {
        let mut c = Clock { skipped: 0, events: Vec::new(), vsyncs: 0, t2_overflows: 0 };
        c.schedule(VBLANK_CYCLES, Event::VBlank);
        c.schedule(T2_CYCLES, Event::T2Overflow);
        c
    }

    /// Current time in cycles given the interpreter's retired instruction count.
    pub fn now(&self, steps: u64) -> u64 {
        steps + self.skipped
    }

    pub fn schedule(&mut self, at: u64, ev: Event) {
        let i = self.events.partition_point(|e| e.0 <= at);
        self.events.insert(i, (at, ev));
    }

    /// Time of the earliest pending event.
    pub fn next_at(&self) -> Option<u64> {
        self.events.first().map(|e| e.0)
    }

    /// Removes and returns the earliest event if it is due at `now`. Periodic events re-arm
    /// themselves from their own due time, so they do not drift.
    pub fn pop_due(&mut self, now: u64) -> Option<(u64, Event)> {
        if self.events.first()?.0 > now {
            return None;
        }
        let (at, ev) = self.events.remove(0);
        match ev {
            Event::VBlank => {
                self.vsyncs += 1;
                self.schedule(at + VBLANK_CYCLES, ev);
            }
            Event::T2Overflow => {
                self.t2_overflows += 1;
                self.schedule(at + T2_CYCLES, ev);
            }
            Event::Wake(_) => {}
        }
        Some((at, ev))
    }

    /// Drops the pending wake-up of a thread.
    pub fn cancel_wake(&mut self, tid: u32) {
        self.events.retain(|e| e.1 != Event::Wake(tid));
    }

    /// Microseconds to cycles.
    pub fn usec_to_cycles(usec: u64) -> u64 {
        usec * (EE_HZ / 1000) / 1000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_come_out_in_time_order_and_periodic_ones_rearm() {
        let mut c = Clock::new();
        c.schedule(100, Event::Wake(3));
        c.schedule(50, Event::Wake(2));
        assert_eq!(c.next_at(), Some(50));
        assert_eq!(c.pop_due(49), None, "nothing is due before its time");
        assert_eq!(c.pop_due(60), Some((50, Event::Wake(2))));
        assert_eq!(c.pop_due(100), Some((100, Event::Wake(3))));
        assert_eq!(c.pop_due(VBLANK_CYCLES - 1), None);
        assert_eq!(c.pop_due(VBLANK_CYCLES), Some((VBLANK_CYCLES, Event::VBlank)));
        assert_eq!(c.vsyncs, 1);
        // The next VBlank is exactly one period later, however late the first was served.
        assert_eq!(c.pop_due(3 * VBLANK_CYCLES), Some((2 * VBLANK_CYCLES, Event::VBlank)));
        assert_eq!(c.pop_due(3 * VBLANK_CYCLES), Some((3 * VBLANK_CYCLES, Event::VBlank)));
        assert_eq!(c.vsyncs, 3);
    }

    #[test]
    fn t2_overflows_about_8_8_times_a_second_and_idle_time_counts() {
        let rate = EE_HZ as f64 / T2_CYCLES as f64;
        assert!((rate - 8.79).abs() < 0.01, "boot contract 2.3: 8.79 Hz, got {rate}");
        let mut c = Clock::new();
        c.skipped = 10_000;
        assert_eq!(c.now(5), 10_005);
        assert_eq!(Clock::usec_to_cycles(1_000_000), EE_HZ);
        c.cancel_wake(7);
        c.schedule(20, Event::Wake(7));
        c.cancel_wake(7);
        assert_eq!(c.next_at(), Some(VBLANK_CYCLES), "the cancelled wake-up is gone");
    }
}
