//! Analog key depth from Hall-effect keyboards through the Wooting Analog SDK.
//!
//! The SDK is not bundled. When the user has installed it (it ships
//! `wooting_analog_wrapper.dll`, MPL-2.0), ZeroIn loads it at runtime and reads
//! key depth for W, A, S and D. Plugins inside the SDK add support for other
//! analog keyboards. If the DLL is missing, everything else keeps working and
//! keyboards are read through Raw Input like any other keyboard.
//!
//! On Windows the DLL is loaded only from the SDK's install folder, the
//! system folder or ZeroIn's own folder, never through `PATH`, so a DLL
//! dropped into a folder on `PATH` cannot be loaded into ZeroIn.

use libloading::{Library, Symbol};

/// HID usage codes for W, A, S, D.
pub const WASD_HID: [u16; 4] = [0x1A, 0x04, 0x16, 0x07];

pub struct WootingAnalog {
    lib: Library,
    pub devices: i32,
}

type InitFn = unsafe extern "C" fn() -> i32;
type ReadFn = unsafe extern "C" fn(u16) -> f32;
type UninitFn = unsafe extern "C" fn() -> i32;

impl WootingAnalog {
    /// Try to load and initialise the SDK. Returns a reason string on failure.
    pub fn load() -> Result<Self, String> {
        // SAFETY: loading a user-installed SDK library from fixed locations;
        // the symbols below match the SDK's C header.
        unsafe {
            let lib = open_sdk().ok_or_else(|| "Wooting Analog SDK not installed".to_string())?;
            let devices = {
                let init: Symbol<InitFn> = lib.get(b"wooting_analog_initialise\0").map_err(|e| e.to_string())?;
                init()
            };
            if devices < 0 {
                return Err(format!("SDK initialisation failed (code {devices})"));
            }
            Ok(WootingAnalog { lib, devices })
        }
    }

    /// Key depth 0.0–1.0 for a HID usage code.
    pub fn read(&self, hid: u16) -> f32 {
        // SAFETY: symbol signature matches the SDK header.
        unsafe {
            match self.lib.get::<ReadFn>(b"wooting_analog_read_analog\0") {
                Ok(f) => {
                    let v = f(hid);
                    if v.is_finite() {
                        v.clamp(0.0, 1.0)
                    } else {
                        0.0
                    }
                }
                Err(_) => 0.0,
            }
        }
    }
}

/// Candidate install folders of the SDK, most specific first.
#[cfg_attr(not(windows), allow(dead_code))]
fn sdk_dirs(program_files: &[Option<std::ffi::OsString>]) -> Vec<std::path::PathBuf> {
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    for pf in program_files.iter().flatten() {
        let p = std::path::Path::new(pf);
        if p.is_absolute() {
            let d = p.join("WootingAnalogSDK");
            if !out.contains(&d) {
                out.push(d);
            }
        }
    }
    out
}

#[cfg(windows)]
unsafe fn open_sdk() -> Option<Library> {
    use libloading::os::windows::{
        Library as WinLib, LOAD_LIBRARY_SEARCH_APPLICATION_DIR, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
        LOAD_LIBRARY_SEARCH_SYSTEM32,
    };
    const NAME: &str = "wooting_analog_wrapper.dll";
    let pf = [std::env::var_os("ProgramW6432"), std::env::var_os("ProgramFiles")];
    for dir in sdk_dirs(&pf) {
        let full = dir.join(NAME);
        if full.is_file() {
            // The SDK's own dependencies resolve from its folder, then system folders.
            if let Ok(l) = unsafe { WinLib::load_with_flags(&full, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) } {
                return Some(l.into());
            }
        }
    }
    // Bare name: application folder and System32 only (no PATH, no CWD).
    unsafe { WinLib::load_with_flags(NAME, LOAD_LIBRARY_SEARCH_APPLICATION_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32) }.ok().map(Into::into)
}

#[cfg(not(windows))]
unsafe fn open_sdk() -> Option<Library> {
    unsafe { Library::new("libwooting_analog_wrapper.so") }.ok()
}

impl Drop for WootingAnalog {
    fn drop(&mut self) {
        // SAFETY: symbol signature matches the SDK header.
        unsafe {
            if let Ok(f) = self.lib.get::<UninitFn>(b"wooting_analog_uninitialise\0") {
                f();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sdk_dirs_ignore_relative_and_duplicate_entries() {
        let pf = [Some("C:\\Program Files".into()), Some("C:\\Program Files".into()), Some("relative".into()), None];
        let d = super::sdk_dirs(&pf);
        // Absolute-path semantics differ off Windows; only check filtering.
        assert!(d.len() <= 1);
        assert!(d.iter().all(|p| p.ends_with("WootingAnalogSDK")));
        let unix = [Some("/opt/pf".into())];
        if cfg!(unix) {
            assert_eq!(super::sdk_dirs(&unix), vec![std::path::PathBuf::from("/opt/pf/WootingAnalogSDK")]);
        }
    }
}
