use super::*;
use leafshield_core::docking::Rect;

fn bounds(window: HWND) -> Rect {
    let mut rect = RECT::default();
    unsafe {
        GetWindowRect(window, &mut rect);
    }
    Rect {
        x: rect.left,
        y: rect.top,
        width: (rect.right - rect.left).max(0) as u32,
        height: (rect.bottom - rect.top).max(0) as u32,
    }
}
fn position(window: HWND, rect: Rect) {
    unsafe {
        SetWindowPos(
            window,
            HWND_TOPMOST,
            rect.x,
            rect.y,
            rect.width as i32,
            rect.height as i32,
            SWP_NOACTIVATE,
        );
    }
    invalidate(window);
}
fn work_area(window: HWND) -> Rect {
    unsafe {
        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(monitor, &mut info);
        Rect {
            x: info.rcWork.left,
            y: info.rcWork.top,
            width: (info.rcWork.right - info.rcWork.left).max(0) as u32,
            height: (info.rcWork.bottom - info.rcWork.top).max(0) as u32,
        }
    }
}
impl App {
    pub(super) fn configure_widget(&mut self) -> Result<(), String> {
        if !self.runtime.engine.data.settings.floating {
            if !self.widget.is_null() {
                let old = self.widget;
                self.widget = null_mut();
                unsafe {
                    DestroyWindow(old);
                }
            }
            self.dock = None;
            self.collapsed = false;
            return Ok(());
        }
        if self.widget.is_null() {
            self.widget = create(
                WIDGET,
                "LeafShield Floating Timer",
                WS_POPUP,
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
                rectangle(80, 80, px(176, self.dpi), px(54, self.dpi)),
                self.host,
            )?;
            unsafe {
                SetLayeredWindowAttributes(self.widget, 0, 240, LWA_ALPHA);
            }
            self.reset_widget();
        }
        self.widget_style();
        unsafe {
            ShowWindow(
                self.widget,
                if self.runtime.engine.phase == Phase::Resting {
                    SW_HIDE
                } else {
                    SW_SHOWNOACTIVATE
                },
            );
        }
        Ok(())
    }
    fn widget_style(&self) {
        if self.widget.is_null() {
            return;
        }
        unsafe {
            let mut style = GetWindowLongPtrW(self.widget, GWL_EXSTYLE) as u32;
            if self.runtime.engine.data.settings.click_through && !self.collapsed {
                style |= WS_EX_TRANSPARENT;
            } else {
                style &= !WS_EX_TRANSPARENT;
            }
            SetWindowLongPtrW(self.widget, GWL_EXSTYLE, style as isize);
        }
    }
    pub(super) fn reset_widget(&mut self) {
        if self.widget.is_null() {
            return;
        }
        self.dock = None;
        self.collapsed = false;
        self.outside_since = None;
        let work = work_area(self.widget);
        let dpi = scale(self.widget);
        position(
            self.widget,
            Rect {
                x: work.x + work.width as i32 - px(196, dpi),
                y: work.y + work.height as i32 - px(76, dpi),
                width: px(176, dpi) as u32,
                height: px(54, dpi) as u32,
            },
        );
        self.widget_style();
    }
    pub(super) fn snap_widget(&mut self) {
        if self.widget.is_null() || !self.runtime.engine.data.settings.edge_hide {
            return;
        }
        self.dock = Dock::snap(
            bounds(self.widget),
            work_area(self.widget),
            scale(self.widget),
        );
        self.collapsed = false;
        self.outside_since = Some(Instant::now());
        if let Some(dock) = self.dock {
            position(self.widget, dock.full);
        }
        self.widget_style();
    }
    pub(super) fn poll_widget(&mut self) {
        if self.widget.is_null() || self.runtime.engine.phase == Phase::Resting {
            return;
        }
        if !unsafe { GetPropW(self.widget, wide("LeafShield.Dragging").as_ptr()) }.is_null() {
            return;
        }
        if !self.runtime.engine.data.settings.edge_hide {
            if self.collapsed
                && let Some(dock) = self.dock
            {
                position(self.widget, dock.full);
            }
            self.dock = None;
            self.collapsed = false;
            self.outside_since = None;
            self.widget_style();
            return;
        }
        let Some(dock) = self.dock else {
            return;
        };
        let mut point = POINT::default();
        unsafe {
            GetCursorPos(&mut point);
        }
        let current = if self.collapsed {
            dock.collapsed()
        } else {
            dock.full
        };
        if current
            .padded(if self.collapsed { 2 } else { 8 })
            .contains(point.x as f64, point.y as f64)
        {
            self.outside_since = None;
            if self.collapsed {
                self.collapsed = false;
                position(self.widget, dock.full);
                self.widget_style();
            }
        } else if self
            .outside_since
            .get_or_insert_with(Instant::now)
            .elapsed()
            >= Duration::from_millis(800)
            && !self.collapsed
        {
            self.collapsed = true;
            position(self.widget, dock.collapsed());
            self.widget_style();
        }
    }
    pub(super) fn paint_widget(&self, dc: HDC) {
        let mut bounds = RECT::default();
        unsafe {
            GetClientRect(self.widget, &mut bounds);
        }
        fill(
            dc,
            &bounds,
            if self.collapsed {
                rgb(35, 144, 177)
            } else {
                rgb(226, 243, 246)
            },
        );
        if self.collapsed {
            return;
        }
        let dpi = scale(self.widget);
        let small = font(px(11, dpi), FW_NORMAL);
        let timer_font = font(px(24, dpi), FW_SEMIBOLD);
        let label = self.tr(match self.runtime.engine.phase {
            Phase::Paused => "已暂停",
            Phase::Deferred => "休息已延后",
            _ => "专注中",
        });
        draw_text(
            dc,
            small,
            &label,
            rectangle(px(17, dpi), px(5, dpi), px(145, dpi), px(17, dpi)),
            rgb(60, 112, 126),
            DT_LEFT | DT_SINGLELINE,
        );
        draw_text(
            dc,
            timer_font,
            &fmt_time(self.runtime.engine.remaining_seconds()),
            rectangle(px(17, dpi), px(21, dpi), px(145, dpi), px(29, dpi)),
            rgb(18, 64, 77),
            DT_LEFT | DT_SINGLELINE,
        );
        draw_text(
            dc,
            timer_font,
            "›",
            rectangle(px(144, dpi), px(12, dpi), px(28, dpi), px(36, dpi)),
            rgb(35, 113, 135),
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        unsafe {
            DeleteObject(small);
            DeleteObject(timer_font);
        }
    }
}
