//! Thread control blocks. Layouts follow the PS2 SDK `ThreadParam` / `ee_thread_status_t` [SDK]:
//! status 0x00, entry 0x04, stack 0x08, stack size 0x0c, gp 0x10, initial priority 0x14, current
//! priority 0x18, attr 0x1c, option 0x20, then (status only) wait type 0x24, wait id 0x28,
//! wakeup count 0x2c.

use kh2_ee::Context;

/// Scheduling state apart from the suspend flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    Dormant,
    Ready,
    Running,
    Waiting,
}

/// What a waiting thread waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    None,
    Sema(u32),
    Sleep,
    Delay,
}

/// `THS_*` status bits of `ReferThreadStatus` [SDK].
pub const THS_RUN: u32 = 0x01;
pub const THS_READY: u32 = 0x02;
pub const THS_WAIT: u32 = 0x04;
pub const THS_SUSPEND: u32 = 0x08;
pub const THS_DORMANT: u32 = 0x10;

/// `TSW_*` wait types [SDK].
pub const TSW_SLEEP: u32 = 1;
pub const TSW_SEMA: u32 = 2;

pub struct Thread {
    pub id: u32,
    pub run: Run,
    pub suspended: bool,
    pub wait: Wait,
    pub entry: u32,
    pub stack: u32,
    pub stack_size: u32,
    pub gp: u32,
    pub init_prio: u32,
    pub prio: u32,
    pub attr: u32,
    pub option: u32,
    pub wakeups: u32,
    /// Queue position among ready threads: smaller runs first within a priority. A preempted
    /// thread takes a position before all others (it stays at the head of its queue).
    pub seq: i64,
    pub ctx: Context,
}

impl Thread {
    /// The `THS_*` value (`ReferThreadStatus`).
    pub fn status(&self) -> u32 {
        let base = match self.run {
            Run::Dormant => return THS_DORMANT,
            Run::Running => THS_RUN,
            Run::Ready => THS_READY,
            Run::Waiting => THS_WAIT,
        };
        if self.suspended { base | THS_SUSPEND } else { base }
    }

    pub fn wait_type(&self) -> u32 {
        match self.wait {
            Wait::None => 0,
            Wait::Sema(_) => TSW_SEMA,
            Wait::Sleep | Wait::Delay => TSW_SLEEP,
        }
    }

    pub fn wait_id(&self) -> u32 {
        match self.wait {
            Wait::Sema(s) => s,
            _ => 0,
        }
    }

    /// Whether the scheduler may pick it.
    pub fn runnable(&self) -> bool {
        self.run == Run::Ready && !self.suspended
    }
}

/// A zeroed register file (the state of a thread before it starts).
pub fn blank_context() -> Context {
    let mut vf = [[0.0f32; 4]; 32];
    vf[0] = [0.0, 0.0, 0.0, 1.0];
    let mut vi = [0u32; 32];
    vi[20] = 0x3F80_0000;
    Context {
        gpr: [0; 32],
        hi: 0,
        lo: 0,
        sa: 0,
        pc: 0,
        npc: 4,
        fpr: [0.0; 32],
        facc: 0.0,
        fcr31: 0,
        vf,
        vi,
        vacc: [0.0; 4],
        q: 0.0,
    }
}

/// Sets the low 64 bits of a saved register to a sign-extended 32-bit value, keeping the upper half.
pub fn set_reg32(ctx: &mut Context, reg: usize, v: i32) {
    ctx.gpr[reg] = (ctx.gpr[reg] & !(u64::MAX as u128)) | (v as i64 as u64) as u128;
}
