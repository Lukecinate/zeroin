//! Where ZeroIn keeps its files.

use sha2::{Digest, Sha256};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Official bundled model (Qwen3 1.7B, Q4_K_M), the same hash release.yml
/// checks before packaging. Only a regular file with this digest is loaded.
#[cfg_attr(not(feature = "llm"), allow(dead_code))]
pub const OFFICIAL_MODEL_SHA256: &str = "b139949c5bd74937ad8ed8c8cf3d9ffb1e99c866c823204dc42c0d91fa181897";

/// File name of the official model as packaged by the installer.
pub const OFFICIAL_MODEL_FILE: &str = "Qwen3-1.7B-Q4_K_M.gguf";

/// Inclusive size window for a candidate `.gguf` (bytes). The official file
/// is about 1.1 GB; this range skips obvious plants without hashing them.
pub const MODEL_MIN_BYTES: u64 = 500 * 1024 * 1024;
pub const MODEL_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;

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
    // Never a path relative to the working directory.
    std::env::temp_dir().join("zeroin")
}

pub fn history_file() -> PathBuf {
    data_dir().join("history.json")
}

pub fn settings_file() -> PathBuf {
    data_dir().join("settings.json")
}

/// The unfinished test, saved after every block so it can be resumed.
pub fn saved_test_file() -> PathBuf {
    data_dir().join("unfinished-test.json")
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

/// Whether `len` is in the expected range for the official model file.
pub fn model_size_ok(len: u64) -> bool {
    (MODEL_MIN_BYTES..=MODEL_MAX_BYTES).contains(&len)
}

/// True when `path` is a regular file, not a symbolic link or directory.
pub fn is_regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file() && !m.file_type().is_symlink())
}

/// SHA-256 (lowercase hex) of a regular file, streamed so a large model is
/// not held in memory all at once.
#[cfg_attr(not(feature = "llm"), allow(dead_code))]
pub fn sha256_hex_file(path: &Path) -> io::Result<String> {
    if !is_regular_file(path) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a regular file"));
    }
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Accept only the official bundled model: regular file, expected size,
/// SHA-256 matching [`OFFICIAL_MODEL_SHA256`].
#[cfg_attr(not(feature = "llm"), allow(dead_code))]
pub fn verify_official_model(path: &Path) -> Result<(), String> {
    if !is_regular_file(path) {
        return Err("model path is not a regular file".into());
    }
    let len = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if !model_size_ok(len) {
        return Err("model file size is outside the expected range".into());
    }
    let hex = sha256_hex_file(path).map_err(|e| e.to_string())?;
    if !hex.eq_ignore_ascii_case(OFFICIAL_MODEL_SHA256) {
        return Err("model SHA-256 does not match the official Qwen3 1.7B Q4_K_M file".into());
    }
    Ok(())
}

/// Plausible `.gguf` files in [`model_dirs`]: regular files of the expected
/// size, preferring names containing "qwen3" and the official file name.
/// The SHA-256 check is done later, off the UI thread, before the file is loaded.
pub fn find_model_candidates() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for dir in model_dirs() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if !p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("gguf")) {
                continue;
            }
            if !is_regular_file(&p) {
                continue;
            }
            let Ok(len) = std::fs::metadata(&p).map(|m| m.len()) else { continue };
            if model_size_ok(len) {
                found.push(p);
            }
        }
    }
    found.sort_by_key(|p| {
        let name = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        (name != OFFICIAL_MODEL_FILE.to_ascii_lowercase(), !name.contains("qwen3"))
    });
    found
}

/// First candidate in [`find_model_candidates`]. The hash is confirmed by
/// the model worker before llama.cpp opens the file.
pub fn find_model() -> Option<PathBuf> {
    find_model_candidates().into_iter().next()
}

/// Windows pointer settings saved before ZeroIn changed them.
pub fn mouse_backup_file() -> PathBuf {
    data_dir().join("mouse-backup.json")
}

/// Pointer settings to put back after a test that used temporary neutral values.
pub fn mouse_temp_file() -> PathBuf {
    data_dir().join("mouse-temp.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "zeroin-model-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn official_hash_constant_is_lowercase_hex() {
        assert_eq!(OFFICIAL_MODEL_SHA256.len(), 64);
        assert!(OFFICIAL_MODEL_SHA256.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')));
        assert!(model_size_ok(1_100_000_000));
        assert!(!model_size_ok(MODEL_MIN_BYTES - 1));
        assert!(!model_size_ok(MODEL_MAX_BYTES + 1));
    }

    #[test]
    fn sha256_matches_empty_and_abc_vectors() {
        let d = tmp();
        let empty = d.join("empty.gguf");
        std::fs::write(&empty, b"").unwrap();
        assert_eq!(sha256_hex_file(&empty).unwrap(), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        let abc = d.join("abc.gguf");
        std::fs::write(&abc, b"abc").unwrap();
        assert_eq!(sha256_hex_file(&abc).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn verify_rejects_wrong_hash_symlink_and_size() {
        let d = tmp();
        let small = d.join("Qwen3-1.7B-Q4_K_M.gguf");
        std::fs::write(&small, b"not-a-model").unwrap();
        let err = verify_official_model(&small).unwrap_err();
        assert!(err.contains("size"), "{err}");

        #[cfg(unix)]
        {
            let link = d.join("linked.gguf");
            std::os::unix::fs::symlink(&small, &link).unwrap();
            assert!(!is_regular_file(&link));
            assert!(sha256_hex_file(&link).is_err());
            assert!(verify_official_model(&link).is_err());
        }
    }
}
