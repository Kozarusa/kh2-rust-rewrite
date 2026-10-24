//! kh2-rs game binary: all code lives in the `kh2_app` library.
//!
//! The release build is a windows-subsystem program (no console window); `--console` brings one back, and the log goes
//! to `kh2-rs.log` in the config folder. Debug builds keep their console.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    kh2_app::run();
}
