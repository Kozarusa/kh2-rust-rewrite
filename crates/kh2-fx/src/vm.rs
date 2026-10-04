//! The particle program runtime: one [`Effect`] per started PAX element.
//!
//! Behaviour follows the game's effect manager (studied in a local decompilation of the user's
//! executable, nothing copied):
//! * Time is fixed point, [`TICK`] = one 60 Hz frame. An effect runs one tick per frame.
//! * A group's objects are born once the effect's time reaches the group's start frame.
//! * Every tick, programs without a draw handler run first, then programs with one (update, then
//!   draw); then each object's frame advances, and objects past their loop end restart (fast-
//!   forwarding to the loop start) or, past their total frame, die.
//! * Each program keeps a cursor into its parameter list. Before a program runs, the cursor moves
//!   to the next entry when that entry's frame equals the object's frame; handlers then apply the
//!   current entry only on the exact frame it names.
//! * Objects are byte buffers laid out like the game's: frame at 0x0c, local matrix at 0x10,
//!   draw matrix at 0x50, work memory from 0xa0 (addressed by each program's useVal offsets).

use std::sync::Arc;

use glam::{Mat4, Vec3, Vec4};

use crate::pax::{Dpd, Group, NEVER, ParticleData, TICK};
use crate::prog;

pub const OBJ_FRAME: usize = 0x0c;
pub const OBJ_LOCAL: usize = 0x10;
pub const OBJ_DRAW: usize = 0x50;
pub const OBJ_WORK: usize = 0xa0;

/// Per-frame inputs from the game world.
#[derive(Clone, Debug)]
pub struct Env {
    /// Owner placement (bone or object world matrix, scaled), the effect's "bind" space.
    pub bind: Mat4,
    /// World to camera.
    pub view: Mat4,
    /// Camera to world.
    pub view_inv: Mat4,
    /// Effect tint (x, y, z, w = r, g, b, a; 1.0 = unchanged).
    pub tint: Vec4,
    /// Two-point effects (weapon trails): the (base, tip) pair in world space.
    pub two_point: Option<[Vec3; 2]>,
    /// Column lengths of the bind matrix (the game's per-effect scale).
    pub bind_scale: Vec3,
}

impl Default for Env {
    fn default() -> Env {
        Env {
            bind: Mat4::IDENTITY,
            view: Mat4::IDENTITY,
            view_inv: Mat4::IDENTITY,
            tint: Vec4::ONE,
            two_point: None,
            bind_scale: Vec3::ONE,
        }
    }
}

/// What the draw handlers produced this tick, in world space.
#[derive(Clone, Debug)]
pub enum Draw {
    /// A textured quad strip from a DPD model (weapon trails).
    Strip(Strip),
}

#[derive(Clone, Debug)]
pub struct Strip {
    pub dpd: usize,
    pub model: usize,
    /// Quads as 4 corners each: (position, rgba 0..1, uv).
    pub quads: Vec<[(Vec3, Vec4, [f32; 2]); 4]>,
    pub texture: Option<usize>,
    /// GS ALPHA register value from the model (0x48 = additive).
    pub alpha_mode: u8,
}

pub(crate) struct Obj {
    pub group: usize,
    pub mem: Vec<u8>,
    /// Byte offset of each program's current parameter entry.
    pub cursors: Vec<usize>,
    /// Born this tick: skip the frame advance once.
    pub fresh: bool,
    pub restart: bool,
    /// Weapon-trail sample ring (the game allocates it from the effect heap).
    pub ring: Vec<[Vec3; 2]>,
}

impl Obj {
    pub fn frame(&self) -> i32 {
        self.i32(OBJ_FRAME)
    }
    pub fn i32(&self, o: usize) -> i32 {
        i32::from_le_bytes(self.mem[o..o + 4].try_into().unwrap())
    }
    pub fn set_i32(&mut self, o: usize, v: i32) {
        self.mem[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&self, o: usize) -> f32 {
        f32::from_bits(self.i32(o) as u32)
    }
    pub fn set_f32(&mut self, o: usize, v: f32) {
        self.set_i32(o, v.to_bits() as i32);
    }
    pub fn i16(&self, o: usize) -> i16 {
        i16::from_le_bytes([self.mem[o], self.mem[o + 1]])
    }
    pub fn set_i16(&mut self, o: usize, v: i16) {
        self.mem[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }
    pub fn vec3(&self, o: usize) -> Vec3 {
        Vec3::new(self.f32(o), self.f32(o + 4), self.f32(o + 8))
    }
    pub fn set_vec3(&mut self, o: usize, v: Vec3) {
        self.set_f32(o, v.x);
        self.set_f32(o + 4, v.y);
        self.set_f32(o + 8, v.z);
    }
    pub fn mat(&self, o: usize) -> Mat4 {
        let mut c = [0f32; 16];
        for (i, x) in c.iter_mut().enumerate() {
            *x = self.f32(o + 4 * i);
        }
        Mat4::from_cols_array(&c)
    }
    pub fn set_mat(&mut self, o: usize, m: Mat4) {
        for (i, x) in m.to_cols_array().iter().enumerate() {
            self.set_f32(o + 4 * i, *x);
        }
    }
}

/// Program-handler context: the effect-wide state handlers may touch.
pub(crate) struct Ctx<'a> {
    pub env: &'a Env,
    pub dpd: &'a Dpd,
    pub dpd_index: usize,
    pub draws: &'a mut Vec<Draw>,
    pub rng: &'a mut u32,
    /// Running effect (the game skips value updates while the effect is paused).
    pub running: bool,
}

struct GroupRt {
    start: i32,
    count: u16,
    last: Option<usize>,
}

pub struct Effect {
    pd: Arc<ParticleData>,
    dpd: Arc<Dpd>,
    dpd_index: usize,
    time: i32,
    looping: bool,
    ending: bool,
    groups: Vec<GroupRt>,
    objs: Vec<Option<Obj>>,
    /// Object draw order (the game's priority-sorted object list).
    order: Vec<usize>,
    rng: u32,
    pub paused: bool,
    pub done: bool,
}

impl Effect {
    pub fn new(dpd: Arc<Dpd>, dpd_index: usize, particle_data: usize) -> Option<Effect> {
        let pd = Arc::new(dpd.particles.get(particle_data)?.clone());
        let groups = pd.groups.iter().map(|g| GroupRt { start: g.start_frame, count: 0, last: None }).collect();
        Some(Effect {
            looping: pd.flags & 1 != 0,
            pd,
            dpd,
            dpd_index,
            time: 0,
            ending: false,
            groups,
            objs: Vec::new(),
            order: Vec::new(),
            rng: 0x3f80_1234,
            paused: false,
            done: false,
        })
    }

    pub fn particle_data(&self) -> &ParticleData {
        &self.pd
    }

    /// The game's loop_end: stop looping; objects finish at their total frame.
    pub fn end(&mut self) {
        self.ending = true;
    }

    pub fn is_ending(&self) -> bool {
        self.ending
    }

    pub fn alive_objects(&self) -> usize {
        self.order.len()
    }

    /// One 60 Hz tick. Returns this tick's draws.
    pub fn tick(&mut self, env: &Env) -> Vec<Draw> {
        let mut draws = Vec::new();
        if self.done {
            return draws;
        }
        self.birth();
        self.run_programs(env, false, &mut draws);
        self.run_programs(env, true, &mut draws);
        if !self.paused {
            self.advance_frames();
            self.expire(env, &mut draws);
            self.time = self.time.saturating_add(TICK);
            self.done = if !self.looping {
                self.time >= self.pd.life
            } else if self.ending {
                self.order.is_empty()
            } else {
                if self.time > 0x6fff_ffff {
                    self.time = NEVER;
                }
                false
            };
        }
        draws
    }

    fn birth(&mut self) {
        for gi in 0..self.groups.len() {
            if self.time >= self.groups[gi].start {
                self.groups[gi].start = i32::MAX;
                let id = self.spawn(gi);
                if let Some(o) = self.objs[id].as_mut() {
                    o.fresh = false;
                }
            }
        }
    }

    /// The game's object creation: allocate, insert into the draw order by group priority, reset
    /// cursors and run every program's init handler.
    fn spawn(&mut self, gi: usize) -> usize {
        let g = &self.pd.groups[gi];
        let size = g.cursor_offset.max(OBJ_WORK as i32) as usize + 4 * g.programs.len();
        let obj = Obj {
            group: gi,
            mem: vec![0; size.max(0x100)],
            cursors: vec![0; g.programs.len()],
            fresh: true,
            restart: false,
            ring: Vec::new(),
        };
        let id = match self.objs.iter().position(|o| o.is_none()) {
            Some(i) => {
                self.objs[i] = Some(obj);
                i
            }
            None => {
                self.objs.push(Some(obj));
                self.objs.len() - 1
            }
        };
        // Insert after the group's newest object, else before the first object of a group with
        // priority >= this group's.
        let pos = match self.groups[gi].last.and_then(|l| self.order.iter().position(|&x| x == l)) {
            Some(p) => p + 1,
            None => self
                .order
                .iter()
                .position(|&x| self.pd.groups[self.objs[x].as_ref().unwrap().group].priority >= g.priority)
                .unwrap_or(self.order.len()),
        };
        self.order.insert(pos, id);
        self.groups[gi].last = Some(id);
        self.groups[gi].count += 1;
        let pd = self.pd.clone();
        let obj = self.objs[id].as_mut().unwrap();
        reset_programs(obj, &pd.groups[gi], true);
        id
    }

    fn ctx<'a>(&'a mut self, env: &'a Env, draws: &'a mut Vec<Draw>) -> (Ctx<'a>, &'a mut Vec<Option<Obj>>, &'a ParticleData) {
        let running = !self.paused;
        (
            Ctx { env, dpd: &self.dpd, dpd_index: self.dpd_index, draws, rng: &mut self.rng, running },
            &mut self.objs,
            &self.pd,
        )
    }

    fn run_programs(&mut self, env: &Env, draw_pass: bool, draws: &mut Vec<Draw>) {
        let order = self.order.clone();
        let (mut ctx, objs, pd) = self.ctx(env, draws);
        for (gi, g) in pd.groups.iter().enumerate() {
            for (pi, prog) in g.programs.iter().enumerate() {
                let h = prog::handlers(prog.kind);
                if h.draw.is_some() != draw_pass || (h.frame.is_none() && h.draw.is_none()) {
                    continue;
                }
                for &id in &order {
                    let Some(obj) = objs[id].as_mut() else { continue };
                    if obj.group != gi || obj.fresh {
                        continue;
                    }
                    if obj.restart && ctx.running {
                        obj.set_i32(OBJ_FRAME, 0);
                        reset_programs(obj, g, false);
                        obj.restart = false;
                    }
                    advance_cursor(obj, prog, pi);
                    let entry = obj.cursors[pi];
                    if let Some(f) = h.frame {
                        f(&mut ctx, obj, prog, entry);
                    }
                    if let Some(d) = h.draw {
                        d(&mut ctx, obj, prog, entry);
                    }
                }
            }
        }
    }

    fn advance_frames(&mut self) {
        for &id in &self.order {
            if let Some(o) = self.objs[id].as_mut() {
                if o.fresh {
                    o.fresh = false;
                } else {
                    let f = o.frame().saturating_add(TICK).min(0x6fff_f000);
                    o.set_i32(OBJ_FRAME, f);
                }
            }
        }
    }

    fn expire(&mut self, env: &Env, draws: &mut Vec<Draw>) {
        let mut all_never = true;
        let order = self.order.clone();
        let mut dead = Vec::new();
        for id in order {
            let gi = self.objs[id].as_ref().unwrap().group;
            let g = self.pd.groups[gi].clone();
            let frame = self.objs[id].as_ref().unwrap().frame();
            if !self.ending && frame >= g.loop_end && g.loop_start < NEVER && g.loop_end < NEVER {
                let pd = self.pd.clone();
                let (mut ctx, objs, _) = self.ctx(env, draws);
                let obj = objs[id].as_mut().unwrap();
                obj.set_i32(OBJ_FRAME, 0);
                reset_programs(obj, &g, false);
                if g.loop_start == 0 {
                    fast_forward(&mut ctx, obj, &pd.groups[gi]);
                    obj.restart = true;
                } else {
                    while obj.frame() < g.loop_start {
                        fast_forward(&mut ctx, obj, &pd.groups[gi]);
                    }
                }
                all_never = false;
                continue;
            }
            if g.total_frame != NEVER {
                all_never = false;
            }
            if g.total_frame <= frame {
                dead.push(id);
            }
        }
        for id in dead {
            self.kill(id);
        }
        if self.looping && self.ending && all_never {
            for id in self.order.clone() {
                self.kill(id);
            }
        }
    }

    fn kill(&mut self, id: usize) {
        let gi = self.objs[id].as_ref().unwrap().group;
        self.order.retain(|&x| x != id);
        let g = &mut self.groups[gi];
        g.count -= 1;
        if g.count == 0 || g.last == Some(id) {
            g.last = self.order.iter().rev().copied().find(|&x| self.objs[x].as_ref().is_some_and(|o| o.group == gi && x != id));
            if g.count == 0 {
                g.last = None;
            }
        }
        self.objs[id] = None;
    }
}

/// Cursors back to each program's first entry, then the programs' init (on birth) or reset handlers.
fn reset_programs(obj: &mut Obj, g: &Group, birth: bool) {
    for (pi, prog) in g.programs.iter().enumerate() {
        obj.cursors[pi] = 0;
        let h = prog::handlers(prog.kind);
        if let Some(f) = if birth { h.init } else { h.reset } {
            f(obj, prog);
        }
    }
}

fn advance_cursor(obj: &mut Obj, prog: &crate::pax::Program, pi: usize) {
    let next = obj.cursors[pi] + prog.param_size;
    if prog.param_size > 0 && next + 4 <= prog.params.len() && prog.frame_at(next) == obj.frame() {
        obj.cursors[pi] = next;
    }
}

/// The game's single-object tick used when restarting a loop: every program's update handler
/// (draw handlers excluded), then the frame advances.
fn fast_forward(ctx: &mut Ctx, obj: &mut Obj, g: &Group) {
    for (pi, prog) in g.programs.iter().enumerate() {
        let h = prog::handlers(prog.kind);
        advance_cursor(obj, prog, pi);
        if let (Some(f), false) = (h.frame, h.skip_in_fast_forward) {
            f(ctx, obj, prog, obj.cursors[pi]);
        }
    }
    if ctx.running {
        let f = obj.frame().saturating_add(TICK).min(0x6fff_f000);
        obj.set_i32(OBJ_FRAME, f);
    }
}
