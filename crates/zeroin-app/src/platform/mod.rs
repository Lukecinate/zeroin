//! Operating-system integration. Real implementations live in `windows.rs`;
//! `fallback.rs` lets the app build and run on other platforms for
//! development (input then comes from the window's own mouse events).

use std::sync::OnceLock;
use std::time::Instant;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(not(windows))]
mod fallback;
#[cfg(not(windows))]
pub use self::fallback::*;

static EPOCH: OnceLock<Instant> = OnceLock::new();

/// Microseconds since app start, on the same monotonic clock as input events.
pub fn now_us() -> u64 {
    EPOCH.get_or_init(Instant::now).elapsed().as_micros() as u64
}

/// Initialise the clock (call once at start-up).
pub fn init_clock() {
    let _ = EPOCH.get_or_init(Instant::now);
}

/// One raw input report.
#[derive(Clone, Copy, Debug)]
pub struct InputEvent {
    /// Timestamp from [`now_us`].
    pub t_us: u64,
    /// Opaque device handle (distinguishes multiple mice).
    pub device: u64,
    /// Payload.
    pub kind: InputKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    /// Relative motion in counts.
    Motion { dx: i32, dy: i32 },
    /// Left button transition.
    LeftButton { down: bool },
    /// Keyboard key transition (Windows virtual-key code).
    Key { vkey: u16, down: bool },
}

/// Windows mouse settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseSettings {
    /// Pointer speed slider (1–20).
    pub pointer_speed: u32,
    /// "Enhance pointer precision".
    pub enhance_pointer_precision: bool,
}

/// Primary display refresh information.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayInfo {
    /// Refresh rate Windows is using now.
    pub current_hz: u32,
    /// Highest refresh rate offered at the current resolution.
    pub max_hz: u32,
    /// Resolution.
    pub width: u32,
    pub height: u32,
}
