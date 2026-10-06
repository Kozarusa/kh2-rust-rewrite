//! Message windows: the framed text boxes of events and dialogue. Rules read from the game's
//! window classes in a local decompilation of `SLUS_210.05` (docs/hud.md, "Message windows").
//!
//! The frame is a nine-part sprite group of the `md_m` SEQD in `msg/<lang>/fontinfo.bar`, drawn
//! from the `md_m` IMGD of the current world's message file `msg/<lang>/<world>.bar` (the system
//! and special message files hold an empty one). Each window kind (0 to 9) owns three consecutive
//! animation groups: opening, idle and closing. Before every draw the frame's parts are stretched
//! around the text (`Seqd::resize_frame`), and the window centres the text in it.

/// First animation group of the frames: kind `k` uses `FIRST_GROUP + 3k` (opening), `+ 1` (idle)
/// and `+ 2` (closing). The game reads them from the string "CDEF...`" at 0x35ea48, one character
/// per group (0x43 is 67); 0x1aaf20 starts the opening group, 0x1ab170 switches to the idle and
/// closing ones.
pub const FIRST_GROUP: usize = 0x43;
/// Number of frame kinds in the string.
pub const KINDS: usize = 10;

/// The button prompt the window shows once its text is complete (0x1aa3a0 starts group 0x41 when
/// the text has no selection slots; 0x1aa3f8 plays 0x42 when it closes). Drawn at the prompt point.
pub const PROMPT_GROUP: usize = 0x41;
pub const PROMPT_CLOSE_GROUP: usize = 0x42;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Open = 0,
    Idle = 1,
    Close = 2,
}

/// Animation group of frame `kind` in `phase`.
pub fn frame_group(kind: usize, phase: Phase) -> usize {
    FIRST_GROUP + 3 * kind + phase as usize
}

/// Size of the text box a window frames: the measured text block with both sides rounded up to
/// even pixels (0x191570 measures every page of the message, keeps the widest and tallest, and
/// rounds each up to even).
pub fn text_box(size: [f32; 2]) -> [i32; 2] {
    let even = |v: f32| (v.max(0.0) as i32 + 1) & !1;
    [even(size[0]), even(size[1])]
}

/// The window classes the game constructs (each sets up the shared base 0x1aaf20 with these
/// values). `x`, `y` is where the text is centred; the frame is drawn there too.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Class {
    /// Constructor address, for reference.
    pub ctor: u32,
    pub font: usize,
    /// Glyph size in pixels (`None` = the font's own).
    pub size: Option<f32>,
    /// Colour id of the `ftst` table (03system.bin), looked up for the current world (0x1a7df8).
    pub color: u32,
    pub x: i32,
    pub y: i32,
    /// Frame kind (0 to 9).
    pub kind: usize,
    /// Text alignment (the text object's +0x58, 0x1a8018): 3 centres both ways, 2 centres x only.
    pub align: u8,
    /// Inner width of the frame when the class fixes it (0x1ab990 passes 432), else the text's.
    pub width: Option<i32>,
    /// The class moves its y down by half the text height after measuring (0x1ab830).
    pub y_from_top: bool,
}

/// The window classes found:
/// - 0: 0x1ab6a8 (vtable 0x35eb80), the event message of kind 0 (0x2275a8) and the window that
///   0x172360 and 0x2c5cf0 open; font 3, colour 5, centred at (128, 104) unless the caller moves it.
/// - 1: 0x1ab8e0 (vtable 0x35eac0), the event message of kind 1 (0x2275a8, 0x2c5cf0): font 0 at
///   16 pixels, colour 6, top centre (256, 52) with the text's top there, frame kind 4, 432 wide.
/// - 2: 0x1ab7d8 (vtable 0x35eb00), opened by 0x1a5608: font 3, colour 4, (256, 32), frame kind 3,
///   text centred on x only with its top at y (alignment 2) and the frame half its height lower.
/// - 3: 0x1ab9e0 (vtable 0x35eb40): font 3, colour 5, (256, 336), frame kind 0 (its caller was not
///   found in the decompiled set).
pub const CLASSES: [Class; 4] = [
    Class { ctor: 0x1ab6a8, font: 3, size: None, color: 5, x: 0x80, y: 0x68, kind: 0, align: 3, width: None, y_from_top: false },
    Class { ctor: 0x1ab8e0, font: 0, size: Some(16.0), color: 6, x: 0x100, y: 0x34, kind: 4, align: 3, width: Some(432), y_from_top: true },
    Class { ctor: 0x1ab7d8, font: 3, size: None, color: 4, x: 0x100, y: 0x20, kind: 3, align: 2, width: None, y_from_top: false },
    Class { ctor: 0x1ab9e0, font: 3, size: None, color: 5, x: 0x100, y: 0x150, kind: 0, align: 3, width: None, y_from_top: false },
];

/// Where a window of `class` with text box `bx` draws: the text origin, the frame's centre, the
/// frame's inner size and the prompt point (0x1ab4e8, 0x1ab990 and 0x1ab370).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub text: [i32; 2],
    pub frame: [i32; 2],
    pub size: [i32; 2],
    pub prompt: [i32; 2],
}

pub fn place(class: &Class, bx: [i32; 2]) -> Placement {
    let (w, h) = (bx[0], bx[1]);
    let x = class.x;
    let y = if class.y_from_top { class.y + (h >> 1) } else { class.y };
    let size = [class.width.unwrap_or(w), h];
    if class.align == 2 {
        // 0x1ab4e8 with alignment 2: the frame is half the text height below y, the prompt at the
        // bottom-right of the text box.
        Placement { text: [x, y], frame: [x, y + (h >> 1)], size, prompt: [x + (w >> 1), y + h] }
    } else {
        // The prompt point is the text box's bottom-right corner (the text width, not the frame's).
        Placement { text: [x, y], frame: [x, y], size, prompt: [x + (w >> 1), y + (h >> 1)] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_groups_follow_the_kind_string() {
        // "CDE" for kind 0, "OPQ" for kind 4, "^_`" for kind 9.
        assert_eq!(frame_group(0, Phase::Open), b'C' as usize);
        assert_eq!(frame_group(0, Phase::Close), b'E' as usize);
        assert_eq!(frame_group(4, Phase::Open), b'O' as usize);
        assert_eq!(frame_group(4, Phase::Idle), b'P' as usize);
        assert_eq!(frame_group(9, Phase::Close), b'`' as usize);
    }

    #[test]
    fn text_boxes_round_up_to_even() {
        assert_eq!(text_box([101.0, 40.0]), [102, 40]);
        assert_eq!(text_box([100.0, 39.0]), [100, 40]);
    }

    #[test]
    fn frames_stretch_around_the_box() {
        use crate::seqd::{Anim, AnimGroup, Part, Seqd, SpriteGroup};
        // Nine 16x16 parts laid out like the files' frames, group margins 16 and 8.
        let mut parts = Vec::new();
        for y in [-45, -29, 30] {
            for x in [-46, -30, 29] {
                parts.push(Part { left: x, top: y, right: x + 16, bottom: y + 16, sprite: 0 });
            }
        }
        let mut seq = Seqd {
            parts,
            sprite_groups: vec![SpriteGroup { start: 0, count: 9 }],
            anims: vec![Anim::default()],
            groups: vec![AnimGroup { first: 0, count: 1, x: 16, y: 8, ..AnimGroup::default() }],
            ..Seqd::default()
        };
        seq.resize_frame(0, 100, 40);
        let rect = |seq: &Seqd, i: usize| {
            let p = seq.parts[i];
            [p.left, p.top, p.right, p.bottom]
        };
        let r = |i: usize| rect(&seq, i);
        // Inner box 116 x 48 centred on the origin; corners outside it, edges along it.
        assert_eq!(r(4), [-58, -24, 58, 24]);
        assert_eq!(r(0), [-74, -40, -58, -24]);
        assert_eq!(r(1), [-58, -40, 58, -24]);
        assert_eq!(r(2), [58, -40, 74, -24]);
        assert_eq!(r(3), [-74, -24, -58, 24]);
        assert_eq!(r(5), [58, -24, 74, 24]);
        assert_eq!(r(6), [-74, 24, -58, 40]);
        assert_eq!(r(7), [-58, 24, 58, 40]);
        assert_eq!(r(8), [58, 24, 74, 40]);
        // Odd sums: the full size rounds down to even, the far half uses the arithmetic shift.
        seq.resize_frame(0, 101, 41);
        assert_eq!(rect(&seq, 4), [-58, -24, 58, 24]);
        assert_eq!(rect(&seq, 8), [58, 24, 74, 40]);
    }

    #[test]
    fn placements_follow_the_draw_calls() {
        let p = place(&CLASSES[0], [100, 40]);
        assert_eq!(p, Placement { text: [128, 104], frame: [128, 104], size: [100, 40], prompt: [178, 124] });
        // The top bar: y moves down by half the text, the frame is 432 wide.
        let p = place(&CLASSES[1], [200, 20]);
        assert_eq!(p, Placement { text: [256, 62], frame: [256, 62], size: [432, 20], prompt: [356, 72] });
        // Alignment 2: the frame centre is half the text height below the text's top.
        let p = place(&CLASSES[2], [200, 20]);
        assert_eq!(p, Placement { text: [256, 32], frame: [256, 42], size: [200, 20], prompt: [356, 52] });
    }
}
