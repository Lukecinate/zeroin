//! Win32: Raw Input on a dedicated thread, mouse settings and display modes.

use super::queue::{event_queue, EventReceiver, DEFAULT_CAPACITY};
use super::{now_us, DisplayInfo, InputEvent, InputKind, MouseSettings};
use std::sync::mpsc::channel;
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

/// Message-only window of the raw-input thread, so the polling check can hand
/// mouse input back to it when it finishes.
pub(crate) static MAIN_RAW_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

pub struct RawInput {
    rx: Option<EventReceiver>,
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

impl RawInput {
    /// Start the raw-input thread. Must be called after the main window
    /// exists so this registration (one target per device class per process)
    /// is the one that stays active.
    pub fn start() -> Self {
        // Bounded: nothing can pile up while the window gets no frames.
        let (tx, rx) = event_queue(DEFAULT_CAPACITY);
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
                MAIN_RAW_HWND.store(hwnd.0 as isize, std::sync::atomic::Ordering::SeqCst);
                let _ = ready_tx.send(true);
                let mut msg = MSG::default();
                // u64 storage keeps the buffer 8-byte aligned; the struct is
                // still copied out with read_unaligned, as in poll_windows.rs.
                let raw_size = std::mem::size_of::<RAWINPUT>();
                let mut buf: Vec<u64> = vec![0; raw_size.div_ceil(8).max(32)];
                // GetMessageW returns -1 on error: only > 0 means a message.
                while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                    if msg.message == WM_INPUT {
                        let t_us = now_us();
                        let handle = HRAWINPUT(msg.lParam.0 as _);
                        let mut size = 0u32;
                        let hdr = std::mem::size_of::<RAWINPUTHEADER>() as u32;
                        if GetRawInputData(handle, RID_INPUT, None, &mut size, hdr) != 0 || size == 0 || size > 64 * 1024 {
                            DispatchMessageW(&msg);
                            continue;
                        }
                        let need = (size as usize).max(raw_size).div_ceil(8);
                        if need > buf.len() {
                            buf.resize(need, 0);
                        }
                        let got = GetRawInputData(handle, RID_INPUT, Some(buf.as_mut_ptr() as *mut _), &mut size, hdr);
                        if got != u32::MAX && got as usize >= std::mem::size_of::<RAWINPUTHEADER>() {
                            // SAFETY: the buffer holds at least size_of::<RAWINPUT>() initialised bytes.
                            let raw: RAWINPUT = std::ptr::read_unaligned(buf.as_ptr() as *const RAWINPUT);
                            let device = raw.header.hDevice.0 as usize as u64;
                            if raw.header.dwType == RIM_TYPEMOUSE.0 {
                                let m = raw.data.mouse;
                                let flags = m.usFlags.0;
                                let buttons = m.Anonymous.Anonymous.usButtonFlags;
                                if flags & MOUSE_MOVE_ABSOLUTE == 0 && (m.lLastX != 0 || m.lLastY != 0) {
                                    tx.push(InputEvent { t_us, device, kind: InputKind::Motion { dx: m.lLastX, dy: m.lLastY } });
                                }
                                if buttons & RI_MOUSE_LEFT_BUTTON_DOWN != 0 {
                                    tx.push(InputEvent { t_us, device, kind: InputKind::LeftButton { down: true } });
                                }
                                if buttons & RI_MOUSE_LEFT_BUTTON_UP != 0 {
                                    tx.push(InputEvent { t_us, device, kind: InputKind::LeftButton { down: false } });
                                }
                            } else if raw.header.dwType == RIM_TYPEKEYBOARD.0 {
                                let k = raw.data.keyboard;
                                let down = k.Flags & RI_KEY_BREAK == 0;
                                tx.push(InputEvent { t_us, device, kind: InputKind::Key { vkey: k.VKey, down } });
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
            rx.drain(out);
        }
    }

    /// Whether the UI wants events (window focused). While false the input
    /// thread drops reports instead of queueing them.
    pub fn set_accept(&self, on: bool) {
        if let Some(rx) = &self.rx {
            rx.set_accept(on);
        }
    }
}

/// Read pointer speed and "Enhance pointer precision".
pub fn mouse_settings() -> Option<MouseSettings> {
    let p = mouse_params()?;
    Some(MouseSettings { pointer_speed: p.speed, enhance_pointer_precision: p.acceleration != 0 })
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
    write_mouse_params(p, true)
}

/// Write pointer settings for this sign-in session only: other apps are
/// notified, but the user profile is not updated, so signing out or
/// restarting brings back the saved values even if ZeroIn cannot restore.
pub fn set_mouse_params_session(p: zeroin_core::winmouse::MouseParams) -> Result<(), String> {
    write_mouse_params(p, false)
}

fn write_mouse_params(p: zeroin_core::winmouse::MouseParams, persist: bool) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{SPIF_UPDATEINIFILE, SPI_SETMOUSE, SPI_SETMOUSESPEED};
    let p = p.sanitized();
    let mut accel: [i32; 3] = [p.threshold1, p.threshold2, p.acceleration];
    // No SPIF_SENDCHANGE: that flag makes the call broadcast WM_SETTINGCHANGE
    // synchronously, which can stall this thread on another app's hung
    // window. Other apps are told afterwards with a non-blocking notify.
    let flags = if persist { SPIF_UPDATEINIFILE } else { SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0) };
    // SAFETY: SPI_SETMOUSE takes a pointer to three INTs; SPI_SETMOUSESPEED
    // takes the speed itself in pvParam (documented, not a pointer).
    unsafe {
        SystemParametersInfoW(SPI_SETMOUSE, 0, Some(accel.as_mut_ptr() as *mut _), flags).map_err(|e| format!("Acceleration: {e}"))?;
        SystemParametersInfoW(SPI_SETMOUSESPEED, 0, Some(p.speed as usize as *mut _), flags).map_err(|e| format!("Pointer speed: {e}"))?;
    }
    notify_setting_change(SPI_SETMOUSE.0);
    notify_setting_change(SPI_SETMOUSESPEED.0);
    Ok(())
}

/// Tell top-level windows that a system parameter changed without waiting
/// for any of them (SendNotifyMessageW returns at once for other threads).
fn notify_setting_change(action: u32) {
    use windows::Win32::UI::WindowsAndMessaging::{SendNotifyMessageW, HWND_BROADCAST, WM_SETTINGCHANGE};
    // SAFETY: broadcast of a documented message with integer parameters only.
    unsafe {
        let _ = SendNotifyMessageW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(action as usize), LPARAM(0));
    }
}

/// The current Windows sign-in: logon session id plus boot time, so a
/// journal written in an earlier sign-in is recognised as such.
pub fn sign_in_session() -> Option<zeroin_core::winmouse::SignInSession> {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{GetTokenInformation, TokenStatistics, TOKEN_QUERY, TOKEN_STATISTICS};
    use windows::Win32::System::SystemInformation::GetTickCount64;
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    // SAFETY: the token handle is ours and closed below; the output buffer is
    // a TOKEN_STATISTICS of the size passed.
    unsafe {
        let uptime = GetTickCount64() / 1000;
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
        let mut stats = TOKEN_STATISTICS::default();
        let mut len = 0u32;
        let r = GetTokenInformation(
            token,
            TokenStatistics,
            Some(&mut stats as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_STATISTICS>() as u32,
            &mut len,
        );
        let _ = CloseHandle(token);
        r.ok()?;
        let luid = stats.AuthenticationId;
        let logon = ((luid.HighPart as u32 as u64) << 32) | luid.LowPart as u64;
        Some(zeroin_core::winmouse::SignInSession { boot_unix: now.saturating_sub(uptime), logon })
    }
}

// ---------------------------------------------------------------------------
// Sign-out / shutdown notice

static SESSION_END: std::sync::OnceLock<fn()> = std::sync::OnceLock::new();

unsafe extern "system" fn session_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{WM_ENDSESSION, WM_QUERYENDSESSION};
    match msg {
        WM_QUERYENDSESSION => LRESULT(1),
        WM_ENDSESSION => {
            if wparam.0 != 0 {
                if let Some(f) = SESSION_END.get() {
                    f();
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Call `on_end` when Windows signs out or shuts down. Uses a hidden
/// top-level window on its own thread (message-only windows do not get
/// WM_ENDSESSION). The window is never shown.
pub fn watch_session_end(on_end: fn()) {
    if SESSION_END.set(on_end).is_err() {
        return;
    }
    let _ = std::thread::Builder::new().name("zeroin-session-end".into()).spawn(|| {
        use windows::Win32::UI::WindowsAndMessaging::{TranslateMessage, WS_EX_TOOLWINDOW, WS_POPUP};
        // SAFETY: plain Win32 calls on this thread's own hidden window.
        unsafe {
            let Ok(hinstance) = GetModuleHandleW(None) else { return };
            let class = w!("ZeroInSessionEnd");
            let wc =
                WNDCLASSW { lpfnWndProc: Some(session_wndproc), hInstance: hinstance.into(), lpszClassName: class, ..Default::default() };
            RegisterClassW(&wc);
            // Not titled "ZeroIn", so the single-instance check finds only the main window.
            if CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class,
                w!("ZeroIn session watcher"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(hinstance.into()),
                None,
            )
            .is_err()
            {
                return;
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });
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
    let ticks = unix.checked_mul(10_000_000)?.checked_add(116_444_736_000_000_000)?;
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

/// Open a URL with the shell (already validated as HTTPS by the caller).
pub(crate) fn open_url_impl(url: &str) -> bool {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let file = HSTRING::from(url);
    // SAFETY: all strings are valid, null-terminated HSTRINGs for the call's duration.
    let r = unsafe { ShellExecuteW(None, w!("open"), &file, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    r.0 as isize > 32
}

// ---------------------------------------------------------------------------
// Single instance

/// Claim the per-user "ZeroIn is running" mutex. Returns false when another
/// ZeroIn already runs in this sign-in session; its window is then brought
/// to the front. Two instances would overwrite each other's history.
pub fn claim_single_instance() -> bool {
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE};
    // SAFETY: plain Win32 calls; the mutex handle is intentionally kept open
    // for the life of the process (Windows closes it at exit).
    unsafe {
        let Ok(_handle) = CreateMutexW(None, false, w!("Local\\ZeroIn.SingleInstance")) else {
            return true; // cannot tell: do not block start-up
        };
        if GetLastError() != ERROR_ALREADY_EXISTS {
            return true;
        }
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), w!("ZeroIn")) {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
        }
        false
    }
}
