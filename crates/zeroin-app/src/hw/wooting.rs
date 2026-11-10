//! Analog key depth from Hall-effect keyboards through the Wooting Analog SDK.
//!
//! The SDK is not bundled. When the user has installed it (it ships
//! `wooting_analog_wrapper.dll`, MPL-2.0), ZeroIn loads it at runtime and reads
//! key depth for W, A, S and D. Plugins inside the SDK add support for other
//! analog keyboards. If the DLL is missing, everything else keeps working and
//! keyboards are read through Raw Input like any other keyboard.

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
        let name = if cfg!(windows) { "wooting_analog_wrapper.dll" } else { "libwooting_analog_wrapper.so" };
        // SAFETY: loading a user-installed SDK library by its documented name;
        // the symbols below match the SDK's C header.
        unsafe {
            let lib = Library::new(name).map_err(|_| "Wooting Analog SDK not installed".to_string())?;
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
