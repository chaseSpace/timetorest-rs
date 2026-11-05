use super::*;

const WIDTH: i32 = 700;
const HEIGHT: i32 = 450;
const TEMPERATURE: usize = 1001;
const BRIGHTNESS: usize = 1002;
const DISPLAY: usize = 1003;
const WORK: usize = 1201;
const REST_MINUTES: usize = 1202;
const SAVE_TIME: usize = 1203;
const PHRASE: usize = 1401;
const THEME: usize = 1402;
const IMAGE: usize = 1403;
const WHITELIST: usize = 1501;
const PICK_PROGRAMS: usize = 1502;
const SAVE_WHITELIST: usize = 1503;
const FLOATING: usize = 1601;
const THROUGH: usize = 1602;
const EDGE_HIDE: usize = 1603;
const NOTIFICATIONS: usize = 1604;
const EMERGENCY: usize = 1605;
const STARTUP: usize = 1606;
const LANGUAGE: usize = 1607;
const DATA_DIRECTORY: usize = 1608;
const HOMEPAGE: usize = 1701;

pub(super) enum PickerResult {
    Programs(Result<Vec<String>, String>),
    Image(Result<Option<std::path::PathBuf>, String>),
}

impl App {
    fn item(
        &self,
        class: &str,
        value: &str,
        id: usize,
        style: u32,
        bounds: (i32, i32, i32, i32),
    ) -> HWND {
        let (mut x, mut y, mut width, mut height) = bounds;
        if id == 900 {
            (x, y, width, height) = (144, 414, 532, 26);
        } else if !(100..106).contains(&id) {
            x += 28;
            y += 48;
        }
        let window = child(
            self.main,
            class,
            &self.tr(value),
            id,
            style,
            rectangle(
                px(x, self.dpi),
                px(y, self.dpi),
                px(width, self.dpi),
                px(height, self.dpi),
            ),
            self.font,
        );
        if class == "BUTTON" && style & 0xf == BS_OWNERDRAW as u32 {
            unsafe {
                SetWindowSubclass(window, Some(theme::button_proc), 2, 0);
            }
        }
        window
    }
    fn label(&self, value: &str, id: usize, bounds: (i32, i32, i32, i32)) -> HWND {
        self.item("STATIC", value, id, 0, bounds)
    }
    fn button(&self, value: &str, id: usize, bounds: (i32, i32, i32, i32)) -> HWND {
        self.item(
            "BUTTON",
            value,
            id,
            WS_TABSTOP | BS_OWNERDRAW as u32,
            bounds,
        )
    }
    fn checkbox(&self, value: &str, id: usize, enabled: bool, bounds: (i32, i32, i32, i32)) {
        self.item(
            "BUTTON",
            value,
            id,
            WS_TABSTOP | BS_AUTOCHECKBOX as u32,
            bounds,
        );
        check(self.main, id, enabled);
    }
    fn combo(&self, id: usize, values: &[String], current: usize, bounds: (i32, i32, i32, i32)) {
        let window = self.item(
            "COMBOBOX",
            "",
            id,
            WS_TABSTOP | CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
            bounds,
        );
        unsafe {
            for value in values {
                SendMessageW(window, CB_ADDSTRING, 0, wide(value).as_ptr() as isize);
            }
            SendMessageW(window, CB_SETCURSEL, current, 0);
            // A closed dropdown has no scroll bar; its popup scrolls only when needed.
            SendMessageW(window, CB_SETDROPPEDWIDTH, px(475, self.dpi) as usize, 0);
        }
    }
    pub(super) fn show_main(&mut self) -> Result<(), String> {
        if self.runtime.engine.phase == Phase::Resting {
            return Ok(());
        }
        if self.main.is_null() {
            let bounds = rectangle(0, 0, px(WIDTH, self.dpi), px(HEIGHT, self.dpi));
            let mut adjusted = bounds;
            unsafe {
                AdjustWindowRectExForDpi(
                    &mut adjusted,
                    WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
                    0,
                    WS_EX_APPWINDOW,
                    (self.dpi * 96.0) as u32,
                );
            }
            let width = adjusted.right - adjusted.left;
            let height = adjusted.bottom - adjusted.top;
            let x = (unsafe { GetSystemMetrics(SM_CXSCREEN) } - width) / 2;
            let y = (unsafe { GetSystemMetrics(SM_CYSCREEN) } - height) / 2;
            self.main = create(
                MAIN,
                "LeafShield",
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
                WS_EX_APPWINDOW,
                rectangle(x, y, width, height),
                self.host,
            )?;
            self.dpi = scale(self.main);
            theme::style_window(self.main);
            unsafe {
                SendMessageW(
                    self.main,
                    WM_SETICON,
                    ICON_SMALL as usize,
                    self.icon as isize,
                );
                SendMessageW(self.main, WM_SETICON, ICON_BIG as usize, self.icon as isize);
            }
            self.build_page();
        }
        unsafe {
            ShowWindow(self.main, SW_RESTORE);
            SetForegroundWindow(self.main);
        }
        invalidate(self.main);
        Ok(())
    }
    pub(super) fn rebuild_main(&mut self) {
        self.dpi = scale(self.main);
        unsafe {
            for item in [self.font, self.heading, self.large] {
                DeleteObject(item);
            }
        }
        self.font = font(px(14, self.dpi), FW_NORMAL);
        self.heading = font(px(22, self.dpi), FW_SEMIBOLD);
        self.large = font(px(64, self.dpi), FW_NORMAL);
        self.build_page();
    }
    pub(super) fn close_window(&mut self, window: HWND) {
        if window == self.main {
            self.main = null_mut();
            unsafe {
                DestroyWindow(window);
            }
        } else if self
            .resting
            .iter()
            .any(|rest| rest.hwnd == window && rest.preview)
        {
            self.close_rest();
        }
    }
    pub(super) fn build_page(&mut self) {
        if self.main.is_null() {
            return;
        }
        unsafe extern "system" fn collect(window: HWND, data: LPARAM) -> i32 {
            unsafe {
                (*(data as *mut Vec<HWND>)).push(window);
            }
            1
        }
        unsafe {
            SendMessageW(self.main, WM_SETREDRAW, 0, 0);
            let mut children: Vec<HWND> = Vec::new();
            EnumChildWindows(self.main, Some(collect), &mut children as *mut _ as isize);
            for child in children {
                DestroyWindow(child);
            }
        }
        for (index, label) in ["显示器", "定时器", "解锁", "白名单", "选项", "关于"]
            .iter()
            .enumerate()
        {
            let button = self.item(
                "BUTTON",
                label,
                100 + index,
                WS_TABSTOP | BS_OWNERDRAW as u32,
                (12, 14 + index as i32 * 44, 112, 38),
            );
            unsafe {
                SetWindowLongPtrW(button, GWLP_USERDATA, isize::from(self.page == index));
            }
        }
        let settings = self.runtime.engine.data.settings.clone();
        match self.page {
            0 => {
                self.label("更温暖", 1100, (118, 20, 135, 22));
                self.label(
                    &format!("{} K", settings.temperature_kelvin),
                    1101,
                    (315, 20, 115, 22),
                );
                self.label("不太温暖", 1102, (505, 20, 115, 22));
                let slider = self.item(
                    "msctls_trackbar32",
                    "",
                    TEMPERATURE,
                    WS_TABSTOP | TBS_HORZ | TBS_NOTICKS,
                    (116, 48, 500, 30),
                );
                unsafe {
                    SendMessageW(slider, TBM_SETRANGEMIN, 0, 0);
                    SendMessageW(slider, TBM_SETRANGEMAX, 0, 5000);
                    SendMessageW(
                        slider,
                        TBM_SETPOS,
                        1,
                        (settings.temperature_kelvin - 1500) as isize,
                    );
                    SendMessageW(slider, TBM_SETLINESIZE, 0, 100);
                }
                theme::style_slider(slider);
                self.label("不太明亮", 1103, (118, 94, 140, 22));
                self.label(
                    &format!("{}%", settings.brightness_percent),
                    1104,
                    (315, 94, 115, 22),
                );
                self.label("更明亮", 1105, (505, 94, 115, 22));
                let slider = self.item(
                    "msctls_trackbar32",
                    "",
                    BRIGHTNESS,
                    WS_TABSTOP | TBS_HORZ | TBS_NOTICKS,
                    (116, 122, 500, 30),
                );
                unsafe {
                    SendMessageW(slider, TBM_SETRANGEMIN, 0, 20);
                    SendMessageW(slider, TBM_SETRANGEMAX, 0, 100);
                    SendMessageW(slider, TBM_SETPOS, 1, settings.brightness_percent as isize);
                }
                theme::style_slider(slider);
                self.checkbox(
                    "开启屏幕调节",
                    DISPLAY,
                    settings.display_enabled,
                    (124, 174, 350, 24),
                );
                for (index, label) in ["正常", "办公", "夜晚", "阅读"].iter().enumerate() {
                    self.button(
                        label,
                        1110 + index,
                        (124 + index as i32 * 120, 222, 108, 29),
                    );
                }
                self.label(
                    "色温与亮度会应用到全部显示器，并同步常用系统鼠标指针",
                    1120,
                    (124, 279, 480, 45),
                );
            }
            1 => {
                let timer = self.label("", 1210, (126, 10, 300, 73));
                unsafe {
                    SendMessageW(timer, WM_SETFONT, self.large as usize, 1);
                }
                self.label("工作（分钟）", 1211, (126, 95, 150, 22));
                self.label("休息（分钟）", 1212, (320, 95, 150, 22));
                self.item(
                    "EDIT",
                    &settings.work_minutes.to_string(),
                    WORK,
                    WS_TABSTOP | WS_BORDER | ES_NUMBER as u32 | ES_AUTOHSCROLL as u32,
                    (126, 122, 120, 26),
                );
                self.item(
                    "EDIT",
                    &settings.rest_minutes.to_string(),
                    REST_MINUTES,
                    WS_TABSTOP | WS_BORDER | ES_NUMBER as u32 | ES_AUTOHSCROLL as u32,
                    (320, 122, 120, 26),
                );
                self.button("保存", SAVE_TIME, (466, 120, 130, 29));
                for (index, (work, rest)) in [(25, 2), (45, 2), (60, 3), (90, 5)].iter().enumerate()
                {
                    self.button(
                        &format!("{work} / {rest}"),
                        1220 + index,
                        (126 + index as i32 * 120, 170, 108, 29),
                    );
                }
                self.button("", ID_PAUSE, (126, 232, 145, 32));
                self.button("立即休息", ID_REST, (286, 232, 145, 32));
                self.button("预览锁屏", ID_PREVIEW, (446, 232, 150, 32));
                self.label(
                    "工作结束后自动进入休息，休息结束后继续计时",
                    1213,
                    (126, 289, 480, 45),
                );
            }
            2 => {
                self.label("解锁文案", 1410, (124, 15, 480, 22));
                let phrases = leafshield_core::locale::phrases(settings.language);
                let labels: Vec<_> = phrases
                    .iter()
                    .map(|s| {
                        let mut label: String = s.chars().take(38).collect();
                        if s.chars().count() > 38 {
                            label.push('…');
                        }
                        label
                    })
                    .collect();
                let index = leafshield_core::locale::phrase_index(&settings.phrase).unwrap_or(0);
                self.combo(PHRASE, &labels, index, (124, 45, 480, 180));
                self.label(&settings.phrase, 1411, (124, 86, 480, 58));
                self.checkbox(
                    "开启紧急解锁",
                    EMERGENCY,
                    settings.emergency_enabled,
                    (124, 153, 480, 24),
                );
                self.label("锁屏时点“突发紧急情况？”，20 秒内按键 50 次解锁。间隔超过 2 秒或超时会重置；长按不会连续计数，解锁会记录日志。", 1414, (124, 180, 480, 60));
                self.label("休息背景", 1412, (124, 247, 160, 22));
                let themes: Vec<_> = ["林间", "远山", "夜空", "自定义"]
                    .iter()
                    .map(|v| self.tr(v))
                    .collect();
                let index = ["forest", "mountain", "night", "custom"]
                    .iter()
                    .position(|v| *v == settings.theme)
                    .unwrap_or(0);
                self.combo(THEME, &themes, index, (124, 276, 205, 145));
                self.button("选择图片", IMAGE, (345, 275, 120, 29));
                self.button("预览锁屏", ID_PREVIEW, (480, 275, 125, 29));
                self.label(
                    "支持 JPG、PNG、WebP，最大 5 MB；图片会自动压缩",
                    1413,
                    (124, 310, 480, 36),
                );
            }
            3 => {
                self.label("前台程序白名单", 1510, (124, 18, 480, 24));
                self.label(
                    "这些程序位于前台时，自动休息会延后；最多 5 个",
                    1511,
                    (124, 52, 480, 38),
                );
                let edit = self.item(
                    "EDIT",
                    &settings.whitelist.join("\r\n"),
                    WHITELIST,
                    WS_TABSTOP | WS_BORDER | ES_MULTILINE as u32 | ES_WANTRETURN as u32,
                    (124, 104, 480, 128),
                );
                unsafe {
                    SendMessageW(edit, EM_SETLIMITTEXT, 500, 0);
                }
                self.button("选择 EXE / 快捷方式", PICK_PROGRAMS, (124, 252, 300, 31));
                self.button("保存", SAVE_WHITELIST, (440, 252, 164, 31));
                self.label(
                    "也可每行填写一个进程文件名，例如 Teams.exe",
                    1512,
                    (124, 306, 480, 35),
                );
            }
            4 => {
                self.checkbox(
                    "显示悬浮倒计时",
                    FLOATING,
                    settings.floating,
                    (124, 18, 470, 25),
                );
                self.checkbox(
                    "悬浮窗鼠标穿透",
                    THROUGH,
                    settings.click_through,
                    (124, 54, 470, 25),
                );
                self.checkbox(
                    "贴边吸附后自动隐藏",
                    EDGE_HIDE,
                    settings.edge_hide,
                    (124, 90, 470, 25),
                );
                self.checkbox(
                    "休息前通知提醒",
                    NOTIFICATIONS,
                    settings.notifications,
                    (124, 126, 470, 25),
                );
                self.checkbox(
                    "开机启动",
                    STARTUP,
                    startup::enabled(&self.runtime.directory).unwrap_or(false),
                    (124, 162, 470, 25),
                );
                self.label("界面语言", 1610, (124, 211, 160, 25));
                self.combo(
                    LANGUAGE,
                    &["简体中文".into(), "English".into()],
                    usize::from(settings.language == Language::English),
                    (300, 208, 235, 100),
                );
                self.button("打开数据目录", DATA_DIRECTORY, (124, 267, 240, 31));
                self.button("重置悬浮窗位置", ID_RESET_WIDGET, (380, 267, 225, 31));
            }
            _ => {
                let title = self.label("一叶 · LeafShield", 1710, (126, 24, 475, 35));
                unsafe {
                    SendMessageW(title, WM_SETFONT, self.heading as usize, 1);
                }
                self.label(env!("CARGO_PKG_VERSION"), 1711, (126, 72, 475, 25));
                self.label("让休息自然发生", 1712, (126, 110, 475, 26));
                self.label("作者：Leigg（磊哥哥）", 1713, (126, 158, 475, 25));
                self.label("开源许可：MIT", 1714, (126, 194, 475, 25));
                self.label("UI 与部分功能参考 CareUEyes。", 1715, (126, 232, 475, 45));
                self.button("项目主页", HOMEPAGE, (126, 293, 230, 31));
            }
        }
        self.label("", 900, (112, 359, 515, 25));
        self.refresh_dynamic();
        unsafe {
            SendMessageW(self.main, WM_SETREDRAW, 1, 0);
            RedrawWindow(
                self.main,
                null(),
                null_mut(),
                RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_ERASE,
            );
        }
    }
    pub(super) fn refresh_dynamic(&self) {
        self.update_status();
        if self.main.is_null() {
            return;
        }
        if self.page == 1 {
            set_text(
                control(self.main, 1210),
                &fmt_time(self.runtime.engine.remaining_seconds()),
            );
            set_text(
                control(self.main, ID_PAUSE),
                &self.tr(if self.runtime.engine.phase == Phase::Paused {
                    "继续计时"
                } else {
                    "暂停计时"
                }),
            );
        }
    }
    pub(super) fn paint_main(&self, dc: HDC) {
        let mut bounds = RECT::default();
        unsafe {
            GetClientRect(self.main, &mut bounds);
        }
        fill(dc, &bounds, theme::BACKGROUND);
        theme::rounded(
            dc,
            rectangle(
                px(136, self.dpi),
                px(54, self.dpi),
                px(540, self.dpi),
                px(346, self.dpi),
            ),
            px(8, self.dpi),
            theme::PANEL,
            theme::BORDER,
        );
        let title =
            self.tr(["显示器", "定时器", "解锁", "白名单", "选项", "关于"][self.page.min(5)]);
        draw_text(
            dc,
            self.heading,
            &title,
            rectangle(
                px(152, self.dpi),
                px(13, self.dpi),
                px(520, self.dpi),
                px(32, self.dpi),
            ),
            theme::TEXT,
            DT_SINGLELINE | DT_VCENTER,
        );
        draw_text(
            dc,
            self.font,
            env!("CARGO_PKG_VERSION"),
            rectangle(
                px(18, self.dpi),
                px(416, self.dpi),
                px(110, self.dpi),
                px(22, self.dpi),
            ),
            theme::MUTED,
            DT_SINGLELINE,
        );
    }
    pub(super) fn slider(&mut self, packed: usize, sender: HWND) {
        if self.runtime.engine.phase == Phase::Resting
            || self.page != 0
            || self.main.is_null()
            || unsafe { GetParent(sender) } != self.main
        {
            return;
        }
        let temperature =
            unsafe { SendMessageW(control(self.main, TEMPERATURE), TBM_GETPOS, 0, 0) };
        let brightness = unsafe { SendMessageW(control(self.main, BRIGHTNESS), TBM_GETPOS, 0, 0) };
        let mut settings = self.runtime.engine.data.settings.clone();
        settings.temperature_kelvin = (1500 + temperature).clamp(1500, 6500) as u32;
        settings.brightness_percent = brightness.clamp(20, 100) as u32;
        if !self.ui_testing {
            display::target(
                settings.temperature_kelvin,
                settings.brightness_percent,
                settings.display_enabled,
            );
        }
        set_text(
            control(self.main, 1101),
            &format!("{} K", settings.temperature_kelvin),
        );
        set_text(
            control(self.main, 1104),
            &format!("{}%", settings.brightness_percent),
        );
        // Preserve smooth 180 ms transitions without writing the configuration for every thumb movement.
        if packed as u16 as u32 != TB_THUMBTRACK {
            let result = self.update_settings(settings);
            self.notice(result);
        }
    }
    pub(super) fn settings_command(
        &mut self,
        id: usize,
        notification: usize,
    ) -> Result<(), String> {
        let mut settings = self.runtime.engine.data.settings.clone();
        match id {
            DISPLAY => settings.display_enabled = checked(self.main, DISPLAY),
            1110..=1113 => {
                let (temperature, brightness) =
                    [(6500, 100), (5000, 80), (3500, 65), (4500, 85)][id - 1110];
                settings.temperature_kelvin = temperature;
                settings.brightness_percent = brightness;
                settings.display_enabled = true;
            }
            SAVE_TIME => {
                settings.work_minutes = text(control(self.main, WORK))
                    .parse()
                    .map_err(|_| self.tr("工作时长应为 1–240 分钟"))?;
                settings.rest_minutes = text(control(self.main, REST_MINUTES))
                    .parse()
                    .map_err(|_| self.tr("休息时长应为 1–60 分钟"))?;
            }
            1220..=1223 => {
                let (work, rest) = [(25, 2), (45, 2), (60, 3), (90, 5)][id - 1220];
                settings.work_minutes = work;
                settings.rest_minutes = rest;
            }
            PHRASE if notification == CBN_SELCHANGE as usize => {
                settings.phrase = leafshield_core::locale::phrases(settings.language)
                    [selected(self.main, PHRASE).min(4)]
                .clone();
            }
            THEME if notification == CBN_SELCHANGE as usize => {
                settings.theme = ["forest", "mountain", "night", "custom"]
                    [selected(self.main, THEME).min(3)]
                .into()
            }
            IMAGE => {
                self.start_picker(false)?;
                return Ok(());
            }
            PICK_PROGRAMS => {
                self.start_picker(true)?;
                return Ok(());
            }
            SAVE_WHITELIST => settings.whitelist = whitelist(&text(control(self.main, WHITELIST))),
            FLOATING => settings.floating = checked(self.main, FLOATING),
            THROUGH => settings.click_through = checked(self.main, THROUGH),
            EDGE_HIDE => settings.edge_hide = checked(self.main, EDGE_HIDE),
            NOTIFICATIONS => settings.notifications = checked(self.main, NOTIFICATIONS),
            EMERGENCY => settings.emergency_enabled = checked(self.main, EMERGENCY),
            STARTUP => {
                if self.ui_testing {
                    return std::fs::write(
                        self.runtime.directory.join("startup.json"),
                        if checked(self.main, STARTUP) {
                            "true"
                        } else {
                            "false"
                        },
                    )
                    .map_err(|error| error.to_string());
                }
                return startup::configure(&self.runtime.directory, checked(self.main, STARTUP));
            }
            LANGUAGE if notification == CBN_SELCHANGE as usize => {
                settings.language = if selected(self.main, LANGUAGE) == 0 {
                    Language::Chinese
                } else {
                    Language::English
                }
            }
            DATA_DIRECTORY => {
                self.open_path(&self.runtime.directory.to_string_lossy());
                return Ok(());
            }
            HOMEPAGE => {
                self.open_path("https://github.com/chaseSpace/timetorest-rs");
                return Ok(());
            }
            _ => return Ok(()),
        }
        if let Err(error) = self.update_settings(settings) {
            self.build_page();
            return Err(error);
        }
        self.build_page();
        set_text(self.main, "LeafShield");
        Ok(())
    }
    fn open_path(&self, path: &str) {
        unsafe {
            ShellExecuteW(
                self.main,
                wide("open").as_ptr(),
                wide(path).as_ptr(),
                null(),
                null(),
                SW_SHOWNORMAL,
            );
        }
    }
    fn start_picker(&mut self, programs: bool) -> Result<(), String> {
        if self.pending.is_some() {
            return Err("程序选择窗口已打开".into());
        }
        let owner = self.main as usize;
        let language = self.runtime.engine.data.settings.language;
        let mut existing = whitelist(&text(control(self.main, WHITELIST)));
        let (sender, receiver) = std::sync::mpsc::channel();
        self.pending = Some(receiver);
        std::thread::spawn(move || {
            let result = if programs {
                PickerResult::Programs(picker::pick(owner, language).map(|names| {
                    if names.is_empty() {
                        return Vec::new();
                    }
                    for name in names {
                        if !existing
                            .iter()
                            .any(|value| value.eq_ignore_ascii_case(&name))
                        {
                            existing.push(name);
                        }
                    }
                    existing
                }))
            } else {
                PickerResult::Image(pick_image(owner, language))
            };
            let _ = sender.send(result);
        });
        Ok(())
    }
    pub(super) fn poll_picker(&mut self) {
        let Some(pending) = self.pending.as_ref() else {
            return;
        };
        let result = match pending.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(_) => {
                self.pending = None;
                self.runtime.notice = Some("程序选择窗口已关闭".into());
                return;
            }
        };
        self.pending = None;
        let result = if self.runtime.engine.phase == Phase::Resting {
            Err("休息结束后可以修改设置".into())
        } else {
            match result {
                PickerResult::Programs(result) => result.and_then(|names| {
                    if names.is_empty() {
                        return Ok(());
                    }
                    let mut settings = self.runtime.engine.data.settings.clone();
                    settings.whitelist = names;
                    self.update_settings(settings)
                }),
                PickerResult::Image(result) => result.and_then(|file| {
                    if let Some(file) = file {
                        self.runtime.import_wallpaper(&file)
                    } else {
                        Ok(())
                    }
                }),
            }
        };
        self.notice(result);
        self.build_page();
    }
}

fn pick_image(owner: usize, language: Language) -> Result<Option<std::path::PathBuf>, String> {
    unsafe {
        let mut buffer = vec![0u16; 32768];
        let filter = wide("Images (*.jpg;*.jpeg;*.png;*.webp)\0*.jpg;*.jpeg;*.png;*.webp\0\0");
        let title = wide(&tr(language, "选择图片"));
        let mut dialog: OPENFILENAMEW = zeroed();
        dialog.lStructSize = size_of::<OPENFILENAMEW>() as u32;
        dialog.hwndOwner = owner as HWND;
        dialog.lpstrFile = buffer.as_mut_ptr();
        dialog.nMaxFile = buffer.len() as u32;
        dialog.lpstrFilter = filter.as_ptr();
        dialog.lpstrTitle = title.as_ptr();
        dialog.Flags =
            OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_DONTADDTORECENT;
        if GetOpenFileNameW(&mut dialog) == 0 {
            let error = CommDlgExtendedError();
            return if error == 0 {
                Ok(None)
            } else {
                Err(format!(
                    "{} ({error})",
                    tr(language, "无法打开图片选择窗口")
                ))
            };
        }
        let length = buffer.iter().position(|v| *v == 0).unwrap_or(0);
        Ok(Some(std::path::PathBuf::from(String::from_utf16_lossy(
            &buffer[..length],
        ))))
    }
}

fn whitelist(value: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for item in value
        .split(['\r', '\n', ',', ';'])
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        if !names.iter().any(|v| v.eq_ignore_ascii_case(item)) {
            names.push(item.to_owned());
        }
    }
    names
}
