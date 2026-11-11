//! Polling check on Windows: a dedicated high-priority thread receives mouse
//! raw input through buffered reads (`GetRawInputBuffer`) and timestamps each
//! read with the performance counter. Device identity comes from the raw
//! input device name and the HID strings the device reports.

use super::windows::MAIN_RAW_HWND;
use super::{now_ns, CapReport, DeviceIdentity};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use windows::core::{w, PCWSTR};
use windows::Win32::Devices::HumanInterfaceDevice::{HidD_GetManufacturerString, HidD_GetProductString};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL};
use windows::Win32::UI::Input::{
    GetRawInputBuffer, GetRawInputDeviceInfoW, RegisterRawInputDevices, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RAWMOUSE,
    RIDEV_INPUTSINK, RIDI_DEVICENAME, RIM_TYPEMOUSE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetQueueStatus, MsgWaitForMultipleObjects, RegisterClassW, HWND_MESSAGE, QS_RAWINPUT,
    WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW,
};

const MOUSE_MOVE_ABSOLUTE: u16 = 0x0001;
/// Hard limit on one capture (reports), about 60 s at 8000 Hz.
const MAX_REPORTS: usize = 500_000;
/// A capture stops by itself after this long.
const MAX_SECONDS: u64 = 30;

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// A running polling check.
pub struct PollCheck {
    stop: Arc<AtomicBool>,
    count: Arc<AtomicU64>,
    handle: Option<JoinHandle<Result<Vec<CapReport>, String>>>,
}

fn mouse_device(hwnd: HWND) -> RAWINPUTDEVICE {
    RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x02, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd }
}

/// Hand mouse raw input back to the app's raw-input thread.
fn restore_main_registration() -> Result<(), String> {
    let main = MAIN_RAW_HWND.load(Ordering::SeqCst);
    if main == 0 {
        return Ok(());
    }
    let dev = [mouse_device(HWND(main as *mut _))];
    // SAFETY: registers our own message-only window for mouse raw input.
    unsafe { RegisterRawInputDevices(&dev, std::mem::size_of::<RAWINPUTDEVICE>() as u32) }
        .map_err(|e| format!("Could not return mouse input to the test: {e}"))
}

impl PollCheck {
    /// Start capturing. Mouse raw input goes to the check until [`finish`](Self::finish).
    pub fn start() -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let count = Arc::new(AtomicU64::new(0));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let (s2, c2) = (stop.clone(), count.clone());
        let handle = std::thread::Builder::new()
            .name("zeroin-poll-check".into())
            .spawn(move || capture(s2, c2, ready_tx))
            .map_err(|e| format!("Could not start the check: {e}"))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(PollCheck { stop, count, handle: Some(handle) }),
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => Err("The check stopped unexpectedly.".into()),
        }
    }

    /// Reports captured so far.
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Stop, restore normal input and return the captured reports.
    pub fn finish(mut self) -> Result<Vec<CapReport>, String> {
        self.stop.store(true, Ordering::SeqCst);
        match self.handle.take().map(|h| h.join()) {
            Some(Ok(r)) => r,
            _ => {
                let _ = restore_main_registration();
                Err("The check stopped unexpectedly.".into())
            }
        }
    }
}

impl Drop for PollCheck {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn capture(
    stop: Arc<AtomicBool>,
    count: Arc<AtomicU64>,
    ready: std::sync::mpsc::Sender<Result<(), String>>,
) -> Result<Vec<CapReport>, String> {
    // SAFETY: plain Win32 calls on this thread's own message-only window; the
    // buffer walk below stays inside the bytes GetRawInputBuffer reported.
    unsafe {
        let fail = |m: String| {
            let _ = ready.send(Err(m.clone()));
            Err(m)
        };
        let hinstance = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(e) => return fail(format!("Could not start the check: {e}")),
        };
        let class = w!("ZeroInPollCheck");
        let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: hinstance.into(), lpszClassName: class, ..Default::default() };
        RegisterClassW(&wc);
        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("ZeroIn polling check"),
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
            Err(e) => return fail(format!("Could not start the check: {e}")),
        };
        let dev = [mouse_device(hwnd)];
        if let Err(e) = RegisterRawInputDevices(&dev, std::mem::size_of::<RAWINPUTDEVICE>() as u32) {
            let _ = DestroyWindow(hwnd);
            return fail(format!("Raw input is not available: {e}"));
        }
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
        let _ = ready.send(Ok(()));

        let hdr = std::mem::size_of::<RAWINPUTHEADER>() as u32;
        // u64 storage keeps the buffer 8-byte aligned, as RAWINPUT requires.
        let mut buf: Vec<u64> = vec![0; 64 * 1024 / 8];
        let mut out: Vec<CapReport> = Vec::with_capacity(64 * 1024);
        let started = now_ns();
        while !stop.load(Ordering::SeqCst) && now_ns().saturating_sub(started) < MAX_SECONDS * 1_000_000_000 && out.len() < MAX_REPORTS {
            let _ = MsgWaitForMultipleObjects(None, false, 20, QS_RAWINPUT);
            // Clear the "new input" state so the next wait blocks again.
            let _ = GetQueueStatus(QS_RAWINPUT);
            loop {
                let mut cb = (buf.len() * 8) as u32;
                let n = GetRawInputBuffer(Some(buf.as_mut_ptr() as *mut RAWINPUT), &mut cb, hdr);
                if n == 0 || n == u32::MAX {
                    break;
                }
                let t_ns = now_ns();
                let base = buf.as_ptr() as *const u8;
                let end = base.add(buf.len() * 8);
                let mut p = base;
                let mut batch: Vec<CapReport> = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    if p.add(std::mem::size_of::<RAWINPUTHEADER>()) > end {
                        break;
                    }
                    let header = std::ptr::read_unaligned(p as *const RAWINPUTHEADER);
                    let size = header.dwSize as usize;
                    let hsz = std::mem::size_of::<RAWINPUTHEADER>();
                    if size < hsz || p.add(size) > end {
                        break;
                    }
                    if header.dwType == RIM_TYPEMOUSE.0 && size >= hsz + std::mem::size_of::<RAWMOUSE>() {
                        let m = std::ptr::read_unaligned(p.add(hsz) as *const RAWMOUSE);
                        if m.usFlags.0 & MOUSE_MOVE_ABSOLUTE == 0 && (m.lLastX != 0 || m.lLastY != 0) {
                            batch.push(CapReport { t_ns, device: header.hDevice.0 as usize as u64, dx: m.lLastX, dy: m.lLastY, batch: 0 });
                        }
                    }
                    // NEXTRAWINPUTBLOCK: advance by dwSize rounded up to 8 bytes.
                    p = p.add((size + 7) & !7);
                }
                let k = batch.len() as u32;
                out.extend(batch.into_iter().map(|r| CapReport { batch: k, ..r }));
                count.store(out.len() as u64, Ordering::Relaxed);
            }
        }
        let restored = restore_main_registration();
        let _ = DestroyWindow(hwnd);
        restored.map(|_| out)
    }
}

/// Read the device name and the HID strings of a raw input device.
pub fn device_identity(device: u64) -> Option<DeviceIdentity> {
    // SAFETY: buffers are sized as the API reports; the file handle is closed.
    unsafe {
        let h = HANDLE(device as usize as *mut _);
        let mut chars = 0u32;
        GetRawInputDeviceInfoW(Some(h), RIDI_DEVICENAME, None, &mut chars);
        if chars == 0 || chars > 4096 {
            return None;
        }
        let mut name: Vec<u16> = vec![0; chars as usize + 1];
        let got = GetRawInputDeviceInfoW(Some(h), RIDI_DEVICENAME, Some(name.as_mut_ptr() as *mut _), &mut chars);
        if got == u32::MAX || got == 0 {
            return None;
        }
        let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
        let path = String::from_utf16_lossy(&name[..len]);
        let mut id = DeviceIdentity { path, product: None, manufacturer: None, mock: false };
        // Access 0: only attribute queries, never reads input or needs admin rights.
        if let Ok(file) = CreateFileW(
            PCWSTR(name.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        ) {
            let read = |f: unsafe fn(HANDLE, *mut core::ffi::c_void, u32) -> bool| -> Option<String> {
                let mut b = [0u16; 127];
                if f(file, b.as_mut_ptr() as *mut _, (b.len() * 2) as u32) {
                    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
                    let s = zeroin_core::gear::sanitize_text(&String::from_utf16_lossy(&b[..n]), 80);
                    (!s.is_empty()).then_some(s)
                } else {
                    None
                }
            };
            id.product = read(HidD_GetProductString);
            id.manufacturer = read(HidD_GetManufacturerString);
            let _ = CloseHandle(file);
        }
        Some(id)
    }
}
