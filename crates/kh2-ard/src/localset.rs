//! `07localset.bin`: the story programs' area table (docs/systems/events-movies/npc-savepoints.md section 4).
//!
//! The game loads the file into `DAT_01c8af88` (`FUN_00198058`) and a type-2 jump record asks it for
//! the area a story program belongs to (`FUN_00198080(out, key, world)`, called by `FUN_0019d8f0`
//! when the trigger's jump record is parsed). The file is a BAR; `FUN_001684c0(table, 2, world)`
//! picks the world's entry (the `world`-th entry of type 2). An entry is `u16 count` and `count`
//! pairs `(u8 program, u8 area)`; the first pair whose program equals the key gives the area.

/// The type-2 entry of `world` in the BAR `file` (the `world`-th entry of kind 2).
pub fn world_entry(file: &[u8], world: u8) -> Option<Vec<u8>> {
    let bar = kh2_bar::parse(file).ok()?;
    bar.iter().filter(|e| e.kind == 2).nth(world as usize).map(|e| e.data.to_vec())
}

/// The `(program, area)` pairs of one world's entry.
pub fn pairs(entry: &[u8]) -> Vec<(u8, u8)> {
    let n = entry.get(..2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as usize);
    (0..n).filter_map(|k| Some((*entry.get(2 + 2 * k)?, *entry.get(3 + 2 * k)?))).collect()
}

/// `FUN_00198080`'s search: the area of the first pair whose program is `key` (a key above 0xff
/// matches no byte).
pub fn area_for(entry: &[u8], key: u16) -> Option<u8> {
    let key = u8::try_from(key).ok()?;
    pairs(entry).into_iter().find(|p| p.0 == key).map(|p| p.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_pair_with_the_key_gives_the_area() {
        // count 3: (0x4c, 4), (0xb5, 4), (0xb5, 9).
        let e = [3, 0, 0x4c, 4, 0xb5, 4, 0xb5, 9];
        assert_eq!(pairs(&e), vec![(0x4c, 4), (0xb5, 4), (0xb5, 9)]);
        assert_eq!(area_for(&e, 0xb5), Some(4));
        assert_eq!(area_for(&e, 0x4c), Some(4));
        assert_eq!(area_for(&e, 0x4d), None);
        assert_eq!(area_for(&e, 0x1b5), None, "a key above a byte matches nothing");
        // A count larger than the data stops at the data.
        assert_eq!(pairs(&[9, 0, 1, 2]), vec![(1, 2)]);
        assert_eq!(area_for(&[], 1), None);
    }
}
