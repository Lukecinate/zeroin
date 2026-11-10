//! Where ZeroIn keeps its files.

use std::path::PathBuf;

/// `%APPDATA%\ZeroIn` on Windows; `$XDG_DATA_HOME/zeroin` or `~/.local/share/zeroin` elsewhere.
pub fn data_dir() -> PathBuf {
    if cfg!(windows) {
        if let Some(a) = std::env::var_os("APPDATA") {
            return PathBuf::from(a).join("ZeroIn");
        }
    }
    if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(x).join("zeroin");
    }
    if let Some(h) = std::env::var_os("HOME") {
        return PathBuf::from(h).join(".local/share/zeroin");
    }
    PathBuf::from(".zeroin")
}

pub fn history_file() -> PathBuf {
    data_dir().join("history.json")
}

pub fn settings_file() -> PathBuf {
    data_dir().join("settings.json")
}

/// Folders searched for a `.gguf` model, in order.
pub fn model_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            v.push(d.join("models"));
        }
    }
    v.push(data_dir().join("models"));
    v
}

/// First `.gguf` file found in [`model_dirs`], preferring names containing "qwen3".
pub fn find_model() -> Option<PathBuf> {
    for dir in model_dirs() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut found: Vec<PathBuf> =
            rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("gguf"))).collect();
        found.sort_by_key(|p| !p.to_string_lossy().to_ascii_lowercase().contains("qwen3"));
        if let Some(p) = found.into_iter().next() {
            return Some(p);
        }
    }
    None
}

/// Windows pointer settings saved before ZeroIn changed them.
pub fn mouse_backup_file() -> PathBuf {
    data_dir().join("mouse-backup.json")
}
