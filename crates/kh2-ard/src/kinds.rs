//! Spawn-group kinds 7 and 9 (`FUN_00198b30` cases 7 and 9; docs/systems/core-runtime/areas.md section 2).
//!
//! Kind 7 is a volume that hands the player to a global request object (`FUN_00199e68` -> `FUN_001ccb10`); kind 9 is a group of
//! units that appear when the player comes near and in front of them and go away when he leaves (`FUN_0019a000`).

use crate::Group;

/// Offsets of the request object `FUN_001ccb10` writes (the object `DAT_003601d0` points at).
pub mod request {
    /// `u32`: the group header's word at +0x1c.
    pub const VALUE: usize = 0x04;
    /// `u32`: bit 0 set by the player-flag case, bit 1 by the other.
    pub const FLAGS: usize = 0x08;
    /// The player pointer when the flag argument is non-zero.
    pub const PLAYER_A: usize = 0x34;
    /// The player pointer when it is zero.
    pub const PLAYER_B: usize = 0x38;
}

/// orig 0x1ccb10
///
/// Fills the request object (`obj`, at least 0x3c bytes; the game does nothing when `DAT_003601d0` is null, so pass `None` for
/// that): `value` to +4, then with `flag` non-zero (the low byte of the third argument) bit 0 of +8 and `player` to +0x34, else
/// bit 1 of +8 and `player` to +0x38.
pub fn fill_request(obj: Option<&mut [u8]>, player: u32, value: u32, flag: u8) {
    let Some(obj) = obj else { return };
    obj[request::VALUE..request::VALUE + 4].copy_from_slice(&value.to_le_bytes());
    let flags = u32::from_le_bytes(obj[request::FLAGS..request::FLAGS + 4].try_into().unwrap());
    if flag != 0 {
        obj[request::FLAGS..request::FLAGS + 4].copy_from_slice(&(flags | 1).to_le_bytes());
        obj[request::PLAYER_A..request::PLAYER_A + 4].copy_from_slice(&player.to_le_bytes());
    } else {
        obj[request::FLAGS..request::FLAGS + 4].copy_from_slice(&(flags | 2).to_le_bytes());
        obj[request::PLAYER_B..request::PLAYER_B + 4].copy_from_slice(&player.to_le_bytes());
    }
}

/// orig 0x199e68
///
/// Kind 7 when the player touches a volume of the group (`FUN_00198b30` answers 0 afterwards, so the walk-in does not stop other
/// groups from testing it): [`fill_request`] with the group header's word +0x1c and bit 0 of the player's data word +0x28 (the
/// word `*(player + 0x6c) + 0x28`) as the flag.
pub fn touch_request(group: &Group, request_object: Option<&mut [u8]>, player: u32, player_word_28: u32) {
    let value = u32::from_le_bytes(group.header[0x1c..0x20].try_into().unwrap());
    fill_request(request_object, player, value, (player_word_28 & 1) as u8);
}

/// What the unit-streaming kind needs from the running game.
pub trait UnitHost {
    /// Position of the player (`FUN_0016b960`: the actor's +0x70, or +0x540 when `FUN_001beb00` is 0).
    fn player_position(&self) -> [f32; 3];
    /// The player's facing vector (`FUN_0016a890`).
    fn player_facing(&self) -> [f32; 3];
    /// The live object with this serial (`FUN_0016b0d8`: the first object in the object list whose `FUN_0016b928` key is it).
    fn find_object(&mut self, serial: u16) -> Option<u32>;
    /// Whether the area's cache of defeated or consumed units (`FUN_001942e0` on the table `0x35d278`) holds this serial.
    fn is_cached(&mut self, serial: u16) -> bool;
    /// Position of an object found by [`UnitHost::find_object`] (`FUN_0016b960`).
    fn object_position(&self, obj: u32) -> [f32; 3];
    /// `FUN_001982c8(entry, group)`: creates the unit of entity `entry` at its record's position.
    fn appear(&mut self, entry: usize);
    /// The object's method at +0x20 (called with the object): the unit goes away.
    fn disappear(&mut self, obj: u32);
}

/// The two constants of kind 9: `DAT_0037b310` (inside this distance a live unit stays) and `DAT_0037b314` (the smallest dot
/// product of the direction to the unit with the player's facing for a unit to appear or stay). Read from the executable at run time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamLimits {
    pub near: f32,
    pub facing: f32,
}

/// `d / |d|` the way the VU0 code does it (`vsqrt`, then `1 / q`, then a multiply), and its dot product with `f`.
fn facing_dot(d: [f32; 3], f: [f32; 3]) -> f32 {
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let inv = 1.0 / len;
    let n = [d[0] * inv, d[1] * inv, d[2] * inv];
    n[0] * f[0] + n[1] * f[1] + n[2] * f[2]
}

/// The larger of |x| and |z| (the game compares them with `FUN_00300188`, an absolute value by masking the sign bit).
fn reach(d: [f32; 3]) -> f32 {
    d[0].abs().max(d[2].abs())
}

/// orig 0x19a000
///
/// Kind 9, once per frame for a group (`radius` is the float at header +0x1c, `entities` its records, `serial` the halfword +0x1e).
/// For each entity:
/// * a unit that does not exist yet and is not cached appears when its distance (the larger of |dx|, |dz| to the player) is below
///   `radius` and the direction to it has a dot product above `limits.facing` with the player's facing;
/// * a unit that exists goes away when that distance is above `radius`, or when it is farther than `limits.near` and its direction
///   has a dot product below `limits.facing`; otherwise it stays.
///
/// [inferred] that the dot product sums all three components of the normalised direction (the VU0 sequence adds lanes with two
/// broadcast adds; the game's facing vectors have y 0, which makes the readings agree). Not covered by an oracle test: the
/// callees are object-list and VU0 code that the factory cannot lay out.
pub fn stream_units(host: &mut dyn UnitHost, group: &Group, radius: f32, limits: StreamLimits) {
    for (i, e) in group.entities.iter().enumerate() {
        let p = host.player_position();
        match host.find_object(e.serial) {
            None => {
                if host.is_cached(e.serial) {
                    continue;
                }
                let d = [e.position[0] - p[0], e.position[1] - p[1], e.position[2] - p[2]];
                if reach(d) < radius && limits.facing < facing_dot(d, host.player_facing()) {
                    host.appear(i);
                }
            }
            Some(obj) => {
                let q = host.object_position(obj);
                let d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
                let m = reach(d);
                if m <= radius && (m <= limits.near || limits.facing <= facing_dot(d, host.player_facing())) {
                    continue;
                }
                host.disappear(obj);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Entity;

    fn group(units: &[(u16, [f32; 3])]) -> Group {
        Group {
            kind: 9,
            id: 1,
            entities: units.iter().map(|&(serial, position)| Entity { object: 1, position, rotation: [0.0; 3], spawn_kind: 0, spawn_arg: 0, serial, raw: [0; 0x40] }).collect(),
            header: [0; 0x2c],
            activators: Vec::new(),
            paths: Vec::new(),
            returns: Vec::new(),
            signals: Vec::new(),
        }
    }

    #[derive(Default)]
    struct World {
        live: Vec<(u16, u32, [f32; 3])>,
        cached: Vec<u16>,
        appeared: Vec<usize>,
        gone: Vec<u32>,
    }

    impl UnitHost for World {
        fn player_position(&self) -> [f32; 3] {
            [0.0; 3]
        }
        fn player_facing(&self) -> [f32; 3] {
            [0.0, 0.0, 1.0]
        }
        fn find_object(&mut self, serial: u16) -> Option<u32> {
            self.live.iter().find(|l| l.0 == serial).map(|l| l.1)
        }
        fn is_cached(&mut self, serial: u16) -> bool {
            self.cached.contains(&serial)
        }
        fn object_position(&self, obj: u32) -> [f32; 3] {
            self.live.iter().find(|l| l.1 == obj).unwrap().2
        }
        fn appear(&mut self, entry: usize) {
            self.appeared.push(entry);
        }
        fn disappear(&mut self, obj: u32) {
            self.gone.push(obj);
        }
    }

    const LIMITS: StreamLimits = StreamLimits { near: 100.0, facing: 0.5 };

    #[test]
    fn units_in_front_and_in_range_appear_and_cached_ones_do_not() {
        let g = group(&[(1, [0.0, 0.0, 500.0]), (2, [0.0, 0.0, -500.0]), (3, [0.0, 0.0, 5000.0]), (4, [0.0, 0.0, 500.0])]);
        let mut w = World { cached: vec![4], ..World::default() };
        stream_units(&mut w, &g, 1000.0, LIMITS);
        assert_eq!(w.appeared, [0]);
        assert!(w.gone.is_empty());
    }

    #[test]
    fn live_units_stay_inside_the_range_and_go_away_behind_or_beyond() {
        let g = group(&[(1, [0.0; 3]), (2, [0.0; 3]), (3, [0.0; 3]), (4, [0.0; 3])]);
        let mut w = World {
            live: vec![(1, 11, [0.0, 0.0, 500.0]), (2, 12, [0.0, 0.0, -500.0]), (3, 13, [0.0, 0.0, -50.0]), (4, 14, [0.0, 0.0, 2000.0])],
            ..World::default()
        };
        stream_units(&mut w, &g, 1000.0, LIMITS);
        // 1 in front stays, 2 behind goes, 3 behind but near stays, 4 beyond the radius goes.
        assert_eq!(w.gone, [12, 14]);
        assert!(w.appeared.is_empty());
    }

    #[test]
    fn the_request_object_records_the_player_by_flag() {
        let mut obj = vec![0u8; 0x40];
        obj[8] = 0x10;
        fill_request(Some(&mut obj), 0xabcd, 0x77, 1);
        assert_eq!(u32::from_le_bytes(obj[4..8].try_into().unwrap()), 0x77);
        assert_eq!(u32::from_le_bytes(obj[8..12].try_into().unwrap()), 0x11);
        assert_eq!(u32::from_le_bytes(obj[0x34..0x38].try_into().unwrap()), 0xabcd);
        fill_request(Some(&mut obj), 0x1111, 0x78, 0);
        assert_eq!(u32::from_le_bytes(obj[8..12].try_into().unwrap()), 0x13);
        assert_eq!(u32::from_le_bytes(obj[0x38..0x3c].try_into().unwrap()), 0x1111);
        fill_request(None, 1, 2, 3);
    }
}
