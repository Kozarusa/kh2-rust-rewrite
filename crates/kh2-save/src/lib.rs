//! The memory card unit of the original (core runtime, WP-L1-P9; spec `docs/systems/core-runtime/memcard.md`).
//!
//! Two layers, as in the original: the save file layer (`0x23c830..0x23e598`: the save body checks and its CRC, the slot table of the
//! card info block, the directory scans, the file names, `icon.sys`) and the card driver's request ring (`0x230e90..0x2331b0`). The
//! PCSX2 card image (`.ps2`) is not part of the game (the original talks to the IOP's card library); [`image`] reads and writes it so
//! the port can load the saves a PCSX2 card holds and write ones the original loads.
//!
//! Nothing of the game is stored here: tables the original reads from its executable (the icon colours, the title strings, the file
//! name parts, the icon variant flag pairs) come in through [`icon::IconEnv`], [`names::NameEnv`] and the arguments of the functions.
//! Nothing in the running game calls this crate except the loader behind the environment variable `KH2_CARD` ([`load`]).

pub mod body;
pub mod crc;
pub mod dir;
pub mod handlers;
pub mod icon;
pub mod image;
pub mod load;
pub mod names;
pub mod ops;
pub mod ring;
pub mod slot;
