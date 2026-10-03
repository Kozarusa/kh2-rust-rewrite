//! Gameplay tables, decoded at runtime from the user's own disc. Struct definitions are generated
//! from `sheets/tables.json`; no game values live in this crate.
//!
//! ```no_run
//! let mut a = kh2_archive::Archive::open("Kingdom Hearts II (USA).iso").unwrap();
//! let t = kh2_tables::load::<kh2_tables::tables::Plrp>(&mut a, &kh2_tables::tables::PLRP).unwrap();
//! println!("{} player rows", t.rows.len());
//! ```

pub mod rt;

pub mod tables {
    include!(concat!(env!("OUT_DIR"), "/tables.rs"));
}

pub mod systems {
    include!(concat!(env!("OUT_DIR"), "/systems.rs"));
}

pub use rt::{DecodeError, Decoded, DynTable, Record, TableDef};

/// Reads a table's archive from the disc, finds its BAR subfile and decodes it.
pub fn load<T: Record>(archive: &mut kh2_archive::Archive, def: &TableDef) -> Result<Decoded<T>, DecodeError> {
    let blob = subfile(archive, def)?;
    rt::decode::<T>(def, &blob)
}

/// The raw subfile bytes for a table (whole file when `tag` is empty).
pub fn subfile(archive: &mut kh2_archive::Archive, def: &TableDef) -> Result<Vec<u8>, DecodeError> {
    let file = archive.read(def.archive).map_err(|e| DecodeError::Archive(e.to_string()))?;
    if def.tag.is_empty() {
        return Ok(file);
    }
    let entries = kh2_bar::parse(&file).map_err(|e| DecodeError::Archive(format!("{}: {e:?}", def.archive)))?;
    kh2_bar::find(&entries, def.tag)
        .map(|e| e.data.to_vec())
        .ok_or_else(|| DecodeError::Archive(format!("{}: no '{}' subfile", def.archive, def.tag)))
}
