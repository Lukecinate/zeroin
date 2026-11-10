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
    mouse_params().map(|p| MouseSettings { pointer_speed: p.speed, enhance_pointer_precision: p.enhance_pointer_precision() })
}

pub fn display_info() -> Option<DisplayInfo> {
    None
}

pub fn open_folder(path: &std::path::Path) {
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

/// Development aid: `ZEROIN_DEV_MOUSE=<acceleration>,<speed>` (for example
/// `1,14`) simulates Windows pointer settings in memory so the Apply / Restore
/// flow can be exercised off Windows. Nothing outside the process changes.
static DEV_MOUSE: std::sync::Mutex<Option<zeroin_core::winmouse::MouseParams>> = std::sync::Mutex::new(None);
static DEV_MOUSE_INIT: std::sync::Once = std::sync::Once::new();

fn dev_mouse() -> std::sync::MutexGuard<'static, Option<zeroin_core::winmouse::MouseParams>> {
    DEV_MOUSE_INIT.call_once(|| {
        let parsed = std::env::var("ZEROIN_DEV_MOUSE").ok().and_then(|v| {
            let (a, s) = v.split_once(',')?;
            let acceleration: i32 = a.trim().parse().ok()?;
            let speed: u32 = s.trim().parse().ok()?;
            let (threshold1, threshold2) = if acceleration != 0 { (6, 10) } else { (0, 0) };
            Some(zeroin_core::winmouse::MouseParams { threshold1, threshold2, acceleration, speed }.sanitized())
        });
        *DEV_MOUSE.lock().unwrap_or_else(|e| e.into_inner()) = parsed;
    });
    DEV_MOUSE.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn mouse_params() -> Option<zeroin_core::winmouse::MouseParams> {
    *dev_mouse()
}

pub fn set_mouse_params(p: zeroin_core::winmouse::MouseParams) -> Result<(), String> {
    let mut m = dev_mouse();
    match m.as_mut() {
        Some(cur) => {
            *cur = p.sanitized();
            Ok(())
        }
        None => Err("Changing pointer settings is only available on Windows.".into()),
    }
}

pub fn confine_cursor(_hwnd: isize) {}

pub fn release_cursor() {}

/// Not resolved off Windows; dates are shown in UTC there.
pub fn local_offset_secs(_unix: u64) -> Option<i64> {
    None
}
