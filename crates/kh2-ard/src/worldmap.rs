//! The world map's data tables (docs/worldmap.md sections 3, 6 and 7).
//!
//! Layouts only: the values stay on the disc. `00progress.bin` holds three counted tables next to
//! the story lists (`wldf`, `wlds`, `epsd`); each entry starts with a word, then an `i32` record count
//! at `+4`, and the records from `+8` (`FUN_001b68c0` reads `DAT_01d5d790 + 4` and `+ 8`,
//! `FUN_001c1cf8` reads `DAT_01d5df68 + 4` and `+ 8`, `FUN_001c14e8` the episode table the same way).
//! `70landing.bar` keeps the landing rows and `msg/us/wm.bar` the world map's texts.
//!
//! Every count is bounded by the bytes present: a record count larger than the data gives the records
//! that fit, never a larger allocation.

/// The BAR tag of the world flag table in `00progress.bin` (`FUN_0017fd10` binds the entry to
/// `DAT_01d5d790` through `FUN_001b6860`).
pub const WLDF: &str = "wldf";
/// The BAR tag of the landing table in `00progress.bin` (`FUN_001c1ce8` keeps it at `DAT_01d5df68`).
pub const WLDS: &str = "wlds";
/// The BAR tag of the episode table in `00progress.bin` (`FUN_001c1400` keeps it at `DAT_01d5df60`).
pub const EPSD: &str = "epsd";

/// The records of a counted table: `u32` word, `i32` count, then `count` records of `N` bytes. Fewer
/// when the data ends first; none when the header is missing or the count is negative.
fn counted<const N: usize>(entry: &[u8]) -> Vec<[u8; N]> {
    let Some(count) = entry.get(4..8).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]])) else { return Vec::new() };
    let body = entry.get(8..).unwrap_or(&[]);
    body.as_chunks::<N>().0.iter().take(usize::try_from(count).unwrap_or(0)).copied().collect()
}

fn u16_le(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

fn u32_le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// One record of `wldf` (16 bytes): which world flag follows which story flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldFlag {
    /// `+0`: the world flag index (bit of the array at `0x3418a4`); the sort key of the table.
    pub index: u8,
    /// `+1`: the kind (0, 1 or 2); kind 1 is not queued while story flag `0x102b` is set (`FUN_001b68f8`).
    pub kind: u8,
    /// `+4`: the story flag this world flag follows (`FUN_001b6da0` compares it with the flag just set).
    pub story_flag: u32,
    /// `+8`: an event number, 0 for none (`FUN_001b6a48` callers; 13 of them are the `wm` librettos).
    pub event: u32,
    /// `+12`: the offset of the action list inside the entry, 0 for none (`FUN_001b6a48`).
    pub list: u32,
}

/// One command of an action list: `u16 op, u16 count`, then `count` pairs `(u16 id, u16 value)`
/// (`FUN_001d8d18` walks the list, `FUN_001b6a90` runs a command).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub op: u16,
    /// The `2 * count` halfwords after the header.
    pub data: Vec<u16>,
}

impl Action {
    /// The first halfword of each pair: the world, flag or episode the command names.
    pub fn ids(&self) -> impl Iterator<Item = u16> + '_ {
        self.data.chunks(2).map(|p| p[0])
    }
}

/// The `wldf` entry: the records and the entry bytes their action lists point into.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wldf {
    entry: Vec<u8>,
    pub records: Vec<WorldFlag>,
}

impl Wldf {
    /// Parses the entry (147 records of 16 bytes on the disc).
    pub fn parse(entry: &[u8]) -> Wldf {
        let records = counted::<16>(entry)
            .iter()
            .map(|r| WorldFlag { index: r[0], kind: r[1], story_flag: u32_le(&r[4..]), event: u32_le(&r[8..]), list: u32_le(&r[12..]) })
            .collect();
        Wldf { entry: entry.to_vec(), records }
    }

    /// `FUN_001b68c0`: the binary search the game runs on the index byte (`FUN_0030ff40`, comparing the
    /// key with `+0`); the same probe order, so a repeated key gives the same record [the compare
    /// function `0x332e08` has no decompilation: a plain unsigned compare is assumed, the oracle in
    /// `progress.rs` shows the result agrees for every record].
    pub fn find(&self, key: u8) -> Option<&WorldFlag> {
        let n = self.records.len();
        // `span` is the search span of the game's compare loop, `top` its upper bound and `lo` its lower bound.
        let (mut lo, mut top, mut span) = (0usize, n, n);
        if n == 0 {
            return None;
        }
        loop {
            let mid = span >> 1;
            let rec = &self.records[mid];
            let mut hi = mid;
            if key >= rec.index {
                lo = mid + 1;
                hi = top;
                if key == rec.index {
                    return Some(rec);
                }
            }
            span = lo + hi;
            top = hi;
            if lo >= hi {
                return None;
            }
        }
    }

    /// The records whose story flag is `flag`, in table order (`FUN_001b6da0`'s loop).
    pub fn following(&self, flag: u32) -> impl Iterator<Item = &WorldFlag> + '_ {
        self.records.iter().filter(move |r| r.story_flag == flag)
    }

    /// The action list at `offset` (a record's `list`): commands until op 0 or the end of the entry.
    /// A command whose pairs run past the entry ends the list.
    pub fn actions(&self, offset: u32) -> Vec<Action> {
        let mut out = Vec::new();
        let mut p = offset as usize;
        if offset == 0 {
            return out;
        }
        while let Some(head) = self.entry.get(p..p + 4) {
            let (op, count) = (u16_le(head), u16_le(&head[2..]) as usize);
            if op == 0 {
                break;
            }
            let Some(body) = self.entry.get(p + 4..p + 4 + 4 * count) else { break };
            out.push(Action { op, data: body.chunks(2).map(u16_le).collect() });
            p += 4 + 4 * count;
        }
        out
    }
}

/// One record of `wlds` (4 bytes): where a landing in a world goes (`FUN_001c1cf8`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Landing {
    pub world: u8,
    /// Matched with byte 1 of the world's record at `0x341858 + 4 * world`.
    pub state: u8,
    /// The area when below 0x33, else a story-program key (`FUN_00198080`).
    pub area: u8,
    pub entrance: u8,
}

/// Where `FUN_001c1cf8` sends a landing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    Area { area: u8, entrance: u8 },
    /// A story-program key; the area comes from `07localset` (`kh2_ard::localset::area_for`).
    Program { key: u8, entrance: u8 },
}

/// The `wlds` entry (34 records of 4 bytes).
pub fn parse_wlds(entry: &[u8]) -> Vec<Landing> {
    counted::<4>(entry).iter().map(|r| Landing { world: r[0], state: r[1], area: r[2], entrance: r[3] }).collect()
}

/// `FUN_001c1cf8`'s search: the first record of `world` whose state byte equals `state`.
pub fn landing(records: &[Landing], world: u8, state: u8) -> Option<Destination> {
    let r = records.iter().find(|r| r.world == world && r.state == state)?;
    Some(if r.area < 0x33 { Destination::Area { area: r.area, entrance: r.entrance } } else { Destination::Program { key: r.area, entrance: r.entrance } })
}

/// One record of `epsd` (8 bytes): an episode of a world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Episode {
    pub id: u16,
    pub world: u8,
    /// The order inside the world.
    pub order: u8,
    /// A text id of `wm.bar`.
    pub title: u16,
    /// A text id of `wm.bar`.
    pub hint: u16,
}

/// The `epsd` entry (43 records of 8 bytes).
pub fn parse_epsd(entry: &[u8]) -> Vec<Episode> {
    counted::<8>(entry).iter().map(|r| Episode { id: u16_le(r), world: r[2], order: r[3], title: u16_le(&r[4..]), hint: u16_le(&r[6..]) }).collect()
}

/// One record of the `land` table of `70landing.bar` (4 bytes), read by `FUN_002d1d20`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LandRow {
    /// The index into `00worldpoint.bin` (the point must have been seen).
    pub world_point: u8,
    pub world: u8,
    /// The row in the destination list.
    pub row: u8,
    /// The number of the picture entry of `70landing.bar`.
    pub picture: u8,
}

/// The BAR tag of the destination rows in `70landing.bar`.
pub const LAND: &str = "land";

/// The `land` entry (53 records of 4 bytes).
pub fn parse_land(entry: &[u8]) -> Vec<LandRow> {
    counted::<4>(entry).iter().map(|r| LandRow { world_point: r[0], world: r[1], row: r[2], picture: r[3] }).collect()
}

/// The BAR kind of the destination pictures in `70landing.bar` (type 24, an IMGD each): `FUN_002f7cc0` takes the `picture`
/// field of a `land` record as the number among the entries of this kind (`FUN_001684c0(bar, 0x18, n)`).
pub const PICTURE_KIND: u16 = 24;
/// The BAR tag of the sequence that draws a destination picture (type 25 SEQD, `FUN_002f7c90`).
pub const WS_I: &str = "ws_i";
/// Most pictures read (the file holds 53).
pub const MAX_PICTURES: usize = 64;

/// The picture entries of `70landing.bar` in file order (`FUN_002f7cc0`'s numbering), at most `MAX_PICTURES`.
pub fn pictures<'a>(entries: &[kh2_bar::Entry<'a>]) -> Vec<&'a [u8]> {
    entries.iter().filter(|e| e.kind == PICTURE_KIND).take(MAX_PICTURES).map(|e| e.data).collect()
}

/// One record of `00worldpoint.bin` (`FUN_00194f68` / `FUN_00194f08`): a save point or landing place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldPoint {
    pub world: u8,
    pub area: u8,
    pub entrance: u8,
}

/// Most records `FUN_00194f68` searches (the bit array at `0x33f910` holds 0x36 of them).
pub const WORLD_POINT_RECORDS: usize = 0x36;

/// The records of `00worldpoint.bin`: 4 bytes each (world, area, entrance, one unused), at most
/// `WORLD_POINT_RECORDS`.
pub fn parse_worldpoints(file: &[u8]) -> Vec<WorldPoint> {
    file.as_chunks::<4>().0.iter().take(WORLD_POINT_RECORDS).map(|r| WorldPoint { world: r[0], area: r[1], entrance: r[2] }).collect()
}

/// The ids of a message BAR such as `msg/us/wm.bar`: the first type-2 entry is `u32` version,
/// `u32` count, then `count` pairs `(u32 id, u32 offset)` (the layout `kh2_image::text::Messages`
/// reads). Ids whose pair is not in the data are not returned.
pub fn message_ids(bar: &[u8]) -> Vec<u32> {
    let Ok(entries) = kh2_bar::parse(bar) else { return Vec::new() };
    let Some(e) = entries.iter().find(|e| e.kind == 2) else { return Vec::new() };
    let count = e.data.get(4..8).map_or(0, |b| u32_le(b) as usize);
    e.data.get(8..).unwrap_or(&[]).as_chunks::<8>().0.iter().take(count).map(|p| u32_le(p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(count: i32, body: &[u8]) -> Vec<u8> {
        let mut v = vec![0, 0, 0, 0];
        v.extend_from_slice(&count.to_le_bytes());
        v.extend_from_slice(body);
        v
    }

    /// A `wldf` entry of three records (index, kind, story flag, event, list offset) and one list.
    fn sample() -> Vec<u8> {
        let rec = |index: u8, kind: u8, flag: u32, event: u32, list: u32| {
            let mut r = vec![index, kind, 0, 0];
            for w in [flag, event, list] {
                r.extend_from_slice(&w.to_le_bytes());
            }
            r
        };
        let mut body = Vec::new();
        body.extend(rec(3, 0, 0x100, 0, 0));
        body.extend(rec(9, 1, 0x200, 7, 8 + 48));
        body.extend(rec(20, 2, 0x100, 0, 0));
        // A list: op 5 on worlds 2 and 4, then the end.
        for h in [5u16, 2, 2, 0, 4, 0, 0, 0] {
            body.extend_from_slice(&h.to_le_bytes());
        }
        table(3, &body)
    }

    #[test]
    fn wldf_records_search_and_lists() {
        let w = Wldf::parse(&sample());
        assert_eq!(w.records.len(), 3);
        assert_eq!((w.records[1].index, w.records[1].kind, w.records[1].story_flag, w.records[1].event), (9, 1, 0x200, 7));
        for r in &w.records {
            assert_eq!(w.find(r.index), Some(r), "key {}", r.index);
        }
        assert_eq!(w.find(4), None);
        assert_eq!(w.find(255), None);
        assert_eq!(Wldf::default().find(0), None);
        assert_eq!(w.following(0x100).map(|r| r.index).collect::<Vec<_>>(), vec![3, 20]);
        let list = w.actions(w.records[1].list);
        assert_eq!(list, vec![Action { op: 5, data: vec![2, 0, 4, 0] }]);
        assert_eq!(list[0].ids().collect::<Vec<_>>(), vec![2, 4]);
        assert!(w.actions(0).is_empty());
    }

    #[test]
    fn a_count_larger_than_the_data_gives_the_records_that_fit() {
        let w = Wldf::parse(&table(i32::MAX, &sample()[8..8 + 48]));
        assert_eq!(w.records.len(), 3, "bounded by the data, not the count");
        assert!(Wldf::parse(&table(-1, &[0; 64])).records.is_empty());
        assert!(Wldf::parse(&[1, 2]).records.is_empty());
        // A list whose pairs run past the entry ends.
        let mut e = sample();
        e.truncate(e.len() - 6);
        assert!(Wldf::parse(&e).actions(8 + 48).is_empty());
        assert_eq!(parse_wlds(&table(i32::MAX, &[1, 2, 3, 4, 5, 6])).len(), 1);
    }

    #[test]
    fn landings_episodes_and_rows_read_their_bytes() {
        let wlds = parse_wlds(&table(3, &[2, 0, 4, 1, 2, 1, 0x40, 9, 5, 0, 0x32, 0]));
        assert_eq!(landing(&wlds, 2, 0), Some(Destination::Area { area: 4, entrance: 1 }));
        assert_eq!(landing(&wlds, 2, 1), Some(Destination::Program { key: 0x40, entrance: 9 }));
        assert_eq!(landing(&wlds, 5, 0), Some(Destination::Area { area: 0x32, entrance: 0 }));
        assert_eq!(landing(&wlds, 5, 1), None);
        let eps = parse_epsd(&table(1, &[0x34, 0x12, 7, 2, 0x02, 0x1e, 0x3b, 0x4c]));
        assert_eq!(eps, vec![Episode { id: 0x1234, world: 7, order: 2, title: 0x1e02, hint: 0x4c3b }]);
        let land = parse_land(&table(1, &[5, 2, 1, 3]));
        assert_eq!(land, vec![LandRow { world_point: 5, world: 2, row: 1, picture: 3 }]);
        let pts = parse_worldpoints(&[2, 4, 1, 0, 3, 5, 9, 0]);
        assert_eq!(pts, vec![WorldPoint { world: 2, area: 4, entrance: 1 }, WorldPoint { world: 3, area: 5, entrance: 9 }]);
        assert_eq!(parse_worldpoints(&vec![0; 4 * 100]).len(), WORLD_POINT_RECORDS);
    }

    #[test]
    fn pictures_are_the_type_24_entries_in_order() {
        let e = |kind: u16, tag: &'static str, data: &'static [u8]| kh2_bar::Entry { kind, tag, data };
        let entries = [e(2, "land", b"x"), e(25, "ws_i", b"s"), e(24, "AL02", b"a"), e(24, "TT27", b"t")];
        assert_eq!(pictures(&entries), vec![&b"a"[..], &b"t"[..]]);
        let many: Vec<_> = (0..100).map(|_| e(24, "P", b"p")).collect();
        assert_eq!(pictures(&many).len(), MAX_PICTURES, "bounded");
        assert!(pictures(&[]).is_empty());
    }

    #[test]
    fn message_ids_come_from_the_first_type_two_entry() {
        let mut sys = vec![1, 0, 0, 0, 2, 0, 0, 0];
        for (id, off) in [(0x4871u32, 24u32), (0x4884, 30)] {
            sys.extend_from_slice(&id.to_le_bytes());
            sys.extend_from_slice(&off.to_le_bytes());
        }
        let mut bar = b"BAR\x01".to_vec();
        bar.extend_from_slice(&1u32.to_le_bytes());
        bar.extend_from_slice(&[0; 8]);
        bar.extend_from_slice(&2u16.to_le_bytes());
        bar.extend_from_slice(&0u16.to_le_bytes());
        bar.extend_from_slice(b"wm\0\0");
        bar.extend_from_slice(&32u32.to_le_bytes());
        bar.extend_from_slice(&(sys.len() as u32).to_le_bytes());
        bar.extend_from_slice(&sys);
        assert_eq!(message_ids(&bar), vec![0x4871, 0x4884]);
        assert!(message_ids(b"nope").is_empty());
    }

    /// The disc, or `None` when `KH2_ISO` is not set or not there (the test is then skipped).
    fn open() -> Option<kh2_archive::Archive> {
        let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))?;
        if !std::path::Path::new(&path).exists() {
            return None;
        }
        let mut a = kh2_archive::Archive::open(&path).ok()?;
        let names = std::fs::read_to_string("../../local/kh2idx.txt").ok();
        let list: Vec<&str> = names.as_deref().map(|l| l.lines().collect()).unwrap_or_default();
        a.apply_names(if list.is_empty() { vec!["000tt.idx"] } else { list }).ok()?;
        Some(a)
    }

    /// WP-WM1 (docs/worldmap.md sections 6 and 7): the world map's tables read from the disc, with their counts and layouts.
    #[test]
    fn the_world_map_tables_have_the_documented_counts_and_layouts() {
        let Some(mut a) = open() else { return };
        let file = a.read("00progress.bin").expect("00progress.bin");
        let bar = kh2_bar::parse(&file).expect("BAR");
        let entry = |tag: &str| bar.iter().find(|e| e.tag == tag).unwrap_or_else(|| panic!("no {tag} entry")).data;

        let wldf = Wldf::parse(entry(WLDF));
        assert_eq!(wldf.records.len(), 147, "wldf records");
        assert!(wldf.records.windows(2).all(|w| w[0].index <= w[1].index), "wldf is sorted by its index byte (the binary search needs it)");
        let (mut lists, mut ops, mut used) = (0, 0, std::collections::BTreeSet::new());
        for r in &wldf.records {
            assert_eq!(wldf.find(r.index).map(|f| f.index), Some(r.index), "find({})", r.index);
            assert!(r.kind <= 2, "kind of {}", r.index);
            let acts = wldf.actions(r.list);
            lists += (!acts.is_empty()) as usize;
            ops += acts.len();
            used.extend(acts.iter().map(|x| x.op));
        }
        eprintln!("wldf: {lists} records with lists, {ops} ops, ops used {used:?}");
        assert_eq!((lists, ops), (131, 235));
        assert_eq!(used.into_iter().collect::<Vec<_>>(), vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 13, 15]);

        let wlds = parse_wlds(entry(WLDS));
        assert_eq!(wlds.len(), 34, "wlds records");
        assert!(wlds.iter().all(|r| r.world < 19), "wlds worlds");
        assert!(wlds.iter().any(|r| r.world == 2), "Twilight Town has a landing record");

        let epsd = parse_epsd(entry(EPSD));
        assert_eq!(epsd.len(), 43, "epsd records");
        assert!(epsd.iter().all(|e| e.world < 19 && e.title != 0), "epsd worlds and titles");

        let landing = a.read("70landing.bar").expect("70landing.bar");
        let lbar = kh2_bar::parse(&landing).expect("70landing BAR");
        eprintln!("70landing.bar entries: {:?}", lbar.iter().map(|e| (e.tag, e.kind, e.data.len())).collect::<Vec<_>>());
        let land = parse_land(lbar.iter().find(|e| e.tag == LAND).expect("land entry").data);
        assert_eq!(land.len(), 53, "land records");
        let pics = pictures(&lbar);
        assert_eq!(pics.len(), 53, "destination pictures");
        assert!(pics.iter().all(|p| p.len() == 17472 && p.starts_with(b"IMGD")), "each picture is an IMGD of 17472 bytes");
        assert!(lbar.iter().find(|e| e.tag == WS_I).is_some_and(|e| e.data.starts_with(b"SEQD")), "ws_i is a SEQD");
        assert!(land.iter().all(|r| (r.picture as usize) < pics.len()), "every row names a picture");
        let points = parse_worldpoints(&a.read("00worldpoint.bin").expect("00worldpoint.bin"));
        assert_eq!(points.len(), 54, "world points");
        assert!(land.iter().all(|r| (r.world_point as usize) < points.len() && r.world < 19), "land rows name a world point and a world");

        let ids = message_ids(&a.read("msg/us/wm.bar").expect("wm.bar"));
        eprintln!("wm.bar: {} ids", ids.len());
        for id in (0x4871..=0x4884).chain(0x4a4d..=0x4a55).chain([0x4c0d, 0x4c16]) {
            assert!(ids.contains(&id), "wm.bar lacks {id:#x}");
        }
        assert!(epsd.iter().all(|e| ids.contains(&u32::from(e.title))), "every episode title is a wm.bar text");
    }
}
