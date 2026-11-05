mod cursor;
mod display;
mod picker;
mod rest;
mod startup;
mod theme;
mod ui;
mod widget;

use crate::runtime::Runtime;
use leafshield_core::{
    Effect, Phase, Settings,
    docking::Dock,
    locale::{Language, tr},
};
use std::{
    cell::RefCell,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::GetModuleHandleW, Threading::*},
    UI::{
        Controls::Dialogs::*, Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*,
        WindowsAndMessaging::*,
    },
};

const HOST: usize = 1;
const MAIN: usize = 2;
const WIDGET: usize = 3;
const REST: usize = 4;
const PREVIEW: usize = 5;
const EVENT_COMMAND: u32 = WM_APP + 1;
const EVENT_CLOSE: u32 = WM_APP + 2;
const EVENT_SLIDER: u32 = WM_APP + 3;
const EVENT_REVEAL: u32 = WM_APP + 4;
const EVENT_DOCK: u32 = WM_APP + 5;
const EVENT_TRAY: u32 = WM_APP + 6;
const EVENT_COMPOSE: u32 = WM_APP + 7;
const EVENT_KEY: u32 = WM_APP + 8;
const EVENT_DISPLAY: u32 = WM_APP + 9;
const EVENT_SHOW: u32 = WM_APP + 10;
const EVENT_DPI: u32 = WM_APP + 11;
const EVENT_TRAY_DISPATCH: u32 = WM_APP + 12;
const ID_SHOW: usize = 10;
const ID_PAUSE: usize = 11;
const ID_REST: usize = 12;
const ID_PREVIEW: usize = 13;
const ID_RESET_WIDGET: usize = 14;
const ID_QUIT: usize = 15;
// TBM_GETPOS is WM_USER; the Win32 metadata does not expose this alias.
const TBM_GETPOS: u32 = WM_USER;

thread_local! { static APP: RefCell<Option<App>> = const { RefCell::new(None) }; }
static LOCKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}
fn role(window: HWND) -> usize {
    unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) as usize }
}
fn text(window: HWND) -> String {
    unsafe {
        let mut value = vec![0u16; GetWindowTextLengthW(window).max(0) as usize + 1];
        let count = GetWindowTextW(window, value.as_mut_ptr(), value.len() as i32);
        String::from_utf16_lossy(&value[..count as usize])
    }
}
fn set_text(window: HWND, value: &str) {
    if text(window) != value {
        unsafe {
            SetWindowTextW(window, wide(value).as_ptr());
        }
    }
}
fn control(window: HWND, id: usize) -> HWND {
    unsafe { GetDlgItem(window, id as i32) }
}
fn checked(window: HWND, id: usize) -> bool {
    unsafe { SendMessageW(control(window, id), BM_GETCHECK, 0, 0) == BST_CHECKED as isize }
}
fn check(window: HWND, id: usize, value: bool) {
    unsafe {
        SendMessageW(
            control(window, id),
            BM_SETCHECK,
            if value { BST_CHECKED } else { BST_UNCHECKED } as usize,
            0,
        );
    }
}
fn selected(window: HWND, id: usize) -> usize {
    unsafe { SendMessageW(control(window, id), CB_GETCURSEL, 0, 0).max(0) as usize }
}
fn invalidate(window: HWND) {
    if !window.is_null() {
        unsafe {
            InvalidateRect(window, null(), 0);
        }
    }
}
fn fmt_time(seconds: u64) -> String {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}
fn scale(window: HWND) -> f64 {
    let dpi = unsafe { GetDpiForWindow(window) };
    dpi.max(96) as f64 / 96.0
}
fn px(value: i32, scale: f64) -> i32 {
    (value as f64 * scale).round() as i32
}
fn rectangle(x: i32, y: i32, w: i32, h: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}
fn fill(dc: HDC, rect: &RECT, color: u32) {
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(dc, rect, brush);
        DeleteObject(brush);
    }
}
fn draw_text(dc: HDC, font: HFONT, value: &str, mut bounds: RECT, color: u32, flags: u32) {
    unsafe {
        let old = SelectObject(dc, font);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, color);
        let value = wide(value);
        DrawTextW(
            dc,
            value.as_ptr(),
            (value.len() - 1) as i32,
            &mut bounds,
            flags,
        );
        SelectObject(dc, old);
    }
}
fn font(height: i32, weight: u32) -> HFONT {
    unsafe {
        CreateFontW(
            -height,
            0,
            0,
            0,
            weight as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            wide("Segoe UI").as_ptr(),
        )
    }
}
fn child(
    parent: HWND,
    class: &str,
    value: &str,
    id: usize,
    styles: u32,
    bounds: RECT,
    font: HFONT,
) -> HWND {
    unsafe {
        let window = CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(value).as_ptr(),
            WS_CHILD | WS_VISIBLE | styles,
            bounds.left,
            bounds.top,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            parent,
            id as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(window, WM_SETFONT, font as usize, 1);
        window
    }
}
fn create(
    kind: usize,
    name: &str,
    styles: u32,
    ex: u32,
    bounds: RECT,
    owner: HWND,
) -> Result<HWND, String> {
    let window = unsafe {
        CreateWindowExW(
            ex,
            wide("LeafShield.Native.Window").as_ptr(),
            wide(name).as_ptr(),
            styles,
            bounds.left,
            bounds.top,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            owner,
            null_mut(),
            GetModuleHandleW(null()),
            kind as *const _,
        )
    };
    if window.is_null() {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(window)
    }
}

pub fn report_error(error: &str) {
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(error).as_ptr(),
            wide("LeafShield").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

struct App {
    runtime: Runtime,
    host: HWND,
    main: HWND,
    widget: HWND,
    page: usize,
    font: HFONT,
    heading: HFONT,
    large: HFONT,
    icon: HICON,
    hook: HHOOK,
    dock: Option<Dock>,
    collapsed: bool,
    outside_since: Option<Instant>,
    resting: Vec<rest::RestWindow>,
    rest_owner: HWND,
    background: Option<rest::Background>,
    last_mouse: POINT,
    emergency: bool,
    last_second: u64,
    last_ui_phase: Phase,
    exiting: bool,
    restore_main: bool,
    pending: Option<std::sync::mpsc::Receiver<ui::PickerResult>>,
    monitors_dirty: bool,
    ui_testing: bool,
    dpi: f64,
}

impl App {
    fn tr(&self, value: &str) -> String {
        tr(self.runtime.engine.data.settings.language, value)
    }
    fn notice(&mut self, result: Result<(), String>) {
        if let Err(error) = result {
            self.runtime.notice = Some(error);
        }
        self.update_status();
    }
    fn update_status(&self) {
        if self.main.is_null() {
            return;
        }
        let status = self
            .runtime
            .notice
            .clone()
            .or_else(|| display::status().error)
            .map(|v| self.tr(&v))
            .unwrap_or_else(|| {
                let state = match self.runtime.engine.phase {
                    Phase::Working => "专注中",
                    Phase::Paused => "已暂停",
                    Phase::Resting => "休息中",
                    Phase::Deferred => "休息已延后",
                };
                format!(
                    "{} · {}",
                    self.tr(state),
                    fmt_time(self.runtime.engine.remaining_seconds())
                )
            });
        set_text(control(self.main, 900), &status);
    }
    fn update_settings(&mut self, settings: Settings) -> Result<(), String> {
        self.runtime.settings(settings)?;
        if !self.ui_testing {
            display::target(
                self.runtime.engine.data.settings.temperature_kelvin,
                self.runtime.engine.data.settings.brightness_percent,
                self.runtime.engine.data.settings.display_enabled,
            );
        }
        self.configure_widget()?;
        self.runtime.notice = None;
        self.update_status();
        Ok(())
    }
    fn apply(&mut self, effect: Effect) {
        match effect {
            Effect::None => {}
            Effect::Warn(seconds) => {
                if self.runtime.engine.data.settings.notifications {
                    self.balloon(&format!("{} {}", seconds, self.tr("秒后开始休息")));
                }
            }
            Effect::Lock => {
                if let Err(error) = self.open_rest(false) {
                    self.close_rest();
                    self.runtime.engine.lock_failed(&error);
                    let _ = self.runtime.persist();
                    self.runtime.notice = Some(error);
                }
            }
            Effect::Unlock => self.close_rest(),
        }
        self.update_status();
        invalidate(self.widget);
    }
    fn command(&mut self, packed: usize, sender: HWND) {
        let id = packed & 0xffff;
        let notification = packed >> 16;
        if id >= 3000 {
            self.rest_command(id, notification, sender);
            return;
        }
        if (100..106).contains(&id) && self.runtime.engine.phase != Phase::Resting {
            self.page = id - 100;
            self.build_page();
            return;
        }
        let result = match id {
            ID_SHOW => self.show_main(),
            ID_PAUSE => {
                let result = self.runtime.action(|engine| {
                    engine.toggle_pause()?;
                    Ok(Effect::None)
                });
                result.map(|effect| {
                    self.apply(effect);
                    self.refresh_dynamic();
                })
            }
            ID_REST => {
                if self.runtime.engine.phase != Phase::Resting {
                    let result = self.runtime.action(|engine| Ok(engine.start_rest()));
                    result.map(|effect| self.apply(effect))
                } else {
                    Ok(())
                }
            }
            ID_PREVIEW => self.open_rest(true),
            ID_RESET_WIDGET => {
                self.reset_widget();
                Ok(())
            }
            ID_QUIT if self.runtime.engine.phase != Phase::Resting => {
                self.exiting = true;
                unsafe {
                    PostQuitMessage(0);
                }
                Ok(())
            }
            1000..=2999
                if self.runtime.engine.phase != Phase::Resting
                    && !sender.is_null()
                    && unsafe { IsWindow(sender) } != 0
                    && unsafe { GetParent(sender) } == self.main
                    && unsafe { GetDlgCtrlID(sender) } == id as i32 =>
            {
                self.settings_command(id, notification)
            }
            _ => Ok(()),
        };
        self.notice(result);
    }
    fn tick(&mut self) {
        self.poll_picker();
        if std::mem::take(&mut self.monitors_dirty) {
            // A display change broadcasts to every window. Rebuild once, and
            // pause safely if rebuilding a real rest window fails.
            if !self.resting.is_empty() {
                let preview = self.runtime.engine.phase != Phase::Resting;
                self.close_rest();
                if let Err(error) = self.open_rest(preview) {
                    self.close_rest();
                    if !preview {
                        self.runtime.engine.lock_failed(&error);
                        let _ = self.runtime.persist();
                    }
                    self.runtime.notice = Some(error);
                }
            }
            self.reset_widget();
        }
        // Foreground queries only near the boundary or while a whitelist defers the rest.
        let foreground = if !self.runtime.engine.data.settings.whitelist.is_empty()
            && (self.runtime.engine.phase == Phase::Deferred
                || self.runtime.engine.remaining_seconds() <= 2)
        {
            foreground()
        } else {
            None
        };
        let effect = self.runtime.tick(foreground.as_deref());
        if effect != Effect::None {
            self.apply(effect);
        }
        self.poll_widget();
        if !self.resting.is_empty() {
            self.poll_rest();
        }
        let seconds = self.runtime.engine.remaining_seconds();
        let phase = self.runtime.engine.phase;
        if self.last_second != seconds || self.last_ui_phase != phase {
            self.last_second = seconds;
            self.last_ui_phase = phase;
            self.update_tray();
            self.refresh_dynamic();
            if !self.collapsed {
                invalidate(self.widget);
            }
            for rest in &self.resting {
                rest::invalidate_timer(rest.hwnd);
            }
        }
    }
    fn update_tray(&self) {
        unsafe {
            let mut data: NOTIFYICONDATAW = zeroed();
            data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = self.host;
            data.uID = 1;
            data.uFlags = NIF_TIP;
            let label = wide(&format!(
                "LeafShield · {}",
                fmt_time(self.runtime.engine.remaining_seconds())
            ));
            let count = label.len().min(data.szTip.len());
            data.szTip[..count].copy_from_slice(&label[..count]);
            data.szTip[data.szTip.len() - 1] = 0;
            Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }
    fn balloon(&self, message: &str) {
        unsafe {
            let mut data: NOTIFYICONDATAW = zeroed();
            data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = self.host;
            data.uID = 1;
            data.uFlags = NIF_INFO;
            data.dwInfoFlags = NIIF_INFO;
            let label = wide(message);
            let count = label.len().min(data.szInfo.len() - 1);
            data.szInfo[..count].copy_from_slice(&label[..count]);
            let title = wide("LeafShield");
            data.szInfoTitle[..title.len()].copy_from_slice(&title);
            Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }
    fn create_menu(&self) -> HMENU {
        unsafe {
            let menu = CreatePopupMenu();
            let locked = self.runtime.engine.phase == Phase::Resting;
            for (id, label) in [
                (ID_SHOW, "打开一叶"),
                (
                    ID_PAUSE,
                    if self.runtime.engine.phase == Phase::Paused {
                        "继续计时"
                    } else {
                        "暂停计时"
                    },
                ),
                (ID_REST, "立即休息"),
                (ID_PREVIEW, "预览锁屏"),
                (ID_RESET_WIDGET, "重置悬浮窗位置"),
                (ID_QUIT, "退出一叶"),
            ] {
                AppendMenuW(
                    menu,
                    MF_STRING | if locked { MF_GRAYED } else { 0 },
                    id,
                    wide(&self.tr(label)).as_ptr(),
                );
            }
            menu
        }
    }
}

fn show_tray_menu() {
    // Release the application borrow before the native modal menu pumps messages.
    // Timers and painting must keep working while the menu is open.
    let (host, menu) = APP.with(|cell| {
        let value = cell.borrow();
        let app = value.as_ref().unwrap();
        (app.host, app.create_menu())
    });
    unsafe {
        let mut point = POINT::default();
        GetCursorPos(&mut point);
        SetForegroundWindow(host);
        SetPropW(host, wide("LeafShield.ActiveMenu").as_ptr(), menu);
        let id = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            host,
            null(),
        );
        RemovePropW(host, wide("LeafShield.ActiveMenu").as_ptr());
        DestroyMenu(menu);
        if id != 0 {
            PostMessageW(host, EVENT_COMMAND, id as usize, 0);
        }
        PostMessageW(host, WM_NULL, 0, 0);
    }
}

impl Drop for App {
    fn drop(&mut self) {
        LOCKED.store(false, std::sync::atomic::Ordering::SeqCst);
        display::stop();
        if let Err(error) = self.runtime.persist() {
            report_error(&error);
        }
        unsafe {
            if !self.hook.is_null() {
                UnhookWindowsHookEx(self.hook);
            }
            let mut tray: NOTIFYICONDATAW = zeroed();
            tray.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            tray.hWnd = self.host;
            tray.uID = 1;
            Shell_NotifyIconW(NIM_DELETE, &tray);
            for rest in &self.resting {
                DestroyWindow(rest.hwnd);
                DeleteObject(rest.font);
            }
            for window in [self.main, self.widget, self.host] {
                if !window.is_null() {
                    DestroyWindow(window);
                }
            }
            for object in [self.font, self.heading, self.large] {
                DeleteObject(object);
            }
            if !self.icon.is_null() {
                DestroyIcon(self.icon);
            }
        }
    }
}

unsafe extern "system" fn keyboard(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 && LOCKED.load(std::sync::atomic::Ordering::Relaxed) {
            let key = &*(l as *const KBDLLHOOKSTRUCT);
            let ctrl = (GetAsyncKeyState(VK_CONTROL as i32) as u16 & 0x8000) != 0;
            if [VK_LWIN as u32, VK_RWIN as u32].contains(&key.vkCode)
                || (key.flags & LLKHF_ALTDOWN != 0
                    && [VK_TAB as u32, VK_ESCAPE as u32, VK_F4 as u32].contains(&key.vkCode))
                || (ctrl && key.vkCode == VK_ESCAPE as u32)
            {
                return 1;
            }
        }
        CallNextHookEx(null_mut(), code, w, l)
    }
}

fn foreground() -> Option<String> {
    unsafe {
        let window = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(window, &mut pid);
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buffer = [0u16; 32768];
        let mut length = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length);
        CloseHandle(process);
        if result == 0 {
            return None;
        }
        std::path::Path::new(&String::from_utf16_lossy(&buffer[..length as usize]))
            .file_name()
            .map(|v| v.to_string_lossy().into_owned())
    }
}

unsafe extern "system" fn window_proc(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            SetWindowLongPtrW(
                window,
                GWLP_USERDATA,
                (*(l as *const CREATESTRUCTW)).lpCreateParams as isize,
            );
            return DefWindowProcW(window, message, w, l);
        }
        let kind = role(window);
        let host = if kind == HOST {
            window
        } else {
            GetWindow(window, GW_OWNER)
        };
        match message {
            EVENT_TRAY if kind == HOST => {
                // Shell callbacks can arrive as sent messages, bypassing the
                // outer GetMessage loop. Queue a separate event before opening UI.
                PostMessageW(window, EVENT_TRAY_DISPATCH, w, l);
                return 0;
            }
            WM_CLOSE => {
                PostMessageW(host, EVENT_CLOSE, window as usize, 0);
                return 0;
            }
            WM_COMMAND => {
                PostMessageW(host, EVENT_COMMAND, w, l);
                return 0;
            }
            WM_HSCROLL => {
                PostMessageW(host, EVENT_SLIDER, w, l);
                return 0;
            }
            WM_TIMER if kind == HOST => {
                APP.with(|cell| {
                    if let Ok(mut value) = cell.try_borrow_mut()
                        && let Some(app) = value.as_mut()
                    {
                        app.tick();
                    }
                });
                return 0;
            }
            WM_ERASEBKGND => return 1,
            WM_PAINT | WM_PRINTCLIENT => {
                let mut paint = zeroed();
                let dc = if message == WM_PAINT {
                    BeginPaint(window, &mut paint)
                } else {
                    w as HDC
                };
                APP.with(|cell| {
                    if let Ok(value) = cell.try_borrow()
                        && let Some(app) = value.as_ref()
                    {
                        match kind {
                            MAIN => app.paint_main(dc),
                            WIDGET => app.paint_widget(dc),
                            REST | PREVIEW => app.paint_rest(window, dc),
                            _ => {}
                        }
                    }
                });
                if message == WM_PAINT {
                    EndPaint(window, &paint);
                }
                return 0;
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORBTN => {
                // SetWindowText can synchronously request painting while App is
                // mutably borrowed. A stock DC brush needs no application borrow.
                let background = if GetDlgCtrlID(l as HWND) == 900 {
                    theme::BACKGROUND
                } else {
                    theme::PANEL
                };
                SetTextColor(
                    w as HDC,
                    if matches!(GetDlgCtrlID(l as HWND), 1413 | 1414) {
                        theme::MUTED
                    } else {
                        theme::TEXT
                    },
                );
                SetBkMode(w as HDC, OPAQUE as i32);
                SetBkColor(w as HDC, background);
                SetDCBrushColor(w as HDC, background);
                return GetStockObject(DC_BRUSH) as isize;
            }
            WM_DRAWITEM => {
                let draw = &*(l as *const DRAWITEMSTRUCT);
                if (100..106).contains(&draw.CtlID) {
                    theme::paint_navigation(draw);
                } else {
                    theme::paint_button(draw);
                }
                return 1;
            }
            WM_SETCURSOR if matches!(kind, REST | PREVIEW) => {
                SetCursor(null_mut());
                return 1;
            }
            WM_MOUSEMOVE if matches!(kind, REST | PREVIEW) => {
                PostMessageW(host, EVENT_REVEAL, window as usize, 0);
            }
            WM_LBUTTONDOWN if kind == WIDGET => {
                let mut bounds = RECT::default();
                GetClientRect(window, &mut bounds);
                if bounds.right < px(40, scale(window)) {
                    return 0;
                }
                if (l as u16 as i16 as i32) >= bounds.right - px(32, scale(window)) {
                    PostMessageW(host, EVENT_SHOW, 0, 0);
                } else {
                    ReleaseCapture();
                    let mut point = POINT {
                        x: l as u16 as i16 as i32,
                        y: ((l as usize >> 16) as u16) as i16 as i32,
                    };
                    ClientToScreen(window, &mut point);
                    let screen =
                        (point.x as u16 as usize | ((point.y as u16 as usize) << 16)) as isize;
                    SendMessageW(window, WM_NCLBUTTONDOWN, HTCAPTION as usize, screen);
                }
                return 0;
            }
            WM_NCLBUTTONDBLCLK if kind == WIDGET => {
                PostMessageW(host, EVENT_SHOW, 0, 0);
                return 0;
            }
            WM_LBUTTONDBLCLK if kind == WIDGET => {
                PostMessageW(host, EVENT_SHOW, 0, 0);
                return 0;
            }
            WM_CONTEXTMENU if kind == WIDGET => {
                PostMessageW(host, EVENT_TRAY, 0, WM_CONTEXTMENU as isize);
                return 0;
            }
            WM_EXITSIZEMOVE if kind == WIDGET => {
                RemovePropW(window, wide("LeafShield.Dragging").as_ptr());
                PostMessageW(host, EVENT_DOCK, 0, 0);
                return 0;
            }
            WM_ENTERSIZEMOVE if kind == WIDGET => {
                SetPropW(
                    window,
                    wide("LeafShield.Dragging").as_ptr(),
                    1usize as HANDLE,
                );
                return 0;
            }
            WM_KEYDOWN
                if matches!(kind, REST | PREVIEW) && w == VK_ESCAPE as usize && kind == PREVIEW =>
            {
                PostMessageW(host, EVENT_CLOSE, window as usize, 0);
                return 0;
            }
            WM_DISPLAYCHANGE => {
                PostMessageW(host, EVENT_DISPLAY, 0, 0);
            }
            WM_DPICHANGED => {
                let bounds = &*(l as *const RECT);
                SetWindowPos(
                    window,
                    null_mut(),
                    bounds.left,
                    bounds.top,
                    bounds.right - bounds.left,
                    bounds.bottom - bounds.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                PostMessageW(host, EVENT_DPI, window as usize, 0);
                return 0;
            }
            WM_QUERYENDSESSION => return 1,
            WM_ENDSESSION if w != 0 => {
                PostQuitMessage(0);
                return 0;
            }
            WM_DESTROY if kind == HOST => {
                PostQuitMessage(0);
                return 0;
            }
            _ => {}
        }
        DefWindowProcW(window, message, w, l)
    }
}

pub fn run() -> Result<(), String> {
    unsafe {
        let arguments: Vec<_> = std::env::args().collect();
        let test_directory = arguments
            .windows(2)
            .find(|args| args[0] == "--ui-test-profile")
            .map(|args| std::path::PathBuf::from(&args[1]));
        let ui_testing = test_directory.is_some();
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mutex_name = if ui_testing {
            format!("Local\\LeafShield.Native.Tests.{}", std::process::id())
        } else {
            "Local\\LeafShield.Native.Windows".into()
        };
        let mutex = CreateMutexW(null(), 0, wide(&mutex_name).as_ptr());
        if mutex.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        struct MutexHandle(HANDLE);
        impl Drop for MutexHandle {
            fn drop(&mut self) {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
        let _guard = MutexHandle(mutex);
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let existing = FindWindowW(
                wide("LeafShield.Native.Window").as_ptr(),
                wide("LeafShield Native Host").as_ptr(),
            );
            if !existing.is_null() {
                PostMessageW(existing, EVENT_SHOW, 0, 0);
            }
            return Ok(());
        }
        let classes = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES | ICC_STANDARD_CLASSES,
        };
        InitCommonControlsEx(&classes);
        let class = wide("LeafShield.Native.Window");
        let wc = WNDCLASSW {
            style: CS_DBLCLKS,
            lpfnWndProc: Some(window_proc),
            hInstance: GetModuleHandleW(null()),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let directory = if let Some(directory) = test_directory {
            directory
        } else {
            std::path::PathBuf::from(
                std::env::var_os("APPDATA").ok_or("无法找到 Windows 应用数据目录")?,
            )
            .join("com.leafshield.desktop")
        };
        let runtime = Runtime::load(directory)?;
        let backup = runtime.directory.join("settings.pre-native-dev1.json");
        if !backup.exists() && runtime.directory.join("settings.json").is_file() {
            std::fs::copy(runtime.directory.join("settings.json"), backup)
                .map_err(|e| e.to_string())?;
        }
        let host = create(
            HOST,
            "LeafShield Native Host",
            WS_POPUP,
            WS_EX_TOOLWINDOW,
            rectangle(0, 0, 0, 0),
            null_mut(),
        )?;
        let dpi = GetDpiForSystem().max(96) as f64 / 96.0;
        let icon_bytes = include_bytes!("../../../../src-tauri/icons/icon.ico");
        // Pick an ICO image with the system decoder; fall back to the shared system icon.
        let image_offset =
            LookupIconIdFromDirectoryEx(icon_bytes.as_ptr() as *mut u8, 1, 32, 32, LR_DEFAULTCOLOR);
        let icon = if image_offset > 0 {
            CreateIconFromResourceEx(
                icon_bytes.as_ptr().add(image_offset as usize),
                (icon_bytes.len() - image_offset as usize) as u32,
                1,
                0x30000,
                32,
                32,
                LR_DEFAULTCOLOR,
            )
        } else {
            CopyIcon(LoadIconW(null_mut(), IDI_APPLICATION))
        };
        let hook = if ui_testing {
            null_mut()
        } else {
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), GetModuleHandleW(null()), 0)
        };
        let app = App {
            runtime,
            host,
            main: null_mut(),
            widget: null_mut(),
            page: 0,
            font: font(px(14, dpi), FW_NORMAL),
            heading: font(px(22, dpi), FW_SEMIBOLD),
            large: font(px(64, dpi), FW_NORMAL),
            icon,
            hook,
            dock: None,
            collapsed: false,
            outside_since: None,
            resting: Vec::new(),
            rest_owner: null_mut(),
            background: None,
            last_mouse: zeroed(),
            emergency: false,
            last_second: u64::MAX,
            last_ui_phase: Phase::Working,
            exiting: false,
            restore_main: false,
            pending: None,
            monitors_dirty: false,
            ui_testing,
            dpi,
        };
        let mut tray: NOTIFYICONDATAW = zeroed();
        tray.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        tray.hWnd = host;
        tray.uID = 1;
        tray.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        tray.hIcon = icon;
        tray.uCallbackMessage = EVENT_TRAY;
        let name = wide("LeafShield Native");
        tray.szTip[..name.len()].copy_from_slice(&name);
        if Shell_NotifyIconW(NIM_ADD, &tray) == 0 {
            return Err("无法创建系统托盘图标".into());
        }
        if !ui_testing {
            display::start(&app.runtime.engine.data.settings);
        }
        let background_start = std::env::args().any(|arg| arg == "--autostart");
        APP.with(|cell| *cell.borrow_mut() = Some(app));
        APP.with(|cell| {
            let mut value = cell.borrow_mut();
            let app = value.as_mut().unwrap();
            let result = app.configure_widget();
            app.notice(result);
            if !background_start {
                let result = app.show_main();
                app.notice(result);
            }
        });
        SetTimer(host, 1, 100, None);
        let taskbar_created = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
        let mut message: MSG = zeroed();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            let mut handled = false;
            let mut menu_requested = false;
            APP.with(|cell| {
                let mut value = cell.borrow_mut();
                let app = value.as_mut().unwrap();
                if message.hwnd == app.host {
                    handled = true;
                    match message.message {
                        WM_TIMER => app.tick(),
                        EVENT_COMMAND => app.command(message.wParam, message.lParam as HWND),
                        EVENT_SHOW => {
                            let result = app.show_main();
                            app.notice(result);
                        }
                        EVENT_CLOSE => app.close_window(message.wParam as HWND),
                        EVENT_SLIDER => app.slider(message.wParam, message.lParam as HWND),
                        EVENT_REVEAL => {
                            app.reveal_rest(message.wParam as HWND, message.lParam != 0)
                        }
                        EVENT_COMPOSE => app.compose(message.lParam as HWND, message.wParam != 0),
                        EVENT_KEY => app.rest_key(message.wParam, message.lParam as HWND),
                        EVENT_DOCK => app.snap_widget(),
                        EVENT_DISPLAY => {
                            app.monitors_dirty = true;
                        }
                        EVENT_DPI => {
                            if message.wParam as HWND == app.main {
                                app.rebuild_main();
                            }
                        }
                        EVENT_TRAY_DISPATCH => match message.lParam as u32 & 0xffff {
                            WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                                let result = app.show_main();
                                app.notice(result);
                            }
                            WM_RBUTTONUP | WM_CONTEXTMENU => menu_requested = true,
                            _ => {}
                        },
                        value if value == taskbar_created => {
                            Shell_NotifyIconW(NIM_ADD, &tray);
                        }
                        _ => handled = false,
                    }
                }
            });
            if menu_requested {
                show_tray_menu();
            }
            if !handled {
                let main = APP.with(|cell| cell.borrow().as_ref().unwrap().main);
                if main.is_null()
                    || (message.hwnd != main && IsChild(main, message.hwnd) == 0)
                    || IsDialogMessageW(main, &message) == 0
                {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
        KillTimer(host, 1);
        let app = APP.with(|cell| cell.borrow_mut().take());
        drop(app);
        Ok(())
    }
}
