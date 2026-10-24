//! The player's settings file: `config.toml` in the config folder (`%APPDATA%\kh2-rs`, or the folder named by
//! `KH2_CONFIG_DIR`). A tiny `key = value` subset of TOML is enough: the disc path, the window size and fullscreen.

use std::path::{Path, PathBuf};

/// Environment variable that moves the config folder (tests and scripts use it; players do not need it).
pub const CONFIG_DIR_ENV: &str = "KH2_CONFIG_DIR";
/// File name of the settings file inside the config folder.
pub const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Config {
    /// Path of the player's own USA disc image.
    pub iso: Option<PathBuf>,
    /// Window size in logical pixels when not fullscreen.
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fullscreen: Option<bool>,
}

/// The config folder: `KH2_CONFIG_DIR`, else `%APPDATA%\kh2-rs`, else `$HOME/.config/kh2-rs`.
pub fn config_dir() -> PathBuf {
    if let Some(d) = std::env::var_os(CONFIG_DIR_ENV).filter(|d| !d.is_empty()) {
        return d.into();
    }
    if let Some(d) = std::env::var_os("APPDATA").filter(|d| !d.is_empty()) {
        return PathBuf::from(d).join("kh2-rs");
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(".config").join("kh2-rs")
}

impl Config {
    pub fn parse(text: &str) -> Config {
        let mut c = Config::default();
        for line in text.lines() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            match k {
                "iso" => c.iso = unquote(v).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty()),
                "width" => c.width = v.parse().ok(),
                "height" => c.height = v.parse().ok(),
                "fullscreen" => c.fullscreen = v.parse().ok(),
                _ => {}
            }
        }
        c
    }

    pub fn to_text(&self) -> String {
        let mut s = String::from("# kh2-rs settings. Safe to edit; delete the iso line to be asked for the disc again.\n");
        if let Some(p) = &self.iso {
            let escaped = p.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
            s.push_str(&format!("iso = \"{escaped}\"\n"));
        }
        if let Some(w) = self.width {
            s.push_str(&format!("width = {w}\n"));
        }
        if let Some(h) = self.height {
            s.push_str(&format!("height = {h}\n"));
        }
        if let Some(f) = self.fullscreen {
            s.push_str(&format!("fullscreen = {f}\n"));
        }
        s
    }

    pub fn load(dir: &Path) -> Config {
        std::fs::read_to_string(dir.join(CONFIG_FILE)).map(|t| Config::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(CONFIG_FILE), self.to_text())
    }
}

/// A TOML basic string (`"C:\x"`) or a bare/literal value.
fn unquote(v: &str) -> Option<String> {
    if let Some(inner) = v.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                out.push(chars.next()?);
            } else {
                out.push(c);
            }
        }
        Some(out)
    } else if let Some(inner) = v.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        Some(inner.to_string())
    } else {
        Some(v.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_through_text_and_disk() {
        let c = Config { iso: Some(PathBuf::from(r#"C:\Games\KH "II"\disc.iso"#)), width: Some(1280), height: Some(720), fullscreen: Some(true) };
        assert_eq!(Config::parse(&c.to_text()), c);
        let dir = std::env::temp_dir().join(format!("kh2rs-config-test-{}", std::process::id()));
        c.save(&dir).unwrap();
        assert_eq!(Config::load(&dir), c);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parse_ignores_comments_unknown_keys_and_bad_numbers() {
        let c = Config::parse("# x\n[section]\nunknown = 3\nwidth = abc\nheight = 600\niso = 'D:\\a.iso'\n");
        assert_eq!(c, Config { iso: Some(PathBuf::from(r"D:\a.iso")), width: None, height: Some(600), fullscreen: None });
        assert_eq!(Config::load(Path::new("definitely/not/here")), Config::default());
    }
}
