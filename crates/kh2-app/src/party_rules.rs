//! Pure rules of the party members' field behaviour (docs/party.md, stage 1 of section 9): the slot
//! bytes and the objects `FRIEND_1` / `FRIEND_2` stand for, the friend-AI class and its follow
//! thresholds, the follow points around the player, the follow step, the steer, the player trail and
//! the warp (catch-up). Each function names the game routine it ports; `party.rs` runs them.
//!
//! Spaces. The rules work in game units (centimetres) in the app's axes (Y up, the game's space turned
//! half a turn about X). Every test below is invariant under that turn (distances, dot products of two
//! vectors that both turn, products of two height differences), except the player's local axes, which
//! `follow_points` takes in the app's space (see there).
//!
//! Values. Every number of the game's data is read from the executable at run time (`Consts::from_exe`,
//! the addresses are in each field's doc comment). Numbers that are instruction immediates (loop counts,
//! a `slti` bound) are named constants with the instruction address.

use bevy::math::{Vec3, Vec4};

/// `00objentry` ids the object factory resolves at run time (FUN_0016e208): the two friends.
pub const FRIEND_1: u32 = 0x237;
pub const FRIEND_2: u32 = 0x238;
/// "No member" in a slot byte (FUN_001a71e8 masks the byte with 0x1f; 0x10 is empty).
pub const EMPTY: u8 = 0x10;
/// The party preset stage 1 starts from (docs/party.md section 9: "default preset 1").
pub const DEFAULT_PRESET: usize = 1;
/// Points in the player's trail (FUN_0010b270: 32 entries after a 16-byte header at 0x3b36c0).
pub const TRAIL_LEN: usize = 32;
/// Wanted velocities the steer averages (FUN_00110468: a ring of six at +0xc70).
pub const STEER_LEN: usize = 6;
/// The warp needs the far counter above 39 (FUN_00110a58 at 0x110ab8: `slti t7, 0x28`).
pub const WARP_FAR_MIN: i32 = 0x28;
/// The first trail point a warp tries is `warps + 0x1c + random(4)` (FUN_00110a58 at 0x110ccc / 0x110cbc).
pub const WARP_BASE: i32 = 0x1c;
pub const WARP_RANDOM: u32 = 4;
/// Trail points tried per warp check and the step between them (FUN_00110a58: 11 tries, +3 modulo 32).
pub const WARP_TRIES: usize = 11;
pub const WARP_STEP: usize = 3;
/// Rows of the follow thresholds (FUN_00114078 / FUN_001140c0): field, battle while following the
/// player, battle with another target.
pub const ROW_FIELD: usize = 0;
/// Speed modes (+0xb78): 0 stand, 1 walk, 2 run (FUN_0010e5b8).
pub const MODES: usize = 3;
/// AI classes in the parameter table at 0x34ea00 (docs/party.md section 4).
pub const CLASSES: usize = 19;

/// The follow thresholds of one AI class: `[row][mode]` = (slow-down distance, speed-up distance),
/// from the class's block in the table at 0x34ea00 (`block + row * 0x20 + mode * 8`, +4).
pub type Thresholds = [[(f32, f32); MODES]; 3];

/// The executable's numbers the stage-1 rules use.
#[derive(Clone, Debug, PartialEq)]
pub struct Consts {
    /// Per AI class, the follow thresholds (0x34ea00 table).
    pub classes: Vec<Thresholds>,
    /// 0x378480: squared spacing of the player's trail points (FUN_0010b270).
    pub trail_step2: f32,
    /// 0x3785a0: a height difference to the player above this counts toward the warp (FUN_00111070).
    pub far_dy: f32,
    /// 0x3785b4 / 0x3785b8: the sideways components of the two follow offsets, 0x3785ac their
    /// forward component (FUN_00111670 builds them once into 0x3b4030 / 0x3b4040).
    pub side: [f32; 2],
    pub ahead: f32,
    /// 0x3785a8 / 0x3785a4: the follow distance is the class's field slow-down distance of mode 1
    /// minus this times the body radius (+0x5b8), at least the second (FUN_001115d0).
    pub radius_k: f32,
    pub follow_min: f32,
    /// 0x378578: the steer divides the summed speeds by this; 0x37857c: it mixes and snaps by this
    /// (FUN_00110468).
    pub steer_div: f32,
    pub steer_mix: f32,
    /// 0x3784c0: standing, a member whose facing points this little toward the player starts turning;
    /// 0x3784b4 the facing's weight in the turn direction; 0x3784b8 the walk-speed factor of the turn;
    /// 0x3784bc the facing dot above which the turn ends (FUN_0010e140, motion state 0x1f).
    pub turn_on: f32,
    pub turn_mix: f32,
    pub turn_speed: f32,
    pub turn_off: f32,
    /// 0x378580 / 0x378584: the trail-point direction test of the warp (FUN_001108c8).
    pub warp_dy: f32,
    pub warp_dot: f32,
    /// 0x378590: fade-in frames after a warp; 0x378594: the warp cooldown (FUN_00110e28).
    pub warp_fade: f32,
    pub warp_cooldown: f32,
}

/// Bytes at a virtual address of the executable, through its program headers.
fn exe_bytes(elf: &[u8], va: u32, len: usize) -> Option<&[u8]> {
    let u16_at = |o: usize| elf.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32_at = |o: usize| elf.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if elf.get(0..4)? != b"\x7fELF" {
        return None;
    }
    let (phoff, phentsize, phnum) = (u32_at(0x1c)? as usize, u16_at(0x2a)?, u16_at(0x2c)?);
    for i in 0..phnum {
        let p = phoff + i * phentsize;
        let (kind, off, vaddr, filesz) = (u32_at(p)?, u32_at(p + 4)?, u32_at(p + 8)?, u32_at(p + 16)?);
        if kind == 1 && va >= vaddr && (va - vaddr) as usize + len <= filesz as usize {
            let start = off as usize + (va - vaddr) as usize;
            return elf.get(start..start + len);
        }
    }
    None
}

/// The friend-AI parameter table (FUN_0010b780 binds `0x34ea00 + offset[class]` to the AI object).
const AI_PARAMS: u32 = 0x34ea00;

impl Consts {
    pub fn from_exe(elf: &[u8]) -> Option<Consts> {
        let w = |va: u32| exe_bytes(elf, va, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let f = |va: u32| w(va).map(f32::from_bits);
        let mut classes = Vec::with_capacity(CLASSES);
        for class in 0..CLASSES as u32 {
            let block = AI_PARAMS + w(AI_PARAMS + 4 * class)?;
            let mut t = [[(0.0, 0.0); MODES]; 3];
            for (row, r) in t.iter_mut().enumerate() {
                for (mode, m) in r.iter_mut().enumerate() {
                    let at = block + row as u32 * 0x20 + mode as u32 * 8;
                    *m = (f(at)?, f(at + 4)?);
                }
            }
            classes.push(t);
        }
        Some(Consts {
            classes,
            trail_step2: f(0x378480)?,
            far_dy: f(0x3785a0)?,
            side: [f(0x3785b4)?, f(0x3785b8)?],
            ahead: f(0x3785ac)?,
            radius_k: f(0x3785a8)?,
            follow_min: f(0x3785a4)?,
            steer_div: f(0x378578)?,
            steer_mix: f(0x37857c)?,
            turn_on: f(0x3784c0)?,
            turn_mix: f(0x3784b4)?,
            turn_speed: f(0x3784b8)?,
            turn_off: f(0x3784bc)?,
            warp_dy: f(0x378580)?,
            warp_dot: f(0x378584)?,
            warp_fade: f(0x378590)?,
            warp_cooldown: f(0x378594)?,
        })
    }

    /// The thresholds of a class (the first block for an unknown class, as FUN_001133e0 binds it).
    pub fn thresholds(&self, class: u8) -> Thresholds {
        self.classes.get(class as usize).or(self.classes.first()).copied().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------------------------------
// Who the friends are

/// FUN_0017cd20: the object a slot byte names in the world's merged `memt` record (24 words, members
/// at words 8..24 by member index); `None` for an empty slot (0x10) or a zero word.
pub fn slot_object(merged: &[u16; 24], slot_byte: u8) -> Option<u16> {
    let i = (slot_byte & 0x1f) as usize;
    if i == EMPTY as usize {
        return None;
    }
    merged.get(8 + i).copied().filter(|&o| o != 0)
}

/// FUN_001a7330 (area-script command 0xf): the preset's player index; then the preset's members
/// flagged 0x80/0x40; then the current members (slots 1..3 in order) the preset also lists; then the
/// preset's remaining members; the rest empty. Stored bytes are masked with 0x9f.
pub fn apply_preset(cur: [u8; 4], preset: [u8; 4]) -> [u8; 4] {
    let mut tmp = preset;
    let mut out = [tmp[0] & 0x1f, EMPTY, EMPTY, EMPTY];
    let mut k = 1;
    let mut put = |out: &mut [u8; 4], v: u8| {
        if k < 4 {
            out[k] = v & 0x9f;
        }
        k += 1;
    };
    for t in tmp.iter_mut().skip(1) {
        if *t & 0xc0 != 0 {
            put(&mut out, *t);
            *t = EMPTY;
        }
    }
    for &c in &cur[1..] {
        let v = c & 0x1f;
        if v != EMPTY
            && let Some(j) = (0..4).find(|&j| tmp[j] & 0x1f == v)
        {
            put(&mut out, v);
            tmp[j] = EMPTY;
        }
    }
    for &t in &tmp[1..] {
        if t & 0x1f != EMPTY {
            put(&mut out, t);
        }
    }
    out
}

/// FUN_0010b780: the friend-AI class (index into the table at 0x34ea00) for an object's NeoStatus
/// and NeoMoveset; `None` = no AI object (Sora 1, Mickey 4, Roxas 14). An unlisted NeoStatus gets
/// the default class (FUN_001133e0) bound to the table's first block.
pub fn ai_class(neo: u16, moveset: u16) -> Option<u8> {
    Some(match neo {
        1 | 4 | 14 => return None,
        2 if moveset == 0x12 => 12,
        2 => 0,
        3 if moveset == 0x15 => 13,
        3 => 1,
        5 => 2,
        15 => 3,
        6 => 4,
        7 => 5,
        8 => 6,
        9 => 7,
        10 => 8,
        11 => 9,
        12 => 10,
        13 => 11,
        200..=204 => (neo - 200 + 14) as u8,
        _ => 0,
    })
}

// ---------------------------------------------------------------------------------------------------
// The party manager: trail and follow points

/// The player's trail (FUN_0010b270 on 0x3b36c0): 32 positions, a new one when the player is at
/// least `sqrt(trail_step2)` from the newest.
#[derive(Clone, Debug, PartialEq)]
pub struct Trail {
    pub points: [Vec3; TRAIL_LEN],
    /// Index of the newest point (+4 of the header), -1 before the first.
    pub head: i32,
    /// Byte 0 of the header: set once a point has been added after the first fill.
    pub moved: bool,
}

impl Default for Trail {
    fn default() -> Self {
        Trail { points: [Vec3::ZERO; TRAIL_LEN], head: -1, moved: false }
    }
}

impl Trail {
    /// One frame: the first call fills every entry with `p`; later calls add `p` after the newest when
    /// it is far enough. Returns whether a point was written this frame (header byte 1).
    pub fn push(&mut self, p: Vec3, step2: f32) -> bool {
        if self.head < 0 {
            self.points = [p; TRAIL_LEN];
            self.head = 0;
            return true;
        }
        if (p - self.points[self.head as usize]).length_squared() < step2 {
            return false;
        }
        self.head = (self.head + 1).rem_euclid(TRAIL_LEN as i32);
        self.points[self.head as usize] = p;
        self.moved = true;
        true
    }
}

/// FUN_001115d0: how far ahead of the player a member's follow points lie: the class's field
/// slow-down distance of mode 1, minus `radius_k` times the body radius (+0x5b8, the radius of the
/// body shape, FUN_00169af0), at least `follow_min`. (Doubled in battle for style 3; no battle here.)
pub fn follow_distance(t: &Thresholds, radius: f32, c: &Consts) -> f32 {
    (t[ROW_FIELD][1].0 - radius * c.radius_k).max(c.follow_min)
}

/// FUN_00111670: the follow point of each member. The two offsets `(side[k], 0, ahead)` times the
/// member's follow distance are carried through the player's model matrix (axis rows from +0x6b0,
/// position row from +0x70: [inferred] to be the player's position); each member takes
/// the nearer of the two, and when both take the same one the member that is farther from it (the
/// second one on a tie) takes the other.
///
/// `player_yaw` is the player's facing in the app's space (0 = +Z): the app draws the model with
/// exactly that turn about its origin, so the matrix's local axes are the app's local axes (local +Z
/// is the facing). That the game's +0x6b0 matches the app's draw transform up to the half turn of the
/// space is [inferred] from the app's models facing their direction of travel; the points therefore
/// lie ahead of the player, `atan(side / ahead)` off the facing to either side.
pub fn follow_points(player: Vec3, player_yaw: f32, members: [Option<(Vec3, f32)>; 2], c: &Consts) -> [Option<Vec3>; 2] {
    let (s, co) = player_yaw.sin_cos();
    let local = |k: usize, d: f32| {
        let (x, z) = (c.side[k] * d, c.ahead * d);
        player + Vec3::new(x * co + z * s, 0.0, -x * s + z * co)
    };
    let mut out = [None; 2];
    let mut pick = [usize::MAX; 2];
    let mut best = [0.0f32; 2];
    for (i, m) in members.iter().enumerate() {
        let Some((pos, d)) = *m else { continue };
        for k in 0..2 {
            let p = local(k, d);
            let d2 = (p - pos).length_squared();
            if pick[i] == usize::MAX || d2 < best[i] {
                best[i] = d2;
                pick[i] = k;
                out[i] = Some(p);
            }
        }
    }
    if let (Some((_, d0)), Some((_, d1))) = (members[0], members[1])
        && pick[0] == pick[1]
    {
        if best[0] <= best[1] {
            out[1] = Some(local(pick[1] ^ 1, d1));
        } else {
            out[0] = Some(local(pick[0] ^ 1, d0));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------
// One member: follow, steer, warp

/// The member's state the follow and the steer keep.
#[derive(Clone, Debug, PartialEq)]
pub struct Follow {
    /// +0xb78: speed mode 0..2.
    pub mode: usize,
    /// +0xb7c bit 0x10: standing and turning to face the player.
    pub turning: bool,
    /// +0xc70 / +0xcd0: the steer's ring of wanted velocities (xyz = direction times speed, w = the
    /// speed squared) and its write index (-1 before the first).
    pub ring: [Vec4; STEER_LEN],
    pub ring_at: i32,
}

impl Default for Follow {
    fn default() -> Self {
        Follow { mode: 0, turning: false, ring: [Vec4::ZERO; STEER_LEN], ring_at: -1 }
    }
}

/// What the follow step reads (FUN_0010df08's relation to the player at +0xb80, the member's facing
/// +0x550, its walk and run speeds +0x110 / +0x114).
#[derive(Clone, Copy, Debug)]
pub struct FollowIn {
    /// The follow point minus the member's position.
    pub to_point: Vec3,
    /// +0xb90 / +0xb9c: the unit direction and the 3D distance to the player.
    pub to_player: Vec3,
    pub dist: f32,
    /// +0xba0: the unit direction to the player on the ground plane.
    pub ground_dir: Vec3,
    /// +0x550: the member's facing.
    pub facing: Vec3,
    pub walk: f32,
    pub run: f32,
}

/// FUN_0010e5b8: the speed of a mode (0 stands, 1 walks, 2 runs).
pub fn mode_speed(mode: usize, walk: f32, run: f32) -> f32 {
    match mode {
        1 => walk,
        2 => run,
        _ => 0.0,
    }
}

/// The speed-mode hysteresis of the follow step: above the mode's speed-up distance one mode faster
/// (at most 2), else below its slow-down distance one mode slower (at least 0).
pub fn next_mode(mode: usize, dist: f32, t: &[(f32, f32); MODES]) -> usize {
    let (down, up) = t[mode.min(MODES - 1)];
    if up < dist {
        (mode + 1).min(MODES - 1)
    } else if dist < down {
        mode.saturating_sub(1)
    } else {
        mode
    }
}

/// FUN_0010e140, motion state 0x1f/0x20 (the friend's free state, FUN_00111070 puts it there), with
/// the follow vector (+0xb80): the direction to the follow point (to the player himself while the
/// turning bit is set), the speed of the mode the distance to the player asks for (field row);
/// standing, a member not facing the player turns toward him at a fraction of the walk speed.
/// Returns the wanted direction (+0xc00) and speed (+0xc0c).
pub fn follow_step(st: &mut Follow, i: &FollowIn, t: &Thresholds, c: &Consts) -> (Vec3, f32) {
    let mut dir = if st.turning { i.to_player } else { i.to_point.normalize_or_zero() };
    st.mode = next_mode(st.mode, i.dist, &t[ROW_FIELD]);
    let mut speed = mode_speed(st.mode, i.walk, i.run);
    if speed != 0.0 {
        st.turning = false;
    } else if !st.turning {
        if i.facing.dot(i.ground_dir) <= c.turn_on {
            st.turning = true;
        }
    } else {
        dir = (dir + i.facing * c.turn_mix).normalize_or_zero();
        speed = i.walk * c.turn_speed;
        if i.facing.dot(i.ground_dir) > c.turn_off {
            st.turning = false;
        }
    }
    (dir, speed)
}

/// FUN_00110468: the wanted velocity is averaged with the last six (a turning member stores zero),
/// mixed with this frame's, then the speed snaps to stand / walk / run. Returns the direction (not
/// normalised: the game keeps the mixed vector) and the speed.
pub fn steer(st: &mut Follow, dir: Vec3, speed: f32, walk: f32, run: f32, c: &Consts) -> (Vec3, f32) {
    let v = if st.turning { Vec4::ZERO } else { (dir * speed).extend(speed * speed) };
    if st.ring_at < 0 {
        st.ring = [v; STEER_LEN];
        st.ring_at = STEER_LEN as i32 - 1;
    }
    st.ring[st.ring_at as usize] = v;
    st.ring_at += 1;
    if st.ring_at >= STEER_LEN as i32 {
        st.ring_at = 0;
    }
    let sum: Vec4 = st.ring.iter().copied().sum();
    let len = sum.truncate().length();
    let avg = if len > 0.0 { (sum.truncate() / len).extend(len / c.steer_div) } else { Vec4::ZERO };
    let mixed = (dir.extend(speed) + avg) * c.steer_mix;
    let mut s = mixed.w;
    if s < run {
        s = if s < walk * c.steer_mix {
            0.0
        } else if s < (walk + run) * c.steer_mix {
            walk
        } else {
            run
        };
    }
    if st.turning {
        s = walk;
    }
    (mixed.truncate(), s)
}

/// FUN_00110a58's first gate: no cooldown left and the far counter above 39.
pub fn warp_due(cooldown: f32, far: i32) -> bool {
    cooldown <= 0.0 && far >= WARP_FAR_MIN
}

/// FUN_00110a58: the trail indices a warp tries, in order: from `(warps + 0x1c + r) & 31` (`r` a
/// random number below 4, FUN_0010f420), eleven tries three apart modulo 32.
pub fn warp_order(warps: i32, r: u32) -> impl Iterator<Item = usize> {
    let start = (warps + WARP_BASE + r as i32) as usize & (TRAIL_LEN - 1);
    (0..WARP_TRIES).map(move |k| (start + k * WARP_STEP) % TRAIL_LEN)
}

/// FUN_001108c8, the direction part (the screen and collision parts are the caller's): `d` is the
/// trail point minus the player. Its height must not lie on the player's side of the member's height
/// difference (`d.y * dy <= 0`, `dy` the player's height minus the member's); then, with the member's
/// ground direction to the player, the point must not lie past the player: dot <= 0 when the height
/// difference is below `warp_dy`, dot <= `warp_dot` otherwise.
pub fn warp_point_ok(point: Vec3, player: Vec3, member: Vec3, ground_dir: Vec3, c: &Consts) -> bool {
    let d = point - player;
    let dy = player.y - member.y;
    if d.y * dy > 0.0 {
        return false;
    }
    let dot = ground_dir.dot(d.with_y(0.0));
    if dy.abs() < c.warp_dy { dot <= 0.0 } else { dot <= c.warp_dot }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consts() -> Consts {
        let t = [[(0.0, 200.0), (160.0, 240.0), (200.0, 1000.0)]; 3];
        Consts {
            classes: vec![t; CLASSES],
            trail_step2: 2025.0,
            far_dy: 400.0,
            side: [0.86, -0.86],
            ahead: 0.51,
            radius_k: 0.5,
            follow_min: 1.0,
            steer_div: 6.0,
            steer_mix: 0.5,
            turn_on: 0.5,
            turn_mix: 0.8,
            turn_speed: 0.2,
            turn_off: 0.97,
            warp_dy: 1000.0,
            warp_dot: -0.5,
            warp_fade: 30.0,
            warp_cooldown: 480.0,
        }
    }

    /// The speed mode climbs one step per frame above the speed-up distance and drops one below the
    /// slow-down distance; in between it holds (hysteresis).
    #[test]
    fn party_speed_mode_hysteresis() {
        let t = consts().classes[0][ROW_FIELD];
        assert_eq!(next_mode(0, 150.0, &t), 0);
        assert_eq!(next_mode(0, 201.0, &t), 1);
        assert_eq!(next_mode(1, 201.0, &t), 1);
        assert_eq!(next_mode(1, 241.0, &t), 2);
        assert_eq!(next_mode(2, 999.0, &t), 2);
        assert_eq!(next_mode(2, 199.0, &t), 1);
        assert_eq!(next_mode(1, 170.0, &t), 1);
        assert_eq!(next_mode(1, 159.0, &t), 0);
    }

    /// The trail fills on the first frame, then adds a point only 45 units (sqrt 2025) from the newest.
    #[test]
    fn party_trail_spacing() {
        let c = consts();
        let mut tr = Trail::default();
        assert!(tr.push(Vec3::ZERO, c.trail_step2));
        assert_eq!(tr.head, 0);
        assert!(!tr.moved);
        assert!(!tr.push(Vec3::new(44.0, 0.0, 0.0), c.trail_step2));
        assert!(tr.push(Vec3::new(45.0, 0.0, 0.0), c.trail_step2));
        assert_eq!((tr.head, tr.moved), (1, true));
        for k in 2..40 {
            tr.push(Vec3::new(45.0 * k as f32, 0.0, 0.0), c.trail_step2);
        }
        assert_eq!(tr.head, 39 % 32);
    }

    /// The steer snaps the mixed speed to stand, walk or run, and a turning member walks.
    #[test]
    fn party_steer_snaps_speed() {
        let c = consts();
        let mut st = Follow::default();
        let (_, s) = steer(&mut st, Vec3::Z, 8.0, 2.0, 8.0, &c);
        assert_eq!(s, 8.0);
        let mut st = Follow::default();
        let (_, s) = steer(&mut st, Vec3::Z, 0.0, 2.0, 8.0, &c);
        assert_eq!(s, 0.0);
        // From running, a stop decays over the ring: (0 + avg) / 2 with avg falling as zeros come in.
        let mut st = Follow::default();
        steer(&mut st, Vec3::Z, 8.0, 2.0, 8.0, &c);
        let speeds: Vec<f32> = (0..6).map(|_| steer(&mut st, Vec3::Z, 0.0, 2.0, 8.0, &c).1).collect();
        assert_eq!(speeds[0], 2.0, "{speeds:?}");
        assert_eq!(*speeds.last().unwrap(), 0.0, "{speeds:?}");
        st.turning = true;
        assert_eq!(steer(&mut st, Vec3::Z, 0.0, 2.0, 8.0, &c).1, 2.0);
    }

    /// The follow points lie ahead of the player to both sides; each member takes the nearer, and two
    /// members never share one.
    #[test]
    fn party_follow_points_split() {
        let c = consts();
        let d = 160.0;
        let left_back = Vec3::new(100.0, 0.0, -50.0);
        let right_back = Vec3::new(-100.0, 0.0, -50.0);
        let p = follow_points(Vec3::ZERO, 0.0, [Some((left_back, d)), Some((right_back, d))], &c);
        assert!((p[0].unwrap() - Vec3::new(0.86 * d, 0.0, 0.51 * d)).length() < 1e-3);
        assert!((p[1].unwrap() - Vec3::new(-0.86 * d, 0.0, 0.51 * d)).length() < 1e-3);
        // Both on the same side: the nearer keeps it, the other gets the other point.
        let p = follow_points(Vec3::ZERO, 0.0, [Some((left_back, d)), Some((left_back * 2.0, d))], &c);
        assert!(p[0].unwrap().x > 0.0 && p[1].unwrap().x < 0.0);
        // Turned a quarter: ahead is +X.
        let p = follow_points(Vec3::ZERO, std::f32::consts::FRAC_PI_2, [Some((Vec3::ZERO, d)), None], &c);
        assert!(p[0].unwrap().x > 0.0 && p[1].is_none());
    }

    /// Standing members facing away from the player start turning (and walk slowly toward the point);
    /// the turn ends once they face him.
    #[test]
    fn party_standing_member_turns_to_the_player() {
        let c = consts();
        let t = c.classes[0];
        let mut st = Follow::default();
        let i = FollowIn { to_point: Vec3::Z * 50.0, to_player: Vec3::Z, dist: 100.0, ground_dir: Vec3::Z, facing: -Vec3::Z, walk: 2.0, run: 8.0 };
        let (_, s) = follow_step(&mut st, &i, &t, &c);
        assert!(st.turning && s == 0.0);
        let (_, s) = follow_step(&mut st, &i, &t, &c);
        assert!((s - 0.4).abs() < 1e-6);
        let i = FollowIn { facing: Vec3::Z, ..i };
        follow_step(&mut st, &i, &t, &c);
        assert!(!st.turning);
    }

    /// The warp tries eleven trail points three apart from `warps + 0x1c + r`, and only points not past
    /// the player as the member sees him.
    #[test]
    fn party_warp_order_and_point_test() {
        let o: Vec<usize> = warp_order(0, 0).collect();
        assert_eq!(o.len(), 11);
        assert_eq!(&o[..4], &[28, 31, 2, 5]);
        assert!(!warp_due(0.0, 39) && warp_due(0.0, 40) && !warp_due(1.0, 100));
        let c = consts();
        let (player, member) = (Vec3::ZERO, Vec3::new(0.0, 0.0, -1000.0));
        let gdir = Vec3::Z;
        assert!(warp_point_ok(Vec3::new(0.0, 0.0, -100.0), player, member, gdir, &c));
        assert!(!warp_point_ok(Vec3::new(0.0, 0.0, 100.0), player, member, gdir, &c));
    }

    // ---- Oracle checks (PLAYBOOK R2): the game's own routines in kh2-ee on the user's executable.

    const STACK: u32 = 0x01ff_0000;
    const OBJ: u32 = 0x0182_0000;
    const CTRL: u32 = 0x0183_0000;
    const AI: u32 = 0x0184_0000;
    const RING: u32 = 0x0180_0000;
    const POS: u32 = 0x0181_0000;
    /// The follow points (FUN_00111670 writes them, FUN_0010e140 reads `+ position x 16`).
    const FOLLOW_POINTS: u32 = 0x0034_e920;
    /// The battle flag FUN_001903d8 reads.
    const BATTLE: u32 = 0x01c8_a7d8;

    fn oracle() -> Option<(kh2_ee::Ee, Consts)> {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists())?;
        let elf = crate::assets::Disc::open(&iso).read_root("SLUS_210.05")?;
        let mut ee = kh2_ee::Ee::new();
        ee.load_elf(&elf)?;
        Some((ee, Consts::from_exe(&elf)?))
    }

    fn clear(ee: &mut kh2_ee::Ee, at: u32, len: u32) {
        for o in (0..len).step_by(4) {
            ee.w32(at + o, 0);
        }
    }

    fn wv(ee: &mut kh2_ee::Ee, at: u32, v: Vec4) {
        for (i, x) in v.to_array().into_iter().enumerate() {
            ee.wf32(at + 4 * i as u32, x);
        }
    }

    fn rv(ee: &mut kh2_ee::Ee, at: u32) -> Vec4 {
        Vec4::new(ee.f32_at(at), ee.f32_at(at + 4), ee.f32_at(at + 8), ee.f32_at(at + 12))
    }

    struct Lcg(u32);
    impl Lcg {
        fn f(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (self.0 >> 8) as f32 / (1u32 << 24) as f32
        }
        fn unit(&mut self) -> Vec3 {
            let a = self.f() * std::f32::consts::TAU;
            Vec3::new(a.sin(), 0.0, a.cos())
        }
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() <= 1e-4 * (1.0 + a.length().max(b.length()))
    }

    /// FUN_0010b270 (the player's trail) against `Trail::push` over a random walk with jumps: the
    /// return byte, the head index, the newest point and the header's byte 0 agree every frame.
    #[test]
    fn party_trail_matches_fun_0010b270() {
        let Some((mut ee, c)) = oracle() else { return };
        clear(&mut ee, RING, 0x220);
        ee.w32(RING + 4, u32::MAX);
        let mut tr = Trail::default();
        let mut rng = Lcg(7);
        let mut p = Vec3::new(100.0, 0.0, -50.0);
        for step in 0..400 {
            p += rng.unit() * if rng.f() < 0.05 { 300.0 } else { rng.f() * 30.0 } + Vec3::Y * (rng.f() - 0.5) * 4.0;
            wv(&mut ee, POS, p.extend(1.0));
            let game = ee.call(0x10b270, &[RING as u64, POS as u64], &[], STACK).expect("FUN_0010b270") & 0xff;
            let port = tr.push(p, c.trail_step2);
            assert_eq!(game, port as u64, "step {step}: point added");
            let head = ee.r32(RING + 4) as i32;
            assert_eq!(head, tr.head, "step {step}: head");
            assert_eq!(ee.r32(RING) & 0xff != 0, tr.moved, "step {step}: header byte 0");
            assert!(close(rv(&mut ee, RING + 16 * (1 + head as u32)).truncate(), tr.points[head as usize]), "step {step}: newest point");
        }
        eprintln!("FUN_0010b270 agrees with Trail::push over 400 frames (head {})", tr.head);
    }

    /// FUN_00110468 (the steer) against `steer` over random wanted velocities, speeds (stand, walk, run
    /// and in between) and turning frames: the mixed direction, the snapped speed and the ring index.
    #[test]
    fn party_steer_matches_fun_00110468() {
        let Some((mut ee, c)) = oracle() else { return };
        let (walk, run) = (2.0, 12.0);
        clear(&mut ee, OBJ, 0xd00);
        clear(&mut ee, CTRL, 0x10);
        ee.w32(OBJ + 0xc, CTRL);
        ee.w32(CTRL + 4, 0x1f);
        ee.wf32(OBJ + 0x110, walk);
        ee.wf32(OBJ + 0x114, run);
        ee.w32(OBJ + 0xcd0, u32::MAX);
        let mut st = Follow::default();
        let mut rng = Lcg(11);
        for step in 0..300 {
            let dir = rng.unit();
            let speed = match (rng.f() * 5.0) as u32 {
                0 => 0.0,
                1 => walk,
                2 => run,
                _ => rng.f() * run,
            };
            st.turning = rng.f() < 0.1;
            wv(&mut ee, OBJ + 0xc00, dir.extend(speed));
            ee.w32(OBJ + 0xb7c, if st.turning { 0x10 } else { 0 });
            ee.call(0x110468, &[OBJ as u64], &[], STACK).expect("FUN_00110468");
            let game = rv(&mut ee, OBJ + 0xc00);
            let (d, s) = steer(&mut st, dir, speed, walk, run, &c);
            assert!(close(game.truncate(), d), "step {step}: direction {game:?} vs {d:?}");
            assert!((game.w - s).abs() <= 1e-4, "step {step}: speed {} vs {s}", game.w);
            assert_eq!(ee.r32(OBJ + 0xcd0) as i32, st.ring_at, "step {step}: ring index");
        }
        eprintln!("FUN_00110468 agrees with steer over 300 frames");
    }

    /// FUN_0010e140 in the friend's motion state 0x1f with the follow vector, out of battle, against
    /// `follow_step` with the executable's thresholds of class 0 (Donald): the speed mode, the turning
    /// bit, the wanted direction and speed, over random positions, facings and starting modes.
    #[test]
    fn party_follow_step_matches_fun_0010e140() {
        let Some((mut ee, c)) = oracle() else { return };
        let (walk, run) = (2.0, 12.0);
        let class = 0u8;
        let block = 0x34ea00 + ee.r32(0x34ea00 + 4 * class as u32);
        let t = c.thresholds(class);
        let mut rng = Lcg(23);
        for step in 0..400 {
            clear(&mut ee, OBJ, 0xd00);
            clear(&mut ee, CTRL, 0x10);
            clear(&mut ee, AI, 0x20);
            ee.w32(OBJ + 0xc, CTRL);
            ee.w32(CTRL + 4, 0x1f);
            ee.w32(AI + 8, block);
            ee.w32(OBJ + 0xb60, AI);
            ee.w32(OBJ + 0xb70, 1);
            ee.w32(BATTLE, 0);
            ee.wf32(OBJ + 0x110, walk);
            ee.wf32(OBJ + 0x114, run);
            let mode = (rng.f() * 3.0) as usize;
            let turning = rng.f() < 0.3;
            ee.w32(OBJ + 0xb78, mode as u32);
            ee.w32(OBJ + 0xb7c, if turning { 0x10 } else { 0 });
            let pos = Vec3::new(rng.f() * 400.0, 0.0, rng.f() * 400.0);
            let player = pos + rng.unit() * rng.f() * 600.0 + Vec3::Y * (rng.f() - 0.5) * 50.0;
            let point = player + rng.unit() * 115.0;
            let facing = rng.unit();
            wv(&mut ee, OBJ + 0x540, pos.extend(1.0));
            wv(&mut ee, FOLLOW_POINTS + 16, point.extend(1.0));
            wv(&mut ee, OBJ + 0x550, facing.extend(0.0));
            let rel = player - pos;
            let gdir = rel.with_y(0.0).normalize_or_zero();
            ee.wf32(OBJ + 0xb80, rel.y);
            wv(&mut ee, OBJ + 0xb90, rel.normalize_or_zero().extend(rel.length()));
            wv(&mut ee, OBJ + 0xba0, gdir.extend(rel.with_y(0.0).length()));
            let r = ee.call(0x10e140, &[OBJ as u64, OBJ as u64 + 0xb80], &[], STACK).expect("FUN_0010e140");
            assert_eq!(r & 0xffff_ffff, 1);
            let mut st = Follow { mode, turning, ..Follow::default() };
            let i = FollowIn { to_point: point - pos, to_player: rel.normalize_or_zero(), dist: rel.length(), ground_dir: gdir, facing, walk, run };
            let (d, s) = follow_step(&mut st, &i, &t, &c);
            let game = rv(&mut ee, OBJ + 0xc00);
            assert_eq!(ee.r32(OBJ + 0xb78) as usize, st.mode, "step {step}: mode");
            assert_eq!(ee.r32(OBJ + 0xb7c) & 0x10 != 0, st.turning, "step {step}: turning bit");
            assert!(
                close(game.truncate(), d),
                "step {step}: direction {game:?} vs {d:?}; mode {mode} turning {turning} pos {pos} player {player} point {point} facing {facing} to point {:?}",
                (point - pos).normalize()
            );
            assert!((game.w - s).abs() <= 1e-4, "step {step}: speed {} vs {s}", game.w);
        }
        eprintln!("FUN_0010e140 (state 0x1f, field) agrees with follow_step over 400 cases");
    }

    /// FUN_00111670 (the follow points) against `follow_points`: the player's model matrix (+0x6b0)
    /// is the turn the app draws him with (rows: local X, Y, Z, position), the two members stand at
    /// random places with the body radius of the disc's Donald; the points written to 0x34e930 and
    /// 0x34e940 agree, the shared-point rule included.
    #[test]
    fn party_follow_points_match_fun_00111670() {
        let Some((mut ee, c)) = oracle() else { return };
        const PLAYER: u32 = 0x0185_0000;
        const MEMBERS: [u32; 2] = [0x0186_0000, 0x0187_0000];
        let class = 0u8;
        let block = 0x34ea00 + ee.r32(0x34ea00 + 4 * class as u32);
        let t = c.thresholds(class);
        let radius = 90.0;
        let mut rng = Lcg(31);
        let mut shared = 0;
        for step in 0..300 {
            for o in [PLAYER, MEMBERS[0], MEMBERS[1]] {
                clear(&mut ee, o, 0xd00);
            }
            clear(&mut ee, AI, 0x20);
            ee.w32(AI + 8, block);
            ee.w32(BATTLE, 0);
            ee.w32(0x34e908, PLAYER);
            ee.w32(0x34e90c, MEMBERS[0]);
            ee.w32(0x34e910, MEMBERS[1]);
            let player = Vec3::new(rng.f() * 1000.0, 0.0, rng.f() * 1000.0);
            let yaw = (rng.f() - 0.5) * std::f32::consts::TAU;
            let (s, co) = yaw.sin_cos();
            wv(&mut ee, PLAYER + 0x540, player.extend(1.0));
            wv(&mut ee, PLAYER + 0x6b0, Vec4::new(co, 0.0, -s, 0.0));
            wv(&mut ee, PLAYER + 0x6c0, Vec4::new(0.0, 1.0, 0.0, 0.0));
            wv(&mut ee, PLAYER + 0x6d0, Vec4::new(s, 0.0, co, 0.0));
            // The routine takes the matrix's three axis rows from +0x6b0 and the position row from +0x70
            // (the position FUN_0016b960 returns for a linked object).
            wv(&mut ee, PLAYER + 0x70, player.extend(1.0));
            let mut pos = [Vec3::ZERO; 2];
            for (k, &m) in MEMBERS.iter().enumerate() {
                // Every third case both members stand on the same side, so they want the same point.
                pos[k] = if step % 3 == 0 && k == 1 { pos[0] + rng.unit() * 40.0 } else { player + rng.unit() * rng.f() * 500.0 };
                wv(&mut ee, m + 0x540, pos[k].extend(1.0));
                ee.w32(m + 0xb60, AI);
                ee.wf32(m + 0x5b8, radius);
            }
            ee.call(0x111670, &[], &[], STACK).expect("FUN_00111670");
            let d = follow_distance(&t, radius, &c);
            let port = follow_points(player, yaw, [Some((pos[0], d)), Some((pos[1], d))], &c);
            for (k, p) in port.iter().enumerate() {
                let game = rv(&mut ee, FOLLOW_POINTS + 16 * (1 + k as u32)).truncate();
                assert!(close(game, p.unwrap()), "step {step} member {k}: {game} vs {p:?}");
            }
            let local = |p: Vec3| ((p - player).x * co - (p - player).z * s).signum();
            if local(pos[0]) == local(pos[1]) {
                shared += 1;
            }
        }
        eprintln!("FUN_00111670 agrees with follow_points over 300 cases ({shared} with both members on one side)");
    }

    /// FUN_001a7330 on the empty party with preset 1 gives Donald and Goofy in slots 1 and 2, and the
    /// slot bytes name merged words 9 and 10.
    #[test]
    fn party_default_preset_names_donald_and_goofy() {
        let s = apply_preset([0, EMPTY, EMPTY, EMPTY], [0, 1, 2, EMPTY]);
        assert_eq!(s, [0, 1, 2, EMPTY]);
        let mut rec = [0u16; 24];
        rec[9] = 0x5a;
        rec[10] = 0x5b;
        assert_eq!(slot_object(&rec, s[1]), Some(0x5a));
        assert_eq!(slot_object(&rec, s[2]), Some(0x5b));
        assert_eq!(slot_object(&rec, s[3]), None);
        assert_eq!(ai_class(2, 17), Some(0));
        assert_eq!(ai_class(3, 0x15), Some(13));
        assert_eq!(ai_class(1, 0), None);
    }
}
