//! The keyed persistent buffers of host 2.21 (`shared_block`, wrapper 0x18e928 -> FUN_001a5040).
//!
//! The game keeps a list of (key, buffer) pairs at 0x1c95528 with its count at 0x1c95568. FUN_001a5040
//! looks the key up: a buffer already registered gets its first byte set to 1 and is returned; otherwise a
//! buffer of size + 1 bytes is allocated (FUN_00104fa8), zeroed (FUN_00310008), registered and returned,
//! with its first byte still 0. So a script can tell its first call (byte 0 = 0) from a later one.
//! The list is emptied by FUN_001a5030 (count = 0, from the battle set-up FUN_00168ba8) and by
//! FUN_001a5108 (frees every buffer, from the battle tear-down FUN_00168ce8).
//!
//! Here the buffers live outside the script's memory at made-up addresses; a host answers the
//! interpreter's reads and writes there through `read` / `write`.

/// First address handed out (above every address the app's hosts use for objects).
pub const BASE: u32 = 0x7a00_0000;

#[derive(Clone, Debug, Default)]
pub struct SharedBlocks {
    /// (key, address, bytes) in registration order.
    list: Vec<(i32, u32, Vec<u8>)>,
    next: u32,
}

impl SharedBlocks {
    /// FUN_001a5040(key, size): the buffer's address.
    pub fn get(&mut self, key: i32, size: i32) -> u32 {
        if let Some((_, addr, b)) = self.list.iter_mut().find(|(k, ..)| *k == key) {
            if let Some(first) = b.first_mut() {
                *first = 1;
            }
            return *addr;
        }
        let len = (size.max(0) as u32).saturating_add(1);
        let addr = BASE + self.next;
        // Each buffer starts on a 16-byte boundary (the allocator's alignment is not read: [inferred]).
        self.next += len.div_ceil(16) * 16;
        self.list.push((key, addr, vec![0; len as usize]));
        addr
    }

    /// FUN_001a5030 / FUN_001a5108: the list is emptied.
    pub fn clear(&mut self) {
        self.list.clear();
        self.next = 0;
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    fn at(&mut self, addr: u32, n: usize) -> Option<&mut [u8]> {
        self.list.iter_mut().find_map(|(_, a, b)| {
            let o = addr.checked_sub(*a)? as usize;
            b.get_mut(o..o.checked_add(n)?)
        })
    }

    pub fn read(&mut self, addr: u32, out: &mut [u8]) -> bool {
        match self.at(addr, out.len()) {
            Some(b) => {
                out.copy_from_slice(b);
                true
            }
            None => false,
        }
    }

    pub fn write(&mut self, addr: u32, data: &[u8]) -> bool {
        match self.at(addr, data.len()) {
            Some(b) => {
                b.copy_from_slice(data);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_block_first_call_zeroed_later_calls_marked() {
        let mut s = SharedBlocks::default();
        let a = s.get(7, 8);
        let mut b = [9u8; 9];
        assert!(s.read(a, &mut b));
        assert_eq!(b, [0; 9], "first call: size + 1 zeroed bytes");
        assert!(!s.read(a + 6, &mut [0u8; 4]), "past the end");
        assert!(s.write(a + 4, &[1, 2, 3, 4]));
        let other = s.get(8, 3);
        assert_ne!(other, a);
        assert_eq!(s.get(7, 100), a, "the key finds its buffer, the size is not looked at");
        assert!(s.read(a, &mut b));
        assert_eq!(b, [1, 0, 0, 0, 1, 2, 3, 4, 0], "first byte set to 1, contents kept");
        s.clear();
        assert!(s.is_empty());
        assert!(!s.read(a, &mut [0u8; 1]));
    }
}
