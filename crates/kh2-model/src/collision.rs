//! Object collision (MDLX BAR type 23): bone-attached shapes for hit/hurt/guard/etc.
//! Layout per OpenKH `docs/kh2/file/type/objectcollision.md` (Apache-2.0).

#[derive(Debug, Clone, Copy)]
pub struct Shape {
    pub group: u8,
    pub parts: u8,
    pub argument: i16,
    /// 2 battle (hurt), 3 target, 6 attack, 13 guard, ... (see the doc table).
    pub kind: u8,
    /// 0 ellipsoid, 1 column, 2 cube, 3 sphere.
    pub shape: u8,
    /// Model bone, or None when the shape follows the object root.
    pub bone: Option<usize>,
    pub offset: [f32; 3],
    /// The short after the offset: the game moves a target point up by this much (FUN_001894d0).
    pub lift: f32,
    pub radius: f32,
    pub height: f32,
}

pub mod kind {
    pub const BATTLE: u8 = 2;
    pub const TARGET: u8 = 3;
    pub const ATTACK: u8 = 6;
    pub const GUARD: u8 = 13;
}

pub fn parse(b: &[u8]) -> Vec<Shape> {
    let n = b.get(0..4).map(|s| u32::from_le_bytes(s.try_into().unwrap()) as usize).unwrap_or(0);
    let i16_at = |o: usize| b.get(o..o + 2).map(|s| i16::from_le_bytes([s[0], s[1]])).unwrap_or(0);
    (0..n.min(1000))
        .filter_map(|i| {
            let o = 64 + 20 * i;
            b.get(o + 19)?;
            let bone = i16_at(o + 6);
            Some(Shape {
                group: b[o],
                parts: b[o + 1],
                argument: i16_at(o + 2),
                kind: b[o + 4],
                shape: b[o + 5],
                bone: (bone != 16384 && bone >= 0).then_some(bone as usize),
                offset: [i16_at(o + 8) as f32, i16_at(o + 10) as f32, i16_at(o + 12) as f32],
                lift: i16_at(o + 14) as f32,
                radius: i16_at(o + 16) as f32,
                height: i16_at(o + 18) as f32,
            })
        })
        .collect()
}
