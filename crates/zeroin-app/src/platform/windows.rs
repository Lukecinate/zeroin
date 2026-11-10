//! Win32: Raw Input on a dedicated thread, mouse settings and display modes.

use super::{now_us, DisplayInfo, InputEvent, InputKind, MouseSettings};
use std::sync::mpsc::{channel, Receiver};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::{
    GetRawInputData, RegisterRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RIDEV_INPUTSINK, RID_INPUT,
    RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, SystemParametersInfoW, HWND_MESSAGE, MSG, SPI_GETMOUSE,
    SPI_GETMOUSESPEED, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WINDOW_EX_STYLE, WINDOW_STYLE, WM_INPUT, WNDCLASSW,
};

const RI_MOUSE_LEFT_BUTTON_DOWN: u16 = 0x0001;
const RI_MOUSE_LEFT_BUTTON_UP: u16 = 0x0002;
const MOUSE_MOVE_ABSOLUTE: u16 = 0x0001;
const RI_KEY_BREAK: u16 = 0x0001;

pub struct RawInput {
    rx: Option<Receiver<InputEvent>>,
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

impl RawInput {
    /// Start the raw-input thread. Must be called after the main window
    /// exists so this registration (one target per device class per process)
    /// is the one that stays active.
    pub fn start() -> Self {
        let (tx, rx) = channel::<InputEvent>();
        let (ready_tx, ready_rx) = channel::<bool>();
        let spawned = std::thread::Builder::new().name("zeroin-raw-input".into()).spawn(move || {
            // SAFETY: plain Win32 calls on this thread's own message-only window.
            unsafe {
                let hinstance = match GetModuleHandleW(None) {
                    Ok(h) => h,
                    Err(_) => {
                        let _ = ready_tx.send(false);
                        return;
                    }
                };
                let class = w!("ZeroInRawInput");
                let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: hinstance.into(), lpszClassName: class, ..Default::default() };
                RegisterClassW(&wc);
                let hwnd = match CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    w!("ZeroIn raw input"),
                    WINDOW_STYLE(0),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    Some(hinstance.into()),
                    None,
                ) {
                    Ok(h) => h,
                    Err(_) => {
                        let _ = ready_tx.send(false);
                        return;
                    }
                };
                let devices = [
                    RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x02, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd },
                    RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x06, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd },
                ];
                if RegisterRawInputDevices(&devices, std::mem::size_of::<RAWINPUTDEVICE>() as u32).is_err() {
                    let _ = ready_tx.send(false);
                    return;
                }
                let _ = ready_tx.send(true);
                let mut msg = MSG::default();
                let mut buf: Vec<u8> = vec![0; 256];
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    if msg.message == WM_INPUT {
                        let t_us = now_us();
                        let handle = HRAWINPUT(msg.lParam.0 as _);
                        let mut size = 0u32;
                        let hdr = std::mem::size_of::<RAWINPUTHEADER>() as u32;
                        GetRawInputData(handle, RID_INPUT, None, &mut size, hdr);
                        if size as usize > buf.len() {
                            buf.resize(size as usize, 0);
                        }
                        let got = GetRawInputData(handle, RID_INPUT, Some(buf.as_mut_ptr() as *mut _), &mut size, hdr);
                        if got != u32::MAX && got as usize >= std::mem::size_of::<RAWINPUTHEADER>() {
                            let raw = &*(buf.as_ptr() as *const RAWINPUT);
                            let device = raw.header.hDevice.0 as usize as u64;
                            if raw.header.dwType == RIM_TYPEMOUSE.0 {
                                let m = raw.data.mouse;
                                let flags = m.usFlags.0;
                                let buttons = m.Anonymous.Anonymous.usButtonFlags;
                                if flags & MOUSE_MOVE_ABSOLUTE == 0 && (m.lLastX != 0 || m.lLastY != 0) {
                                    let _ = tx.send(InputEvent { t_us, device, kind: InputKind::Motion { dx: m.lLastX, dy: m.lLastY } });
                                }
                                if buttons & RI_MOUSE_LEFT_BUTTON_DOWN != 0 {
                                    let _ = tx.send(InputEvent { t_us, device, kind: InputKind::LeftButton { down: true } });
                                }
                                if buttons & RI_MOUSE_LEFT_BUTTON_UP != 0 {
                                    let _ = tx.send(InputEvent { t_us, device, kind: InputKind::LeftButton { down: false } });
                                }
                            } else if raw.header.dwType == RIM_TYPEKEYBOARD.0 {
                                let k = raw.data.keyboard;
                                let down = k.Flags & RI_KEY_BREAK == 0;
                                let _ = tx.send(InputEvent { t_us, device, kind: InputKind::Key { vkey: k.VKey, down } });
                            }
                        }
                    }
                    DispatchMessageW(&msg);
                }
            }
        });
        let ok = spawned.is_ok() && ready_rx.recv().unwrap_or(false);
        RawInput { rx: ok.then_some(rx) }
    }

    /// True when raw input is registered and delivering events.
    pub fn is_native(&self) -> bool {
        self.rx.is_some()
    }

    /// Move all pending events into `out`.
    pub fn drain(&self, out: &mut Vec<InputEvent>) {
        if let Some(rx) = &self.rx {
            out.extend(rx.try_iter());
        }
    }
}

/// Read pointer speed and "Enhance pointer precision".
pub fn mouse_settings() -> Option<MouseSettings> {
    let mut speed: i32 = 0;
    let mut accel: [i32; 3] = [0; 3];
    // SAFETY: both buffers are the sizes these SPI actions document.
    unsafe {
        SystemParametersInfoW(SPI_GETMOUSESPEED, 0, Some(&mut speed as *mut i32 as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).ok()?;
        SystemParametersInfoW(SPI_GETMOUSE, 0, Some(accel.as_mut_ptr() as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).ok()?;
    }
    Some(MouseSettings { pointer_speed: speed.clamp(1, 20) as u32, enhance_pointer_precision: accel[2] != 0 })
}

/// Current and maximum refresh rate of the primary display.
pub fn display_info() -> Option<DisplayInfo> {
    let mut cur = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
    // SAFETY: DEVMODEW is initialised with its size as the API requires.
    unsafe {
        if !EnumDisplaySettingsW(PCWSTR::null(), ENUM_CURRENT_SETTINGS, &mut cur).as_bool() {
            return None;
        }
        let mut max_hz = cur.dmDisplayFrequency;
        let mut i = 0u32;
        loop {
            let mut m = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
            if !EnumDisplaySettingsW(PCWSTR::null(), ENUM_DISPLAY_SETTINGS_MODE(i), &mut m).as_bool() {
                break;
            }
            if m.dmPelsWidth == cur.dmPelsWidth && m.dmPelsHeight == cur.dmPelsHeight {
                max_hz = max_hz.max(m.dmDisplayFrequency);
            }
            i += 1;
            if i > 4096 {
                break;
            }
        }
        Some(DisplayInfo { current_hz: cur.dmDisplayFrequency, max_hz, width: cur.dmPelsWidth, height: cur.dmPelsHeight })
    }
}

/// Open a folder in Explorer.
pub fn open_folder(path: &std::path::Path) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

// ---------------------------------------------------------------------------
// Pointer settings (write path)

/// Read the raw values behind the pointer settings.
pub fn mouse_params() -> Option<zeroin_core::winmouse::MouseParams> {
    let mut speed: i32 = 0;
    let mut accel: [i32; 3] = [0; 3];
    // SAFETY: both buffers are the sizes these SPI actions document.
    unsafe {
        SystemParametersInfoW(SPI_GETMOUSESPEED, 0, Some(&mut speed as *mut i32 as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).ok()?;
        SystemParametersInfoW(SPI_GETMOUSE, 0, Some(accel.as_mut_ptr() as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).ok()?;
    }
    Some(zeroin_core::winmouse::MouseParams {
        threshold1: accel[0],
        threshold2: accel[1],
        acceleration: accel[2],
        speed: speed.clamp(1, 20) as u32,
    })
}

/// Write pointer speed and acceleration for the current user (no admin
/// rights needed), persist them, and notify other apps.
pub fn set_mouse_params(p: zeroin_core::winmouse::MouseParams) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETMOUSE, SPI_SETMOUSESPEED};
    let p = p.sanitized();
    let mut accel: [i32; 3] = [p.threshold1, p.threshold2, p.acceleration];
    let flags = SPIF_UPDATEINIFILE | SPIF_SENDCHANGE;
    // SAFETY: SPI_SETMOUSE takes a pointer to three INTs; SPI_SETMOUSESPEED
    // takes the speed itself in pvParam (documented, not a pointer).
    unsafe {
        SystemParametersInfoW(SPI_SETMOUSE, 0, Some(accel.as_mut_ptr() as *mut _), flags).map_err(|e| format!("Acceleration: {e}"))?;
        SystemParametersInfoW(SPI_SETMOUSESPEED, 0, Some(p.speed as usize as *mut _), flags).map_err(|e| format!("Pointer speed: {e}"))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Cursor confinement during drills

/// Confine the cursor to the window's client area and park it in the middle,
/// so clicks cannot land on another window or monitor. Raw Input is
/// unaffected: `SetCursorPos` produces no raw mouse motion.
pub fn confine_cursor(hwnd: isize) {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::{ClipCursor, GetClientRect, SetCursorPos};
    let hwnd = HWND(hwnd as *mut _);
    let mut rc = RECT::default();
    // SAFETY: plain Win32 calls on our own window handle with valid out-pointers.
    unsafe {
        if GetClientRect(hwnd, &mut rc).is_err() {
            return;
        }
        let mut tl = POINT { x: rc.left, y: rc.top };
        let mut br = POINT { x: rc.right, y: rc.bottom };
        if !ClientToScreen(hwnd, &mut tl).as_bool() || !ClientToScreen(hwnd, &mut br).as_bool() {
            return;
        }
        let screen = RECT { left: tl.x, top: tl.y, right: br.x, bottom: br.y };
        let _ = ClipCursor(Some(&screen));
        let _ = SetCursorPos((tl.x + br.x) / 2, (tl.y + br.y) / 2);
    }
}

/// Release any cursor confinement. Safe to call at any time, including from
/// a panic hook.
pub fn release_cursor() {
    use windows::Win32::UI::WindowsAndMessaging::ClipCursor;
    // SAFETY: a null rectangle removes the clip; no other state is touched.
    unsafe {
        let _ = ClipCursor(None);
    }
}

// ---------------------------------------------------------------------------
// Local time

/// Offset of local time from UTC, in seconds, at the given Unix time
/// (daylight saving time applied for that date).
pub fn local_offset_secs(unix: u64) -> Option<i64> {
    use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTimeEx};
    let ticks = unix.checked_mul(10_000_000)? + 116_444_736_000_000_000;
    let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    let mut lft = FILETIME::default();
    // SAFETY: all pointers refer to initialised locals.
    unsafe {
        FileTimeToSystemTime(&ft, &mut utc).ok()?;
        SystemTimeToTzSpecificLocalTimeEx(None, &utc, &mut local).ok()?;
        SystemTimeToFileTime(&local, &mut lft).ok()?;
    }
    let lticks = ((lft.dwHighDateTime as u64) << 32) | lft.dwLowDateTime as u64;
    Some((lticks as i64 - ticks as i64) / 10_000_000)
}
