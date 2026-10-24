//! Starting the game as a player: command line, disc discovery, log file, message boxes, console handling.
//!
//! The disc is looked up in this order: `KH2_ISO` (scripts and the rig), the `iso` key of the config file, `local/iso.txt`
//! (developers, written by `kh2 set-iso`), then a file dialog whose choice is saved to the config file.

use crate::config::{self, Config};
use std::path::{Path, PathBuf};

/// Text of `--help`: every flag and the environment variables a player may care about.
pub const HELP: &str = "\
kh2-rs: a Rust rewrite of Kingdom Hearts II. You need your own Kingdom Hearts II (USA) disc image (.iso).

Usage: kh2-game [options] [area]

  area             start area, for example tt04 (the default) or tt02
  --help, -h       show this text
  --choose-disc    ask for the disc image again, even if one is remembered
  --windowed       start in a window
  --fullscreen     start in borderless fullscreen
  --console        keep a console window with the log (the release build has none otherwise)

In the game: F11 or Alt+Enter toggles borderless fullscreen. The window size and fullscreen are remembered.

Files (folder: %APPDATA%\\kh2-rs, or the folder named by KH2_CONFIG_DIR):
  config.toml      the remembered disc path, window size and fullscreen
  kh2-rs.log       the log of this run (kh2-rs.1.log is the previous run)

Environment variables:
  KH2_ISO          path of the disc image; wins over the config file
  KH2_CONFIG_DIR   use this folder for config.toml and the log
  KH2_SAVE_DIR     folder for save slot files
";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Options {
    pub area: Option<String>,
    pub help: bool,
    pub choose_disc: bool,
    pub windowed: bool,
    pub fullscreen: bool,
    pub console: bool,
}

/// Parses the arguments after the program name. The first argument that is not a flag is the start area; unknown
/// flags are ignored so older scripts keep working.
pub fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Options {
    let mut o = Options::default();
    for a in args {
        match a.as_str() {
            "--help" | "-h" | "/?" => o.help = true,
            "--choose-disc" => o.choose_disc = true,
            "--windowed" => o.windowed = true,
            "--fullscreen" => o.fullscreen = true,
            "--console" => o.console = true,
            s if s.starts_with('-') => {}
            _ => {
                if o.area.is_none() {
                    o.area = Some(a);
                }
            }
        }
    }
    o
}

/// Checks that `path` is the USA disc: a readable ISO9660 image whose `SYSTEM.CNF` names `SLUS_210.05`. The message
/// is ready to show to the player.
pub fn check_image(path: &Path) -> Result<(), String> {
    let mut iso = kh2_iso::Iso::open(path).map_err(|e| format!("The file {} is not a readable disc image ({e}).", path.display()))?;
    let cnf = iso.read("SYSTEM.CNF").map_err(|_| format!("The file {} is a disc image but has no SYSTEM.CNF, so it is not a PlayStation 2 disc.", path.display()))?;
    match kh2_archive::check_disc(&String::from_utf8_lossy(&cnf), false) {
        Ok(_) => Ok(()),
        Err(_) => Err(format!(
            "The disc image {} is not the Kingdom Hearts II (USA) disc ({}). Only the USA release is supported; Final Mix and other editions cannot be used.",
            path.display(),
            kh2_archive::SUPPORTED_BOOT
        )),
    }
}

/// Finds the disc. `pick` is the file dialog (a closure so tests can stand in for it). A disc chosen through it is
/// saved to the config file in `cfg_dir`.
pub fn discover(env_iso: Option<PathBuf>, cfg_dir: &Path, local_txt: &Path, force_dialog: bool, pick: &mut dyn FnMut() -> Option<PathBuf>) -> Result<PathBuf, String> {
    if !force_dialog {
        if let Some(p) = env_iso {
            return Ok(p);
        }
        if let Some(p) = Config::load(cfg_dir).iso.filter(|p| p.exists()) {
            check_image(&p)?;
            return Ok(p);
        }
        if let Some(p) = std::fs::read_to_string(local_txt).ok().map(|s| PathBuf::from(s.trim())).filter(|p| p.exists()) {
            check_image(&p)?;
            return Ok(p);
        }
    }
    let Some(p) = pick() else {
        return Err(format!("No disc image was chosen. Start the game again and pick your Kingdom Hearts II (USA) .iso file. The choice is remembered in {}.", cfg_dir.join(config::CONFIG_FILE).display()));
    };
    check_image(&p)?;
    let mut cfg = Config::load(cfg_dir);
    cfg.iso = Some(p.clone());
    if let Err(e) = cfg.save(cfg_dir) {
        return Err(format!("The disc was recognised but the choice could not be saved to {} ({e}).", cfg_dir.join(config::CONFIG_FILE).display()));
    }
    Ok(p)
}

/// The native file dialog.
pub fn pick_disc_dialog() -> Option<PathBuf> {
    rfd::FileDialog::new().set_title("Choose your Kingdom Hearts II (USA) disc image").add_filter("Disc image (*.iso)", &["iso"]).pick_file()
}

/// A native message box; the text also goes to stderr (the log).
pub fn message_box(title: &str, text: &str) {
    eprintln!("{title}: {text}");
    rfd::MessageDialog::new().set_title(title).set_description(text).set_level(rfd::MessageLevel::Error).show();
}

/// A native information box (used for `--help` when there is no console).
pub fn info_box(title: &str, text: &str) {
    rfd::MessageDialog::new().set_title(title).set_description(text).set_level(rfd::MessageLevel::Info).show();
}

/// Shows `text` and ends the process with status 1.
pub fn fail(text: &str) -> ! {
    message_box("kh2-rs", text);
    std::process::exit(1);
}

pub const LOG_FILE: &str = "kh2-rs.log";

/// Moves the previous log to `kh2-rs.1.log` (the older one is dropped) and returns the path of the new log.
pub fn rotate_log(dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let (cur, old) = (dir.join(LOG_FILE), dir.join("kh2-rs.1.log"));
    if cur.exists() {
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&cur, &old)?;
    }
    Ok(cur)
}

/// Sends stdout and stderr to the log file (Windows) and makes panics land there too, with a message box that names
/// the file. Elsewhere only the panic hook is installed.
pub fn redirect_log(dir: &Path) {
    let Ok(path) = rotate_log(dir) else { return };
    #[cfg(windows)]
    {
        use std::os::windows::io::IntoRawHandle;
        use windows_sys::Win32::System::Console::{STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle};
        if let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let h = f.into_raw_handle();
            // SAFETY: the handle stays open for the life of the process; both standard handles share it.
            unsafe {
                SetStdHandle(STD_OUTPUT_HANDLE, h as _);
                SetStdHandle(STD_ERROR_HANDLE, h as _);
            }
        }
    }
    let log = path.clone();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("PANIC: {info}");
        previous(info);
        message_box("kh2-rs stopped unexpectedly", &format!("The game hit an internal error and has to close.\n\nDetails are in the log file:\n{}", log.display()));
    }));
}

/// Gives the windows-subsystem build a console again: the parent's if there is one (a terminal), else a new one when
/// `allow_new`. Returns whether standard output now goes to a console.
pub fn attach_console(allow_new: bool) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::io::IntoRawHandle;
        use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle};
        // SAFETY: plain Win32 calls with no pointers.
        let ok = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) != 0 || (allow_new && AllocConsole() != 0) };
        if ok && let Ok(f) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
            let h = f.into_raw_handle();
            // SAFETY: the handle stays open for the life of the process.
            unsafe {
                SetStdHandle(STD_OUTPUT_HANDLE, h as _);
                SetStdHandle(STD_ERROR_HANDLE, h as _);
            }
        }
        ok
    }
    #[cfg(not(windows))]
    {
        let _ = allow_new;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kh2rs-launcher-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A tiny ISO9660 image with one file, SYSTEM.CNF, holding `cnf`.
    fn synthetic_iso(path: &Path, cnf: &str) {
        let mut img = vec![0u8; 2048 * 20];
        let pvd = &mut img[16 * 2048..17 * 2048];
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        // Root directory record at 156: extent 18, one sector.
        pvd[156] = 34;
        pvd[158..162].copy_from_slice(&18u32.to_le_bytes());
        pvd[166..170].copy_from_slice(&2048u32.to_le_bytes());
        pvd[181] = 2;
        pvd[188] = 1;
        let root = &mut img[18 * 2048..19 * 2048];
        let name = b"SYSTEM.CNF;1";
        let n = 33 + name.len() + (name.len() + 1) % 2;
        root[0] = n as u8;
        root[2..6].copy_from_slice(&19u32.to_le_bytes());
        root[10..14].copy_from_slice(&(cnf.len() as u32).to_le_bytes());
        root[32] = name.len() as u8;
        root[33..33 + name.len()].copy_from_slice(name);
        img[19 * 2048..19 * 2048 + cnf.len()].copy_from_slice(cnf.as_bytes());
        std::fs::write(path, img).unwrap();
    }

    const USA_CNF: &str = "BOOT2 = cdrom0:\\SLUS_210.05;1\nVER = 1.00\nVMODE = NTSC\n";

    #[test]
    fn check_image_accepts_usa_and_explains_other_images() {
        let d = scratch("check");
        let usa = d.join("usa.iso");
        synthetic_iso(&usa, USA_CNF);
        assert!(check_image(&usa).is_ok());
        let fm = d.join("fm.iso");
        synthetic_iso(&fm, "BOOT2 = cdrom0:\\SLPM_666.75;1\n");
        let msg = check_image(&fm).unwrap_err();
        assert!(msg.contains("USA") && msg.contains("Final Mix"), "{msg}");
        let junk = d.join("junk.iso");
        std::fs::write(&junk, vec![7u8; 50_000]).unwrap();
        assert!(check_image(&junk).unwrap_err().contains("not a readable disc image"));
        assert!(check_image(&d.join("missing.iso")).is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn discovery_order_env_then_config_then_local_then_dialog() {
        let d = scratch("order");
        let (cfg, usa, other) = (d.join("cfg"), d.join("a.iso"), d.join("b.iso"));
        synthetic_iso(&usa, USA_CNF);
        synthetic_iso(&other, USA_CNF);
        let local = d.join("iso.txt");
        std::fs::write(&local, other.to_string_lossy().as_bytes()).unwrap();
        let mut never = || -> Option<PathBuf> { panic!("the dialog must not open") };
        // Environment wins over everything, unchecked.
        assert_eq!(discover(Some("X:/env.iso".into()), &cfg, &local, false, &mut never).unwrap(), PathBuf::from("X:/env.iso"));
        // No config file yet: local/iso.txt.
        assert_eq!(discover(None, &cfg, &local, false, &mut never).unwrap(), other);
        // The config file beats local/iso.txt.
        Config { iso: Some(usa.clone()), ..Default::default() }.save(&cfg).unwrap();
        assert_eq!(discover(None, &cfg, &local, false, &mut never).unwrap(), usa);
        // A remembered disc that moved falls through to local/iso.txt.
        Config { iso: Some(d.join("gone.iso")), ..Default::default() }.save(&cfg).unwrap();
        assert_eq!(discover(None, &cfg, &local, false, &mut never).unwrap(), other);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn dialog_choice_is_saved_and_cancel_or_bad_image_is_an_error() {
        let d = scratch("dialog");
        let (cfg, usa, junk, missing) = (d.join("cfg"), d.join("a.iso"), d.join("j.iso"), d.join("none.txt"));
        synthetic_iso(&usa, USA_CNF);
        std::fs::write(&junk, b"nope").unwrap();
        let mut cancel = || None;
        let e = discover(None, &cfg, &missing, false, &mut cancel).unwrap_err();
        assert!(e.contains("config.toml"), "{e}");
        let mut bad = || Some(junk.clone());
        assert!(discover(None, &cfg, &missing, false, &mut bad).is_err());
        assert_eq!(Config::load(&cfg).iso, None);
        let mut good = || Some(usa.clone());
        assert_eq!(discover(None, &cfg, &missing, false, &mut good).unwrap(), usa);
        assert_eq!(Config::load(&cfg).iso, Some(usa.clone()));
        // The next start finds it without the dialog; --choose-disc forces the dialog again.
        let mut never = || -> Option<PathBuf> { panic!("the dialog must not open") };
        assert_eq!(discover(None, &cfg, &missing, false, &mut never).unwrap(), usa);
        let mut asked = false;
        let mut again = || {
            asked = true;
            Some(usa.clone())
        };
        discover(None, &cfg, &missing, true, &mut again).unwrap();
        assert!(asked);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn help_mentions_every_flag_and_args_parse() {
        for flag in ["--help", "--choose-disc", "--windowed", "--fullscreen", "--console", "KH2_ISO", "KH2_CONFIG_DIR", "F11"] {
            assert!(HELP.contains(flag), "{flag}");
        }
        let o = parse_args(["--fullscreen", "tt02", "--choose-disc", "tt00", "--unknown"].map(String::from));
        assert_eq!(o, Options { area: Some("tt02".into()), choose_disc: true, fullscreen: true, ..Default::default() });
        assert_eq!(parse_args(Vec::new()), Options::default());
    }

    #[test]
    fn log_rotation_keeps_the_last_two() {
        let d = scratch("log");
        for run in ["one", "two", "three"] {
            let p = rotate_log(&d).unwrap();
            std::fs::write(&p, run).unwrap();
        }
        assert_eq!(std::fs::read_to_string(d.join(LOG_FILE)).unwrap(), "three");
        assert_eq!(std::fs::read_to_string(d.join("kh2-rs.1.log")).unwrap(), "two");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
