//! Runtime support for generated table code.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Header {
    /// u32 version, u32 count.
    Table,
    /// u16 version, u16 count.
    Short,
    /// A `Table` that follows another `Table` in the same subfile.
    SecondTable,
    /// Layout not modelled yet (unfinished sheet row).
    Unknown,
}

#[derive(Debug, Clone, Copy)]
pub struct TableDef {
    pub name: &'static str,
    pub archive: &'static str,
    /// BAR subfile tag; empty = the archive file itself is the table.
    pub tag: &'static str,
    pub header: Header,
    pub record_size: Option<usize>,
    /// For `SecondTable`: record size of the table before it.
    pub first_record_size: usize,
}

pub trait Record: Sized {
    const SIZE: usize;
    const FIELDS: &'static [&'static str];
    fn parse(r: &mut Reader<'_>) -> Self;
    fn cells(&self) -> Vec<String>;
}

/// Little-endian cursor. Callers check bounds once per record, so reads here cannot fail.
pub struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

macro_rules! prim {
    ($($n:ident $t:ty),*) => {$(
        pub fn $n(&mut self) -> $t {
            const N: usize = std::mem::size_of::<$t>();
            let v = <$t>::from_le_bytes(self.b[self.p..self.p + N].try_into().unwrap());
            self.p += N;
            v
        }
    )*};
}

impl<'a> Reader<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Reader { b, p: 0 }
    }
    prim!(u8 u8, i8 i8, u16 u16, i16 i16, u32 u32, i32 i32, f32 f32, u64 u64, i64 i64);
    /// Fixed-size Latin-1 string, cut at the first NUL.
    pub fn string(&mut self, n: usize) -> String {
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        s.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect()
    }
}

#[derive(Debug)]
pub enum DecodeError {
    Archive(String),
    NoLayout(&'static str),
    Truncated { table: &'static str, need: usize, have: usize },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Archive(s) => write!(f, "{s}"),
            DecodeError::NoLayout(t) => write!(f, "{t}: no layout in sheets/tables.json yet"),
            DecodeError::Truncated { table, need, have } => write!(f, "{table}: needs {need} bytes, subfile has {have}"),
        }
    }
}

impl std::error::Error for DecodeError {}

#[derive(Debug, Clone)]
pub struct Decoded<T> {
    pub version: u32,
    pub rows: Vec<T>,
    /// Bytes of the subfile consumed by header + records.
    pub used: usize,
    pub len: usize,
}

impl<T> Decoded<T> {
    pub fn trailing(&self) -> usize {
        self.len - self.used
    }
}

fn header(b: &[u8], short: bool) -> Option<(u32, usize, usize)> {
    if short {
        let h = b.get(0..4)?;
        Some((u16::from_le_bytes([h[0], h[1]]) as u32, u16::from_le_bytes([h[2], h[3]]) as usize, 4))
    } else {
        let h = b.get(0..8)?;
        Some((u32::from_le_bytes(h[0..4].try_into().ok()?), u32::from_le_bytes(h[4..8].try_into().ok()?) as usize, 8))
    }
}

pub fn decode<T: Record>(def: &TableDef, blob: &[u8]) -> Result<Decoded<T>, DecodeError> {
    let trunc = |need| DecodeError::Truncated { table: def.name, need, have: blob.len() };
    let start = match def.header {
        Header::Unknown => return Err(DecodeError::NoLayout(def.name)),
        Header::SecondTable => {
            let (_, n, hs) = header(blob, false).ok_or(trunc(8))?;
            hs + n * def.first_record_size
        }
        _ => 0,
    };
    let b = blob.get(start..).ok_or(trunc(start))?;
    let (version, n, hs) = header(b, def.header == Header::Short).ok_or(trunc(start + 8))?;
    let need = hs + n * T::SIZE;
    if need > b.len() {
        return Err(trunc(start + need));
    }
    let mut r = Reader::new(&b[hs..need]);
    let rows = (0..n).map(|_| T::parse(&mut r)).collect();
    Ok(Decoded { version, rows, used: start + need, len: blob.len() })
}

/// Type-erased table for tools (CLI listing, validation).
#[derive(Debug, Clone)]
pub struct DynTable {
    pub version: u32,
    pub fields: &'static [&'static str],
    pub rows: Vec<Vec<String>>,
    pub used: usize,
    pub len: usize,
}

impl<T: Record> From<Decoded<T>> for DynTable {
    fn from(d: Decoded<T>) -> Self {
        DynTable {
            version: d.version,
            fields: T::FIELDS,
            rows: d.rows.iter().map(Record::cells).collect(),
            used: d.used,
            len: d.len,
        }
    }
}
