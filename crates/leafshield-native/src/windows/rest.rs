use super::*;
use image::{ImageReader, imageops::FilterType};
use std::io::{Cursor, Read};

pub(super) struct RestWindow {
    pub hwnd: HWND,
    pub edit: HWND,
    button: HWND,
    emergency_button: HWND,
    pub font: HFONT,
    pub preview: bool,
    revealed: bool,
    composing: bool,
}
pub(super) struct Background {
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}
impl Background {
    fn load(runtime: &Runtime) -> Result<Self, String> {
        let settings = &runtime.engine.data.settings;
        let custom = if settings.theme == "custom" {
            let name = settings
                .custom_wallpaper
                .as_deref()
                .filter(|v| leafshield_core::wallpaper::owned_name(v))
                .ok_or("自定义图片无效，请重新选择")?;
            let mut bytes = Vec::new();
            std::fs::File::open(runtime.directory.join("wallpapers").join(name))
                .map_err(|e| e.to_string())?
                .take(leafshield_core::wallpaper::MAX_STORED as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > leafshield_core::wallpaper::MAX_STORED {
                return Err("自定义图片无效，请重新选择".into());
            }
            Some(bytes)
        } else {
            None
        };
        let bytes: &[u8] = match settings.theme.as_str() {
            "mountain" => include_bytes!("../../../../public/wallpapers/mountain.jpg"),
            "night" => include_bytes!("../../../../public/wallpapers/night.jpg"),
            _ => custom
                .as_deref()
                .unwrap_or(include_bytes!("../../../../public/wallpapers/forest.jpg")),
        };
        let mut reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode().map_err(|e| e.to_string())?;
        let image = if image.width() > 1920 || image.height() > 1200 {
            image.resize(1920, 1200, FilterType::Triangle)
        } else {
            image
        };
        let mut rgba = image.into_rgba8();
        for pixel in rgba.pixels_mut() {
            let [r, g, b, _] = pixel.0;
            pixel.0 = [
                (b as u16 * 45 / 100) as u8,
                (g as u16 * 45 / 100) as u8,
                (r as u16 * 45 / 100) as u8,
                255,
            ];
        }
        Ok(Self {
            width: rgba.width() as i32,
            height: rgba.height() as i32,
            pixels: rgba.into_raw(),
        })
    }
    fn paint(&self, dc: HDC, bounds: RECT) {
        let width = bounds.right - bounds.left;
        let height = bounds.bottom - bounds.top;
        if width <= 0 || height <= 0 {
            return;
        }
        let aspect = width as f64 / height as f64;
        let (source_w, source_h) = if self.width as f64 / self.height as f64 > aspect {
            ((self.height as f64 * aspect) as i32, self.height)
        } else {
            (self.width, (self.width as f64 / aspect) as i32)
        };
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: self.width,
                biHeight: -self.height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..unsafe { zeroed() }
            },
            ..unsafe { zeroed() }
        };
        unsafe {
            SetStretchBltMode(dc, COLORONCOLOR);
            StretchDIBits(
                dc,
                0,
                0,
                width,
                height,
                (self.width - source_w) / 2,
                (self.height - source_h) / 2,
                source_w,
                source_h,
                self.pixels.as_ptr().cast(),
                &info,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        }
    }
}

unsafe extern "system" fn monitor(_: HMONITOR, _: HDC, bounds: *mut RECT, data: LPARAM) -> i32 {
    unsafe {
        (*(data as *mut Vec<RECT>)).push(*bounds);
    }
    1
}
fn monitors() -> Vec<RECT> {
    let mut bounds = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            null_mut(),
            null(),
            Some(monitor),
            &mut bounds as *mut _ as isize,
        );
    }
    bounds
}

unsafe extern "system" fn edit_proc(
    window: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    host: usize,
) -> LRESULT {
    unsafe {
        let parent = GetParent(window);
        match message {
            WM_PASTE | WM_COPY | WM_CUT | WM_CONTEXTMENU | WM_DROPFILES => return 0,
            WM_SETCURSOR => {
                SetCursor(null_mut());
                return 1;
            }
            WM_KEYDOWN => {
                if l as usize & (1 << 30) == 0 {
                    PostMessageW(host as HWND, EVENT_KEY, w, parent as isize);
                }
                if w == VK_RETURN as usize || w == VK_ESCAPE as usize {
                    return 0;
                }
                if w == b'A' as usize && GetAsyncKeyState(VK_CONTROL as i32) as u16 & 0x8000 != 0 {
                    SendMessageW(window, EM_SETSEL, 0, -1);
                    return 0;
                }
            }
            WM_CHAR if w == 13 || w == 27 => return 0,
            WM_IME_STARTCOMPOSITION => {
                PostMessageW(host as HWND, EVENT_REVEAL, parent as usize, 1);
                PostMessageW(host as HWND, EVENT_COMPOSE, 1, parent as isize);
            }
            WM_IME_ENDCOMPOSITION => {
                PostMessageW(host as HWND, EVENT_COMPOSE, 0, parent as isize);
            }
            WM_NCDESTROY => {
                RemoveWindowSubclass(window, Some(edit_proc), 1);
            }
            _ => {}
        }
        DefSubclassProc(window, message, w, l)
    }
}

fn input_bounds(window: HWND) -> RECT {
    let mut bounds = RECT::default();
    unsafe {
        GetClientRect(window, &mut bounds);
    }
    let dpi = scale(window);
    let width = (bounds.right - px(72, dpi)).min(px(760, dpi));
    rectangle(
        (bounds.right - width) / 2,
        bounds.bottom / 2 + px(46, dpi),
        width,
        px(36, dpi),
    )
}
pub(super) fn invalidate_timer(window: HWND) {
    let mut bounds = RECT::default();
    unsafe {
        GetClientRect(window, &mut bounds);
    }
    let dpi = scale(window);
    let timer = rectangle(
        0,
        px(110, dpi).min(bounds.bottom / 6) + px(42, dpi),
        bounds.right,
        px(100, dpi),
    );
    unsafe {
        InvalidateRect(window, &timer, 0);
    }
}

impl App {
    pub(super) fn open_rest(&mut self, preview: bool) -> Result<(), String> {
        if preview && self.runtime.engine.phase == Phase::Resting {
            return Err(self.tr("休息结束后可以预览锁屏"));
        }
        if !preview && self.hook.is_null() {
            return Err("键盘保护不可用，请重新启动一叶".into());
        }
        if !self.resting.is_empty() {
            self.close_rest();
        }
        let monitors = monitors();
        if monitors.is_empty() {
            return Err("没有检测到显示器".into());
        }
        self.restore_main = !self.main.is_null() && unsafe { IsWindowVisible(self.main) } != 0;
        self.background = match Background::load(&self.runtime) {
            Ok(background) => Some(background),
            Err(error) => {
                self.runtime.notice = Some(error);
                None
            }
        };
        self.emergency = false;
        self.rest_owner = null_mut();
        unsafe {
            GetCursorPos(&mut self.last_mouse);
        }
        for bounds in monitors {
            let hwnd = match create(
                if preview { PREVIEW } else { REST },
                &self.tr("休息时间 · 一叶"),
                WS_POPUP | WS_CLIPCHILDREN,
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                bounds,
                self.host,
            ) {
                Ok(window) => window,
                Err(error) => {
                    self.close_rest();
                    return Err(error);
                }
            };
            let dpi = scale(hwnd);
            let font = font(px(16, dpi), FW_NORMAL);
            // A focused native EDIT stays outside the client area until activity. This
            // preserves the first key/IME composition without exposing an input box.
            let edit = child(
                hwnd,
                "EDIT",
                "",
                3001,
                WS_TABSTOP | ES_AUTOHSCROLL as u32 | WS_BORDER,
                rectangle(-32000, -32000, px(600, dpi), px(36, dpi)),
                font,
            );
            let button = child(
                hwnd,
                "BUTTON",
                &self.tr("正念解锁"),
                3000,
                WS_TABSTOP | BS_PUSHBUTTON as u32,
                rectangle(0, 0, 0, 0),
                font,
            );
            let emergency_button = child(
                hwnd,
                "BUTTON",
                &self.tr("突发紧急情况？"),
                3002,
                WS_TABSTOP | BS_PUSHBUTTON as u32,
                rectangle(0, 0, 0, 0),
                font,
            );
            unsafe {
                SendMessageW(edit, EM_SETLIMITTEXT, 240, 0);
                SetWindowSubclass(edit, Some(edit_proc), 1, self.host as usize);
                EnableWindow(button, 0);
                ShowWindow(button, SW_HIDE);
                ShowWindow(emergency_button, SW_HIDE);
            }
            if preview {
                let mut client = RECT::default();
                unsafe {
                    GetClientRect(hwnd, &mut client);
                }
                child(
                    hwnd,
                    "BUTTON",
                    &self.tr("关闭预览 ×"),
                    3003,
                    WS_TABSTOP | BS_PUSHBUTTON as u32,
                    rectangle(
                        client.right - px(190, dpi),
                        px(18, dpi),
                        px(160, dpi),
                        px(30, dpi),
                    ),
                    font,
                );
            }
            self.resting.push(RestWindow {
                hwnd,
                edit,
                button,
                emergency_button,
                font,
                preview,
                revealed: false,
                composing: false,
            });
        }
        if !preview {
            LOCKED.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        unsafe {
            if !self.main.is_null() {
                ShowWindow(self.main, SW_HIDE);
            }
            if !self.widget.is_null() {
                ShowWindow(self.widget, SW_HIDE);
            }
            for rest in &self.resting {
                SetWindowPos(
                    rest.hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                invalidate(rest.hwnd);
            }
            if let Some(first) = self.resting.first() {
                SetForegroundWindow(first.hwnd);
                SetFocus(first.edit);
            }
        }
        Ok(())
    }
    pub(super) fn close_rest(&mut self) {
        LOCKED.store(false, std::sync::atomic::Ordering::SeqCst);
        self.rest_owner = null_mut();
        self.emergency = false;
        for rest in self.resting.drain(..) {
            unsafe {
                DestroyWindow(rest.hwnd);
                DeleteObject(rest.font);
            }
        }
        self.background = None;
        let result = self.configure_widget();
        if let Err(error) = result {
            self.runtime.notice = Some(error);
        }
        if self.restore_main && !self.main.is_null() {
            unsafe {
                ShowWindow(self.main, SW_SHOW);
                SetForegroundWindow(self.main);
            }
        }
        self.restore_main = false;
    }
    pub(super) fn reveal_rest(&mut self, window: HWND, keyboard: bool) {
        if !self.resting.iter().any(|rest| rest.hwnd == window) {
            return;
        }
        if !keyboard {
            let mut point = POINT::default();
            unsafe {
                GetCursorPos(&mut point);
            }
            if point.x == self.last_mouse.x && point.y == self.last_mouse.y {
                return;
            }
            let mut bounds = RECT::default();
            unsafe {
                GetWindowRect(window, &mut bounds);
            }
            if point.x < bounds.left
                || point.y < bounds.top
                || point.x >= bounds.right
                || point.y >= bounds.bottom
            {
                return;
            }
            self.last_mouse = point;
        }
        if self.rest_owner == window {
            return;
        }
        self.rest_owner = window;
        for rest in &mut self.resting {
            rest.revealed = rest.hwnd == window;
            let dpi = scale(rest.hwnd);
            unsafe {
                if rest.revealed {
                    let bounds = input_bounds(rest.hwnd);
                    MoveWindow(
                        rest.edit,
                        bounds.left,
                        bounds.top,
                        bounds.right - bounds.left,
                        bounds.bottom - bounds.top,
                        1,
                    );
                    MoveWindow(
                        rest.button,
                        bounds.right - px(160, dpi),
                        bounds.bottom + px(14, dpi),
                        px(160, dpi),
                        px(32, dpi),
                        1,
                    );
                    MoveWindow(
                        rest.emergency_button,
                        bounds.left,
                        bounds.bottom + px(14, dpi),
                        px(220, dpi),
                        px(32, dpi),
                        1,
                    );
                    ShowWindow(rest.button, SW_SHOWNOACTIVATE);
                    if self.runtime.engine.data.settings.emergency_enabled {
                        ShowWindow(rest.emergency_button, SW_SHOWNOACTIVATE);
                    }
                    SetForegroundWindow(rest.hwnd);
                    SetFocus(rest.edit);
                } else {
                    SetWindowPos(
                        rest.edit,
                        null_mut(),
                        -32000,
                        -32000,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                    ShowWindow(rest.button, SW_HIDE);
                    ShowWindow(rest.emergency_button, SW_HIDE);
                }
            }
            invalidate(rest.hwnd);
        }
        self.validate_input(window);
    }
    pub(super) fn poll_rest(&mut self) {
        let mut point = POINT::default();
        unsafe {
            GetCursorPos(&mut point);
        }
        if point.x != self.last_mouse.x || point.y != self.last_mouse.y {
            let target = self
                .resting
                .iter()
                .find(|rest| {
                    let mut bounds = RECT::default();
                    unsafe {
                        GetWindowRect(rest.hwnd, &mut bounds);
                    }
                    point.x >= bounds.left
                        && point.x < bounds.right
                        && point.y >= bounds.top
                        && point.y < bounds.bottom
                })
                .map(|rest| rest.hwnd);
            if let Some(target) = target {
                self.reveal_rest(target, false);
            }
        }
        // Reassert stacking without activating another screen or stealing IME focus.
        for rest in &self.resting {
            unsafe {
                SetWindowPos(
                    rest.hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
    }
    pub(super) fn compose(&mut self, window: HWND, composing: bool) {
        if let Some(rest) = self.resting.iter_mut().find(|rest| rest.hwnd == window) {
            rest.composing = composing;
        }
        self.validate_input(window);
    }
    fn validate_input(&self, window: HWND) {
        if let Some(rest) = self.resting.iter().find(|rest| rest.hwnd == window) {
            unsafe {
                EnableWindow(
                    rest.button,
                    i32::from(
                        !rest.composing
                            && text(rest.edit) == self.runtime.engine.data.settings.phrase,
                    ),
                );
            }
            let bounds = input_bounds(window);
            let mut client = RECT::default();
            unsafe {
                GetClientRect(window, &mut client);
            }
            let area = rectangle(
                0,
                bounds.bottom,
                client.right,
                (client.bottom - bounds.bottom).max(0),
            );
            unsafe {
                InvalidateRect(window, &area, 0);
            }
        }
    }
    pub(super) fn rest_key(&mut self, key: usize, window: HWND) {
        self.reveal_rest(window, true);
        let Some(rest) = self.resting.iter().find(|rest| rest.hwnd == window) else {
            return;
        };
        if rest.preview && key == VK_ESCAPE as usize {
            self.close_rest();
            return;
        }
        if key == VK_RETURN as usize && !rest.composing {
            self.rest_command(3000, 0, rest.button);
            return;
        }
        if self.emergency && !rest.preview {
            let result = self.runtime.action(|engine| engine.emergency_key());
            match result {
                Ok(effect) => self.apply(effect),
                Err(error) => self.runtime.notice = Some(error),
            }
            for rest in &self.resting {
                self.validate_input(rest.hwnd);
            }
        }
    }
    pub(super) fn rest_command(&mut self, id: usize, notification: usize, sender: HWND) {
        if sender.is_null() || unsafe { IsWindow(sender) } == 0 {
            return;
        }
        let window = unsafe { GetParent(sender) };
        let Some(rest) = self.resting.iter().find(|rest| rest.hwnd == window) else {
            return;
        };
        match id {
            3001 if notification == EN_CHANGE as usize => {
                self.reveal_rest(window, true);
                self.validate_input(window);
            }
            3000 => {
                let value = text(rest.edit);
                if rest.composing || value != self.runtime.engine.data.settings.phrase {
                    self.validate_input(window);
                    return;
                }
                if rest.preview {
                    self.close_rest();
                    return;
                }
                let result = self.runtime.action(|engine| engine.unlock(&value));
                match result {
                    Ok(effect) => self.apply(effect),
                    Err(error) => self.runtime.notice = Some(error),
                }
            }
            3002 => {
                self.emergency = !self.emergency;
                for rest in &self.resting {
                    invalidate(rest.hwnd);
                }
            }
            3003 if rest.preview => self.close_rest(),
            _ => {}
        }
    }
    pub(super) fn paint_rest(&self, window: HWND, dc: HDC) {
        let Some(rest) = self.resting.iter().find(|rest| rest.hwnd == window) else {
            return;
        };
        let mut bounds = RECT::default();
        unsafe {
            GetClientRect(window, &mut bounds);
        }
        if let Some(background) = &self.background {
            background.paint(dc, bounds);
        } else {
            fill(dc, &bounds, rgb(18, 30, 35));
        }
        let dpi = scale(window);
        let heading = font(px(30, dpi), FW_NORMAL);
        let large = font(px(72, dpi), FW_NORMAL);
        let body = font(px(17, dpi), FW_NORMAL);
        let small = font(px(13, dpi), FW_NORMAL);
        let white = rgb(240, 247, 245);
        let muted = rgb(196, 215, 208);
        draw_text(
            dc,
            body,
            "LEAFSHIELD",
            rectangle(
                px(30, dpi),
                px(24, dpi),
                bounds.right - px(60, dpi),
                px(28, dpi),
            ),
            white,
            DT_SINGLELINE,
        );
        let title_y = px(110, dpi).min(bounds.bottom / 6);
        draw_text(
            dc,
            heading,
            &self.tr("休息一下"),
            rectangle(0, title_y, bounds.right, px(46, dpi)),
            white,
            DT_CENTER | DT_SINGLELINE,
        );
        let seconds = if rest.preview {
            self.runtime.engine.data.settings.rest_minutes as u64 * 60
        } else {
            self.runtime.engine.remaining_seconds()
        };
        draw_text(
            dc,
            large,
            &fmt_time(seconds),
            rectangle(0, title_y + px(42, dpi), bounds.right, px(100, dpi)),
            white,
            DT_CENTER | DT_SINGLELINE,
        );
        draw_text(
            dc,
            body,
            &self.tr("看看远处，放松双眼。"),
            rectangle(0, title_y + px(153, dpi), bounds.right, px(30, dpi)),
            muted,
            DT_CENTER | DT_SINGLELINE,
        );
        if rest.revealed {
            let input = input_bounds(window);
            draw_text(
                dc,
                small,
                &self.tr("需要提前回来？请手动输入以下文案"),
                rectangle(
                    input.left,
                    input.top - px(108, dpi),
                    input.right - input.left,
                    px(24, dpi),
                ),
                muted,
                DT_CENTER | DT_SINGLELINE,
            );
            draw_text(
                dc,
                body,
                &self.runtime.engine.data.settings.phrase,
                rectangle(
                    input.left,
                    input.top - px(73, dpi),
                    input.right - input.left,
                    px(65, dpi),
                ),
                white,
                DT_CENTER | DT_WORDBREAK,
            );
            let value = text(rest.edit);
            let mismatch = !rest.composing
                && !value.is_empty()
                && !self.runtime.engine.data.settings.phrase.starts_with(&value);
            if mismatch {
                fill(
                    dc,
                    &rectangle(
                        input.left,
                        input.bottom,
                        input.right - input.left,
                        px(2, dpi),
                    ),
                    rgb(242, 90, 90),
                );
            }
            let count = format!(
                "{} / {} {}{}",
                value.chars().count(),
                self.runtime.engine.data.settings.phrase.chars().count(),
                self.tr("字"),
                if mismatch {
                    format!(" · {}", self.tr("文案不匹配"))
                } else {
                    String::new()
                }
            );
            draw_text(
                dc,
                small,
                &count,
                rectangle(
                    input.left,
                    input.bottom + px(53, dpi),
                    input.right - input.left,
                    px(24, dpi),
                ),
                if mismatch { rgb(255, 151, 145) } else { muted },
                DT_CENTER | DT_SINGLELINE,
            );
            if self.emergency {
                let label = format!(
                    "{}  {} / 50",
                    self.tr(
                        "20 秒内手动敲击 50 次；间隔超过 2 秒重新计数，长按无效。解锁会记录日志。"
                    ),
                    self.runtime.engine.emergency_count()
                );
                draw_text(
                    dc,
                    small,
                    &label,
                    rectangle(
                        input.left,
                        input.bottom + px(86, dpi),
                        input.right - input.left,
                        px(65, dpi),
                    ),
                    muted,
                    DT_CENTER | DT_WORDBREAK,
                );
            }
        }
        draw_text(
            dc,
            small,
            &self.tr(if rest.preview {
                "预览不会锁定桌面或记录解锁"
            } else {
                "休息结束后自动返回"
            }),
            rectangle(
                px(24, dpi),
                bounds.bottom - px(38, dpi),
                bounds.right - px(48, dpi),
                px(26, dpi),
            ),
            muted,
            DT_CENTER | DT_SINGLELINE,
        );
        for font in [heading, large, body, small] {
            unsafe {
                DeleteObject(font);
            }
        }
    }
}
