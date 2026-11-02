use std::sync::atomic::{AtomicBool, Ordering};

pub static LOCKED: AtomicBool = AtomicBool::new(false);
static READY: AtomicBool = AtomicBool::new(false);
pub fn ready() -> bool {
    READY.load(Ordering::Relaxed)
}
pub fn release() {
    LOCKED.store(false, Ordering::SeqCst);
    release_native();
}

#[cfg(target_os = "windows")]
pub fn install() {
    std::thread::spawn(|| unsafe {
        use windows_sys::Win32::{
            System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*,
        };
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard),
            GetModuleHandleW(std::ptr::null()),
            0,
        );
        READY.store(!hook.is_null(), Ordering::Relaxed);
        if hook.is_null() {
            return;
        }
        let mut message = std::mem::zeroed();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        UnhookWindowsHookEx(hook);
        READY.store(false, Ordering::Relaxed);
    });
}
#[cfg(target_os = "windows")]
unsafe extern "system" fn keyboard(code: i32, w: usize, l: isize) -> isize {
    use windows_sys::Win32::UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*};
    unsafe {
        if code >= 0 && LOCKED.load(Ordering::Relaxed) {
            let key = &*(l as *const KBDLLHOOKSTRUCT);
            let alt = key.flags & LLKHF_ALTDOWN != 0;
            let ctrl = (GetAsyncKeyState(VK_CONTROL as i32) as u16 & 0x8000) != 0;
            if key.vkCode == VK_LWIN as u32
                || key.vkCode == VK_RWIN as u32
                || (alt && [VK_TAB as u32, VK_ESCAPE as u32, VK_F4 as u32].contains(&key.vkCode))
                || (ctrl && key.vkCode == VK_ESCAPE as u32)
            {
                return 1;
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, w, l)
    }
}
#[cfg(target_os = "windows")]
pub fn foreground() -> Option<String> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    };
    unsafe {
        let window = GetForegroundWindow();
        if window.is_null() {
            return None;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(window, &mut pid);
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buffer = [0u16; 1024];
        let mut length = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length);
        CloseHandle(process);
        if result == 0 {
            return None;
        }
        std::path::Path::new(&String::from_utf16_lossy(&buffer[..length as usize]))
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use std::{ffi::c_void, sync::atomic::AtomicUsize};
    static TAP: AtomicUsize = AtomicUsize::new(0);
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events: u64,
            callback: unsafe extern "C" fn(
                *mut c_void,
                u32,
                *mut c_void,
                *mut c_void,
            ) -> *mut c_void,
            user: *mut c_void,
        ) -> *mut c_void;
        fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
        fn CGEventGetFlags(event: *mut c_void) -> u64;
        fn CGEventTapEnable(tap: *mut c_void, enable: bool);
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFRunLoopCommonModes: *const c_void;
        fn CFMachPortCreateRunLoopSource(
            allocator: *const c_void,
            port: *mut c_void,
            order: isize,
        ) -> *mut c_void;
        fn CFRunLoopGetCurrent() -> *mut c_void;
        fn CFRunLoopAddSource(runloop: *mut c_void, source: *mut c_void, mode: *const c_void);
        fn CFRunLoopRun();
    }
    unsafe extern "C" fn callback(
        _: *mut c_void,
        kind: u32,
        event: *mut c_void,
        _: *mut c_void,
    ) -> *mut c_void {
        unsafe {
            if kind >= 0xffff_fffe {
                CGEventTapEnable(TAP.load(Ordering::Relaxed) as *mut c_void, true);
                return event;
            }
            if LOCKED.load(Ordering::Relaxed) {
                let key = CGEventGetIntegerValueField(event, 9);
                let flags = CGEventGetFlags(event);
                if (flags & (1 << 20) != 0 && [48, 49, 12, 13].contains(&key))
                    || (flags & (1 << 18) != 0 && (123..=126).contains(&key))
                    || key == 99
                {
                    return std::ptr::null_mut();
                }
            }
            event
        }
    }
    pub fn install() {
        std::thread::spawn(|| unsafe {
            if !AXIsProcessTrusted() {
                return;
            }
            let tap = CGEventTapCreate(
                1,
                0,
                0,
                (1 << 10) | (1 << 11),
                callback,
                std::ptr::null_mut(),
            );
            if tap.is_null() {
                return;
            }
            TAP.store(tap as usize, Ordering::Relaxed);
            let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            if source.is_null() {
                return;
            }
            CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);
            READY.store(true, Ordering::Relaxed);
            CFRunLoopRun();
        });
    }
}
#[cfg(target_os = "macos")]
pub use mac::install;
#[cfg(target_os = "macos")]
pub fn foreground() -> Option<String> {
    let output = std::process::Command::new("osascript").args(["-e", "tell application \"System Events\" to get name of first application process whose frontmost is true"]).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(target_os = "linux")]
pub fn install() {}
#[cfg(target_os = "linux")]
pub fn foreground() -> Option<String> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return None;
    }
    let active = std::process::Command::new("xprop")
        .args(["-root", "_NET_ACTIVE_WINDOW"])
        .output()
        .ok()?;
    let id = String::from_utf8_lossy(&active.stdout)
        .split_whitespace()
        .last()?
        .to_string();
    if !id.starts_with("0x") {
        return None;
    }
    let pid = std::process::Command::new("xprop")
        .args(["-id", &id, "_NET_WM_PID"])
        .output()
        .ok()?;
    let pid = String::from_utf8_lossy(&pid.stdout)
        .split_whitespace()
        .last()?
        .parse::<u32>()
        .ok()?;
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|s| s.trim().to_owned())
}

#[cfg(target_os = "linux")]
static X_DISPLAY: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub fn capture(_window: &tauri::WebviewWindow) {
    #[cfg(target_os = "linux")]
    {
        use raw_window_handle::{
            HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
        };
        if let (Ok(display), Ok(window), Ok(xlib)) = (
            _window.display_handle(),
            _window.window_handle(),
            x11_dl::xlib::Xlib::open(),
        ) {
            if let (RawDisplayHandle::Xlib(display), RawWindowHandle::Xlib(window)) =
                (display.as_raw(), window.as_raw())
            {
                if let Some(display) = display.display {
                    unsafe {
                        let d = display.as_ptr().cast();
                        if (xlib.XGrabKeyboard)(d, window.window, 1, 1, 1, 0) == 0 {
                            X_DISPLAY.store(d as usize, Ordering::Relaxed);
                            READY.store(true, Ordering::Relaxed);
                        }
                    }
                }
            }
        }
    }
    LOCKED.store(true, Ordering::SeqCst);
}
fn release_native() {
    #[cfg(target_os = "linux")]
    {
        let display = X_DISPLAY.swap(0, Ordering::Relaxed);
        if display != 0 {
            if let Ok(xlib) = x11_dl::xlib::Xlib::open() {
                unsafe {
                    (xlib.XUngrabKeyboard)(display as *mut _, 0);
                }
            }
        }
    }
}
