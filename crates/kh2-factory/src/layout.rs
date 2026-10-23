//! Layout-faithful state (RULES C7): typed little-endian accessors over a byte block that has the original's layout,
//! and the macro that declares a struct as such a block with named fields at the original's offsets.
//!
//! A block type made with [`layout!`] is a `[u8; N]` newtype: `Default`, `Clone`, a Bevy component when the caller
//! derives it, copied into and out of kh2-ee unchanged by an oracle test. Pointer fields stay in the block as handles
//! (an index the owner resolves, 0 for null), never host addresses.

pub trait Bytes {
    fn bytes(&self) -> &[u8];
    fn u8_at(&self, o: usize) -> u8 {
        self.bytes()[o]
    }
    fn u16_at(&self, o: usize) -> u16 {
        u16::from_le_bytes(self.bytes()[o..o + 2].try_into().unwrap())
    }
    fn i16_at(&self, o: usize) -> i16 {
        self.u16_at(o) as i16
    }
    fn u32_at(&self, o: usize) -> u32 {
        u32::from_le_bytes(self.bytes()[o..o + 4].try_into().unwrap())
    }
    fn i32_at(&self, o: usize) -> i32 {
        self.u32_at(o) as i32
    }
    fn f32_at(&self, o: usize) -> f32 {
        f32::from_bits(self.u32_at(o))
    }
    fn vec4_at(&self, o: usize) -> [f32; 4] {
        std::array::from_fn(|i| self.f32_at(o + 4 * i))
    }
}

pub trait BytesMut: Bytes {
    fn bytes_mut(&mut self) -> &mut [u8];
    fn set_u8(&mut self, o: usize, v: u8) {
        self.bytes_mut()[o] = v;
    }
    fn set_u16(&mut self, o: usize, v: u16) {
        self.bytes_mut()[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn set_u32(&mut self, o: usize, v: u32) {
        self.bytes_mut()[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn set_f32(&mut self, o: usize, v: f32) {
        self.set_u32(o, v.to_bits());
    }
    fn set_vec4(&mut self, o: usize, v: [f32; 4]) {
        for (i, x) in v.iter().enumerate() {
            self.set_f32(o + 4 * i, *x);
        }
    }
}

impl Bytes for [u8] {
    fn bytes(&self) -> &[u8] {
        self
    }
}

impl BytesMut for [u8] {
    fn bytes_mut(&mut self) -> &mut [u8] {
        self
    }
}

impl<const N: usize> Bytes for [u8; N] {
    fn bytes(&self) -> &[u8] {
        self
    }
}

impl<const N: usize> BytesMut for [u8; N] {
    fn bytes_mut(&mut self) -> &mut [u8] {
        self
    }
}

/// Declares a layout-faithful block: `layout!(pub Status, 0x200, { hp / set_hp: i32 @ 0x0, drive / set_drive: f32 @ 0x58 })`
/// gives `Status([u8; 0x200])` with the getters and setters at the original's offsets, `Bytes`/`BytesMut`, `SIZE`
/// and `from_bytes` (what an oracle test copies into and out of kh2-ee).
#[macro_export]
macro_rules! layout {
    ($vis:vis $name:ident, $size:expr, { $($get:ident / $set:ident : $ty:ident @ $off:expr),* $(,)? }) => {
        #[derive(Clone, PartialEq, Debug)]
        $vis struct $name(pub [u8; $size]);
        impl Default for $name {
            fn default() -> Self {
                $name([0; $size])
            }
        }
        impl $crate::layout::Bytes for $name {
            fn bytes(&self) -> &[u8] {
                &self.0
            }
        }
        impl $crate::layout::BytesMut for $name {
            fn bytes_mut(&mut self) -> &mut [u8] {
                &mut self.0
            }
        }
        impl $name {
            pub const SIZE: usize = $size;
            pub fn from_bytes(b: &[u8]) -> Self {
                let mut s = Self::default();
                s.0.copy_from_slice(&b[..$size]);
                s
            }
            $(
                pub fn $get(&self) -> $ty {
                    <$ty as $crate::layout::Field>::get(&self.0, $off)
                }
                pub fn $set(&mut self, v: $ty) {
                    <$ty as $crate::layout::Field>::put(&mut self.0, $off, v)
                }
            )*
        }
    };
}

/// A scalar that lives in a layout block.
pub trait Field: Sized {
    fn get(b: &[u8], o: usize) -> Self;
    fn put(b: &mut [u8], o: usize, v: Self);
}

macro_rules! field {
    ($($t:ty),*) => {$(
        impl Field for $t {
            fn get(b: &[u8], o: usize) -> Self {
                <$t>::from_le_bytes(b[o..o + std::mem::size_of::<$t>()].try_into().unwrap())
            }
            fn put(b: &mut [u8], o: usize, v: Self) {
                b[o..o + std::mem::size_of::<$t>()].copy_from_slice(&v.to_le_bytes());
            }
        }
    )*};
}
field!(u8, i8, u16, i16, u32, i32, u64, f32);
