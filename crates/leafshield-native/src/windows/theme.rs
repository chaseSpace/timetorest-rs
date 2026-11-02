//! Small GDI controls with native keyboard, focus, and drag behavior.
use super::*;
use std::mem::size_of_val;

pub(super) const BACKGROUND: u32 = 0x00f3f3f3;
pub(super) const PANEL: u32 = 0x00ffffff;
pub(super) const TEXT: u32 = 0x001b1b1b;
pub(super) const MUTED: u32 = 0x00616161;
pub(super) const BORDER: u32 = 0x00e5e5e5;
pub(super) const ACCENT: u32 = 0x00c06700;

pub(super) fn rounded(dc: HDC, bounds: RECT, radius: i32, background: u32, border: u32) {
    unsafe {
        let old_brush = SelectObject(dc, GetStockObject(DC_BRUSH));
        let old_pen = SelectObject(dc, GetStockObject(DC_PEN));
        SetDCBrushColor(dc, background);
        SetDCPenColor(dc, border);
        RoundRect(
            dc,
            bounds.left,
            bounds.top,
            bounds.right,
            bounds.bottom,
            radius * 2,
            radius * 2,
        );
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
    }
}

pub(super) unsafe extern "system" fn button_proc(
    window: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        match message {
            WM_MOUSEMOVE => {
                if GetPropW(window, wide("LeafShield.Hover").as_ptr()).is_null() {
                    SetPropW(window, wide("LeafShield.Hover").as_ptr(), 1usize as HANDLE);
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: window,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut track);
                    invalidate(window);
                }
            }
            WM_MOUSELEAVE => {
                RemovePropW(window, wide("LeafShield.Hover").as_ptr());
                invalidate(window);
            }
            WM_NCDESTROY => {
                RemoveWindowSubclass(window, Some(button_proc), 2);
            }
            _ => {}
        }
        DefSubclassProc(window, message, w, l)
    }
}

pub(super) fn hovered(window: HWND) -> bool {
    !unsafe { GetPropW(window, wide("LeafShield.Hover").as_ptr()) }.is_null()
}

pub(super) fn paint_navigation(draw: &DRAWITEMSTRUCT) {
    let dpi = scale(draw.hwndItem);
    let active = unsafe { GetWindowLongPtrW(draw.hwndItem, GWLP_USERDATA) } != 0;
    fill(draw.hDC, &draw.rcItem, BACKGROUND);
    if active || hovered(draw.hwndItem) {
        let color = if active {
            rgb(229, 229, 229)
        } else {
            rgb(236, 236, 236)
        };
        rounded(draw.hDC, draw.rcItem, px(5, dpi), color, color);
    }
    if active {
        rounded(
            draw.hDC,
            rectangle(px(2, dpi), px(10, dpi), px(3, dpi), px(18, dpi)),
            px(1, dpi),
            ACCENT,
            ACCENT,
        );
    }
    let icon_font = unsafe {
        CreateFontW(
            -px(17, dpi),
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            wide("Segoe Fluent Icons").as_ptr(),
        )
    };
    let icon = [
        "\u{e7f4}", "\u{e823}", "\u{e72e}", "\u{e8fd}", "\u{e713}", "\u{e946}",
    ][(draw.CtlID - 100).min(5) as usize];
    draw_text(
        draw.hDC,
        icon_font,
        icon,
        rectangle(px(13, dpi), 0, px(24, dpi), px(38, dpi)),
        TEXT,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    let label_font = unsafe { SendMessageW(draw.hwndItem, WM_GETFONT, 0, 0) as HFONT };
    draw_text(
        draw.hDC,
        label_font,
        &text(draw.hwndItem),
        rectangle(px(42, dpi), 0, px(70, dpi), px(38, dpi)),
        TEXT,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
    unsafe {
        DeleteObject(icon_font);
    }
    if draw.itemState & ODS_FOCUS != 0 {
        unsafe {
            DrawFocusRect(draw.hDC, &draw.rcItem);
        }
    }
}

pub(super) fn paint_button(draw: &DRAWITEMSTRUCT) {
    let dpi = scale(draw.hwndItem);
    let enabled = draw.itemState & ODS_DISABLED == 0;
    let pressed = draw.itemState & ODS_SELECTED != 0;
    let accent = matches!(draw.CtlID, 12 | 1203 | 1503);
    let background = if !enabled {
        rgb(245, 245, 245)
    } else if accent {
        if pressed {
            rgb(0, 83, 156)
        } else if hovered(draw.hwndItem) {
            rgb(25, 117, 197)
        } else {
            ACCENT
        }
    } else if pressed {
        rgb(239, 239, 239)
    } else if hovered(draw.hwndItem) {
        rgb(247, 247, 247)
    } else {
        PANEL
    };
    fill(draw.hDC, &draw.rcItem, PANEL);
    let mut bounds = draw.rcItem;
    bounds.right -= 1;
    bounds.bottom -= 1;
    rounded(
        draw.hDC,
        bounds,
        px(5, dpi),
        background,
        if accent && enabled {
            background
        } else {
            rgb(207, 207, 207)
        },
    );
    let label_font = unsafe { SendMessageW(draw.hwndItem, WM_GETFONT, 0, 0) as HFONT };
    draw_text(
        draw.hDC,
        label_font,
        &text(draw.hwndItem),
        bounds,
        if !enabled {
            rgb(155, 155, 155)
        } else if accent {
            PANEL
        } else {
            TEXT
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    if draw.itemState & ODS_FOCUS != 0 {
        bounds.left += px(3, dpi);
        bounds.top += px(3, dpi);
        bounds.right -= px(3, dpi);
        bounds.bottom -= px(3, dpi);
        unsafe {
            DrawFocusRect(draw.hDC, &bounds);
        }
    }
}

fn thumb_rect(window: HWND) -> RECT {
    let mut bounds = RECT::default();
    unsafe {
        SendMessageW(window, TBM_GETTHUMBRECT, 0, &mut bounds as *mut _ as isize);
    }
    bounds
}

fn travel(window: HWND) -> (i32, i32, i32, i32) {
    unsafe {
        let minimum = SendMessageW(window, TBM_GETRANGEMIN, 0, 0) as i32;
        let maximum = SendMessageW(window, TBM_GETRANGEMAX, 0, 0) as i32;
        let mut channel = RECT::default();
        SendMessageW(
            window,
            TBM_GETCHANNELRECT,
            0,
            &mut channel as *mut _ as isize,
        );
        let thumb = thumb_rect(window);
        let width = (thumb.right - thumb.left).max(1);
        // GETTHUMBRECT is cached until a real position update. Sampling min/max
        // with redraw disabled returned the same rectangle, collapsing the rail.
        // The channel is stable and includes one thumb width at its two ends.
        (
            channel.left + width / 2,
            channel.right - (width - width / 2),
            minimum,
            maximum,
        )
    }
}

unsafe extern "system" fn slider_proc(
    window: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        match message {
            WM_LBUTTONDOWN => {
                SendMessageW(
                    window,
                    WM_CHANGEUISTATE,
                    UIS_SET as usize | ((UISF_HIDEFOCUS as usize) << 16),
                    0,
                );
                let x = l as u16 as i16 as i32;
                let thumb = thumb_rect(window);
                if x < thumb.left || x >= thumb.right {
                    let (start, end, minimum, maximum) = travel(window);
                    let span = i64::from(end - start);
                    if span > 0 {
                        let offset = i64::from((x - start).clamp(0, end - start));
                        let position = i64::from(minimum)
                            + (offset * i64::from(maximum - minimum) + span / 2) / span;
                        SendMessageW(window, TBM_SETPOS, 1, position as isize);
                        PostMessageW(
                            GetParent(window),
                            WM_HSCROLL,
                            TB_THUMBPOSITION as usize,
                            window as isize,
                        );
                    }
                }
                // Native hit testing now starts a normal thumb drag at the new location.
            }
            WM_ERASEBKGND => return 1,
            WM_PAINT | WM_PRINTCLIENT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = if message == WM_PAINT {
                    BeginPaint(window, &mut paint)
                } else {
                    w as HDC
                };
                let mut bounds = RECT::default();
                GetClientRect(window, &mut bounds);
                fill(dc, &bounds, PANEL);
                let dpi = scale(window);
                let thumb = thumb_rect(window);
                let center_x = (thumb.left + thumb.right) / 2;
                let center_y = (thumb.top + thumb.bottom) / 2;
                let (start, end, _, _) = travel(window);
                let line = rectangle(
                    start,
                    center_y - px(2, dpi),
                    (end - start).max(1),
                    px(4, dpi),
                );
                rounded(dc, line, px(2, dpi), rgb(145, 145, 145), rgb(145, 145, 145));
                if center_x > start {
                    rounded(
                        dc,
                        rectangle(start, line.top, center_x - start, line.bottom - line.top),
                        px(2, dpi),
                        ACCENT,
                        ACCENT,
                    );
                }
                let radius = px(9, dpi);
                rounded(
                    dc,
                    rectangle(
                        center_x - radius,
                        center_y - radius,
                        radius * 2 + 1,
                        radius * 2 + 1,
                    ),
                    radius,
                    PANEL,
                    rgb(180, 180, 180),
                );
                let inner = px(if GetCapture() == window { 5 } else { 4 }, dpi);
                rounded(
                    dc,
                    rectangle(
                        center_x - inner,
                        center_y - inner,
                        inner * 2 + 1,
                        inner * 2 + 1,
                    ),
                    inner,
                    ACCENT,
                    ACCENT,
                );
                if GetFocus() == window
                    && SendMessageW(window, WM_QUERYUISTATE, 0, 0) as u32 & UISF_HIDEFOCUS == 0
                {
                    let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
                    let old_pen = SelectObject(dc, GetStockObject(DC_PEN));
                    SetDCPenColor(dc, ACCENT);
                    let ring = radius + px(3, dpi);
                    Ellipse(
                        dc,
                        center_x - ring,
                        center_y - ring,
                        center_x + ring + 1,
                        center_y + ring + 1,
                    );
                    SelectObject(dc, old_pen);
                    SelectObject(dc, old_brush);
                }
                if message == WM_PAINT {
                    EndPaint(window, &paint);
                }
                return 0;
            }
            WM_NCDESTROY => {
                RemoveWindowSubclass(window, Some(slider_proc), 3);
            }
            WM_KEYDOWN => {
                SendMessageW(
                    window,
                    WM_CHANGEUISTATE,
                    UIS_CLEAR as usize | ((UISF_HIDEFOCUS as usize) << 16),
                    0,
                );
            }
            _ => {}
        }
        let result = DefSubclassProc(window, message, w, l);
        // Our circular thumb and focus ring extend beyond the native thumb's
        // invalidation rectangle. Repaint the whole small control after movement.
        if matches!(
            message,
            WM_LBUTTONDOWN
                | WM_LBUTTONUP
                | WM_KEYDOWN
                | WM_SETFOCUS
                | WM_KILLFOCUS
                | WM_CAPTURECHANGED
                | WM_UPDATEUISTATE
        ) || (message == WM_MOUSEMOVE && GetCapture() == window)
            || (message == TBM_SETPOS && w != 0)
        {
            invalidate(window);
        }
        result
    }
}

pub(super) fn style_slider(window: HWND) {
    unsafe {
        SetWindowSubclass(window, Some(slider_proc), 3, 0);
    }
}

pub(super) fn style_window(window: HWND) {
    use windows_sys::Win32::Graphics::Dwm::*;
    unsafe {
        // Unsupported attributes are ignored on older Windows versions.
        let corner = DWMWCP_ROUND;
        DwmSetWindowAttribute(
            window,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &corner as *const _ as *const _,
            size_of_val(&corner) as u32,
        );
        for (attribute, color) in [(DWMWA_CAPTION_COLOR, BACKGROUND), (DWMWA_TEXT_COLOR, TEXT)] {
            DwmSetWindowAttribute(
                window,
                attribute as u32,
                &color as *const _ as *const _,
                size_of_val(&color) as u32,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_slider_channel_geometry() {
        unsafe {
            let setup = INITCOMMONCONTROLSEX {
                dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_BAR_CLASSES,
            };
            InitCommonControlsEx(&setup);
            let parent = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("slider-test").as_ptr(),
                WS_POPUP,
                0,
                0,
                700,
                100,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!parent.is_null());
            let slider = child(
                parent,
                "msctls_trackbar32",
                "",
                1001,
                TBS_NOTICKS,
                rectangle(0, 0, 500, 30),
                GetStockObject(DEFAULT_GUI_FONT),
            );
            SendMessageW(slider, TBM_SETRANGEMIN, 0, 0);
            SendMessageW(slider, TBM_SETRANGEMAX, 0, 5000);
            let mut channel = RECT::default();
            SendMessageW(
                slider,
                TBM_GETCHANNELRECT,
                0,
                &mut channel as *mut _ as isize,
            );
            SendMessageW(slider, TBM_SETPOS, 1, 0);
            let start = thumb_rect(slider);
            SendMessageW(slider, TBM_SETPOS, 1, 5000);
            let end = thumb_rect(slider);
            let geometry = travel(slider);
            assert_eq!(geometry.0, (start.left + start.right) / 2);
            assert_eq!(geometry.1, (end.left + end.right) / 2);
            assert!(geometry.1 - geometry.0 > 400);
            assert_eq!(SendMessageW(slider, TBM_GETPOS, 0, 0), 5000);
            DestroyWindow(parent);
            assert!(end.left > start.left);
        }
    }

    #[test]
    fn synchronous_control_repaint_works_while_application_state_is_borrowed() {
        // A memory DC reproduces the SetWindowText callback without opening UI.
        struct MemoryDc(HDC);
        impl Drop for MemoryDc {
            fn drop(&mut self) {
                unsafe {
                    DeleteDC(self.0);
                }
            }
        }
        let dc = MemoryDc(unsafe { CreateCompatibleDC(null_mut()) });
        assert!(!dc.0.is_null());
        APP.with(|cell| {
            let _active_update = cell.borrow_mut();
            let brush = unsafe { window_proc(null_mut(), WM_CTLCOLORSTATIC, dc.0 as usize, 0) };
            assert_eq!(brush, unsafe { GetStockObject(DC_BRUSH) } as isize);
            assert_eq!(unsafe { GetBkColor(dc.0) }, PANEL);
            assert_eq!(unsafe { GetDCBrushColor(dc.0) }, PANEL);
            assert_eq!(unsafe { GetTextColor(dc.0) }, TEXT);
        });
    }
}
