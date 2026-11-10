//! Non-Windows stand-ins used for development builds only.

use super::{DisplayInfo, InputEvent, MouseSettings};

pub struct RawInput;

impl RawInput {
    pub fn start() -> Self {
        RawInput
    }
    /// Raw input is not available; the app uses window events instead.
    pub fn is_native(&self) -> bool {
        false
    }
    pub fn drain(&self, _out: &mut Vec<InputEvent>) {}
}

pub fn mouse_settings() -> Option<MouseSettings> {
    None
}

pub fn display_info() -> Option<DisplayInfo> {
    None
}

pub fn open_folder(path: &std::path::Path) {
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

pub fn mouse_params() -> Option<zeroin_core::winmouse::MouseParams> {
    None
}

pub fn set_mouse_params(_p: zeroin_core::winmouse::MouseParams) -> Result<(), String> {
    Err("Changing pointer settings is only available on Windows.".into())
}

pub fn confine_cursor(_hwnd: isize) {}

pub fn release_cursor() {}

/// Not resolved off Windows; dates are shown in UTC there.
pub fn local_offset_secs(_unix: u64) -> Option<i64> {
    None
}
