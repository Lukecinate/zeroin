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

    pub fn set_accept(&self, _on: bool) {}
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

/// Session-only write; off Windows this only touches the in-memory dev mock.
pub fn set_mouse_params_session(p: zeroin_core::winmouse::MouseParams) -> Result<(), String> {
    set_mouse_params(p)
}

/// No sign-out notice off Windows.
pub fn watch_session_end(_on_end: fn()) {}

pub fn sign_in_session() -> Option<zeroin_core::winmouse::SignInSession> {
    None
}

pub fn confine_cursor(_hwnd: isize) {}

pub fn release_cursor() {}

/// Not resolved off Windows; dates are shown in UTC there.
pub fn local_offset_secs(_unix: u64) -> Option<i64> {
    None
}

// ---------------------------------------------------------------------------
// Polling check (development mock only)

/// Off Windows there is no raw input. For UI development,
/// `ZEROIN_DEV_POLL=<hz>[:<jitter>]` (for example `8000:0.02` or `2500:0.45`)
/// generates a synthetic report stream; the screen labels it as mock data.
pub struct PollCheck {
    started_ns: u64,
    hz: f64,
    jitter: f64,
}

fn dev_poll() -> Option<(f64, f64)> {
    let v = std::env::var("ZEROIN_DEV_POLL").ok()?;
    let (h, j) = v.split_once(':').unwrap_or((v.as_str(), "0.03"));
    let hz: f64 = h.trim().parse().ok()?;
    let jitter: f64 = j.trim().parse().ok()?;
    (hz > 0.0 && hz <= 16_000.0).then_some((hz, jitter.clamp(0.0, 0.95)))
}

impl PollCheck {
    pub fn start() -> Result<Self, String> {
        match dev_poll() {
            Some((hz, jitter)) => Ok(PollCheck { started_ns: super::now_ns(), hz, jitter }),
            None => Err("Not measurable here: the polling check needs Windows Raw Input.".into()),
        }
    }

    pub fn count(&self) -> u64 {
        (super::now_ns().saturating_sub(self.started_ns) as f64 / 1e9 * self.hz) as u64
    }

    pub fn finish(self) -> Result<Vec<super::CapReport>, String> {
        let secs = (super::now_ns().saturating_sub(self.started_ns) as f64 / 1e9).min(30.0);
        let n = (secs * self.hz) as u64;
        let t = 1e9 / self.hz;
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut out: Vec<super::CapReport> = (0..n)
            .map(|i| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                let u = (x % 20001) as f64 / 10000.0 - 1.0;
                let ts = (i as f64 * t + self.jitter * t * u).max(0.0) as u64 + self.started_ns;
                super::CapReport { t_ns: ts, device: 1, dx: 6, dy: 3, batch: 1 }
            })
            .collect();
        out.sort_by_key(|r| r.t_ns);
        Ok(out)
    }
}

pub fn device_identity(_device: u64) -> Option<super::DeviceIdentity> {
    dev_poll().map(|_| super::DeviceIdentity {
        path: "mock".into(),
        product: Some("Development mock (no device)".into()),
        manufacturer: None,
        mock: true,
    })
}

pub(crate) fn open_url_impl(url: &str) -> bool {
    std::process::Command::new("xdg-open").arg(url).spawn().is_ok()
}

/// Development builds off Windows do not limit instances.
pub fn claim_single_instance() -> bool {
    true
}
