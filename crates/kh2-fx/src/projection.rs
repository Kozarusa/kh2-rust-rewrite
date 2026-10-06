//! The game's projection as the effect system sees it (the matrix copied to 0x1db1000 by
//! FUN_001e44f0 every frame, from the camera built by FUN_001078f0 / FUN_00106ba0 / FUN_00106c48 /
//! FUN_00106a68). The constants live in the executable and are read from the user's disc; the
//! formulas were read from those routines (nothing copied).
//!
//! Row convention: the game multiplies row vectors (`clip = x * row0 + y * row1 + z * row2 + row3`),
//! so the glam matrix here keeps each game row in a column and `matrix * v.extend(1)` is the clip
//! position. The screen position is `clip.xyz / clip.w` in GS pixels (12 bits, centre 2048) and the
//! GS z is `p22 + p32 / z` (larger means nearer; the sprite and model draws use ZTST GREATER).

use glam::{Mat4, Vec4};

/// Virtual address of the first PT_LOAD segment's bytes the floats are read from.
const ELF_PT_LOAD: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projection {
    /// The GS frame the x and y scales are stated for (DAT_00378100, DAT_00378104: 512 x 416).
    pub frame: [f32; 2],
    /// The scales of the x and y rows (camera +0x4c, +0x50 from DAT_00378138, DAT_0037813c): the
    /// 640 x 480 logical screen squeezed into the frame.
    pub stretch: [f32; 2],
    /// Screen centre in GS pixels (DAT_0037812c, both axes).
    pub centre: [f32; 2],
    /// DAT_003780fc (one half).
    pub half: f32,
    /// The z range constant of FUN_00106ba0 (DAT_003780f8).
    pub z_scale: f32,
    /// Near plane (DAT_00378130; FUN_001078f0 passes it both as the near distance and the z
    /// reference) and far plane (DAT_00378140).
    pub near: f32,
    pub far: f32,
}

fn pt_load(exe: &[u8]) -> Option<Vec<(u32, usize, usize)>> {
    let u16_ = |o: usize| exe.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32_ = |o: usize| exe.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if exe.get(..4)? != b"\x7fELF" {
        return None;
    }
    let (phoff, phentsize, phnum) = (u32_(0x1c)? as usize, u16_(0x2a)?, u16_(0x2c)?);
    let mut segs = Vec::new();
    for i in 0..phnum {
        let p = phoff + i * phentsize;
        if u32_(p)? == ELF_PT_LOAD {
            segs.push((u32_(p + 8)?, u32_(p + 4)? as usize, u32_(p + 16)? as usize));
        }
    }
    Some(segs)
}

impl Projection {
    /// Reads the constants from the game's executable (`SLUS_210.05`).
    pub fn from_exe(exe: &[u8]) -> Option<Projection> {
        let segs = pt_load(exe)?;
        let f = |va: u32| -> Option<f32> {
            let (vaddr, off, _) = segs.iter().copied().find(|&(v, _, n)| v <= va && va < v + n as u32)?;
            let o = off + (va - vaddr) as usize;
            exe.get(o..o + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };
        let p = Projection {
            frame: [f(0x378100)?, f(0x378104)?],
            stretch: [f(0x378138)?, f(0x37813c)?],
            centre: [f(0x37812c)?, f(0x37812c)?],
            half: f(0x3780fc)?,
            z_scale: f(0x3780f8)?,
            near: f(0x378130)?,
            far: f(0x378140)?,
        };
        (p.frame[0] > 0.0 && p.frame[1] > 0.0 && p.stretch[0] > 0.0 && p.stretch[1] > 0.0 && p.near > 0.0 && p.far > p.near).then_some(p)
    }

    /// Half the logical screen size in pixels (320 x 240 on the disc).
    fn half_screen(&self) -> [f32; 2] {
        [self.frame[0] / self.stretch[0] * self.half, self.frame[1] / self.stretch[1] * self.half]
    }

    /// Focal length in logical pixels for a horizontal field of view (FUN_00106c48: camera +0x40).
    pub fn focal(&self, hfov: f32) -> f32 {
        self.half_screen()[0] / (hfov * self.half).tan()
    }

    /// Vertical field of view for a horizontal one (FUN_00106c48: camera +0x48).
    pub fn vfov(&self, hfov: f32) -> f32 {
        let a = (self.half_screen()[1] / self.focal(hfov)).atan();
        a + a
    }

    /// The horizontal field of view whose vertical one is `vfov` (the inverse of [`Self::vfov`]).
    pub fn hfov_from_vfov(&self, vfov: f32) -> f32 {
        let [hw, hh] = self.half_screen();
        let focal = hh / (vfov * self.half).tan();
        2.0 * (hw / focal).atan()
    }

    /// The z row: (p22, p32) with GS z = p22 + p32 / z (FUN_00106ba0: camera +0x60 and +0x54).
    pub fn z_row(&self) -> (f32, f32) {
        let (c, near, far) = (self.z_scale, self.near, self.far);
        let range = c - near;
        ((near * c + -far * near) / range, (near * c * (far - near)) / range)
    }

    /// The projection for a horizontal field of view (FUN_00106a68: the focal-length matrix times
    /// the frame matrix), game rows in glam columns.
    pub fn matrix(&self, hfov: f32) -> Mat4 {
        let f = self.focal(hfov);
        let (p22, p32) = self.z_row();
        Mat4::from_cols(
            Vec4::new(self.stretch[0] * f, 0.0, 0.0, 0.0),
            Vec4::new(0.0, self.stretch[1] * f, 0.0, 0.0),
            Vec4::new(self.centre[0], self.centre[1], p22, 1.0),
            Vec4::new(0.0, 0.0, p32, 0.0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The constants as they are on the USA disc, for tests without one.
    pub(crate) fn disc_like() -> Projection {
        Projection { frame: [512.0, 416.0], stretch: [0.8, 0.8666667], centre: [2048.0, 2048.0], half: 0.5, z_scale: 4.0e6, near: 1.0, far: 16_770_000.0 }
    }

    #[test]
    fn screen_and_fov_round_trip() {
        let p = disc_like();
        let m = p.matrix(1.5);
        // A point on the view axis lands on the screen centre; one at the edge of the field of view
        // on the frame's edge (256 GS pixels off centre).
        let c = m * Vec4::new(0.0, 0.0, 100.0, 1.0);
        assert!((c.x / c.w - 2048.0).abs() < 1e-3 && (c.y / c.w - 2048.0).abs() < 1e-3);
        let edge = m * Vec4::new((0.75f32).tan() * 100.0, 0.0, 100.0, 1.0);
        assert!((edge.x / edge.w - (2048.0 + 256.0)).abs() < 1e-2, "{}", edge.x / edge.w);
        let v = p.vfov(1.5);
        assert!((p.hfov_from_vfov(v) - 1.5).abs() < 1e-5);
        let (p22, p32) = p.z_row();
        assert!(p22 < 0.0 && p32 > 1.6e7);
    }

    #[test]
    fn reads_the_constants_from_an_elf() {
        // A one-segment ELF image with the floats at their addresses.
        let base = 0x378000u32;
        let mut body = vec![0u8; 0x200];
        let mut put = |va: u32, v: f32| body[(va - base) as usize..(va - base) as usize + 4].copy_from_slice(&v.to_le_bytes());
        let d = disc_like();
        put(0x378100, d.frame[0]);
        put(0x378104, d.frame[1]);
        put(0x378138, d.stretch[0]);
        put(0x37813c, d.stretch[1]);
        put(0x37812c, d.centre[0]);
        put(0x3780fc, d.half);
        put(0x3780f8, d.z_scale);
        put(0x378130, d.near);
        put(0x378140, d.far);
        let mut elf = vec![0u8; 0x40];
        elf[..4].copy_from_slice(b"\x7fELF");
        elf[0x1c..0x20].copy_from_slice(&0x40u32.to_le_bytes());
        elf[0x2a..0x2c].copy_from_slice(&0x20u16.to_le_bytes());
        elf[0x2c..0x2e].copy_from_slice(&1u16.to_le_bytes());
        let mut ph = vec![0u8; 0x20];
        ph[..4].copy_from_slice(&1u32.to_le_bytes());
        ph[4..8].copy_from_slice(&0x60u32.to_le_bytes());
        ph[8..12].copy_from_slice(&base.to_le_bytes());
        ph[16..20].copy_from_slice(&(body.len() as u32).to_le_bytes());
        elf.extend_from_slice(&ph);
        elf.extend_from_slice(&body);
        assert_eq!(Projection::from_exe(&elf), Some(d));
        assert_eq!(Projection::from_exe(&elf[..0x30]), None);
    }
}
