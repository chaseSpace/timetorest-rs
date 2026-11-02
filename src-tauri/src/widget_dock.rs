use super::AppState;
use leafshield_core::{
    Phase,
    docking::{Dock, Edge, Rect},
};
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize};

pub const WIDTH: f64 = 176.0;
pub const HEIGHT: f64 = 54.0;

#[derive(Clone, Copy, Default, Serialize)]
pub struct View {
    pub edge: Option<Edge>,
    pub hidden: bool,
}
pub struct Controller {
    dock: Option<Dock>,
    hidden: bool,
    observed: Option<Rect>,
    moved_at: Instant,
    outside_since: Option<Instant>,
    transition_until: Instant,
}
impl Default for Controller {
    fn default() -> Self {
        Self {
            dock: None,
            hidden: false,
            observed: None,
            moved_at: Instant::now(),
            outside_since: None,
            transition_until: Instant::now(),
        }
    }
}
impl Controller {
    pub fn view(&self) -> View {
        View {
            edge: self.dock.map(|d| d.edge),
            hidden: self.hidden,
        }
    }
}
fn apply(
    window: &tauri::WebviewWindow,
    rect: Rect,
    view: View,
    click_through: bool,
) -> Result<(), String> {
    window
        .emit("widget-dock", view)
        .map_err(|e| e.to_string())?;
    window
        .set_size(PhysicalSize::new(rect.width, rect.height))
        .map_err(|e| e.to_string())?;
    window
        .set_position(PhysicalPosition::new(rect.x, rect.y))
        .map_err(|e| e.to_string())?;
    window
        .set_ignore_cursor_events(click_through && !view.hidden)
        .map_err(|e| e.to_string())
}
fn bounds(monitor: &tauri::Monitor) -> Rect {
    let area = monitor.work_area();
    Rect {
        x: area.position.x,
        y: area.position.y,
        width: area.size.width,
        height: area.size.height,
    }
}
fn visible(mut rect: Rect, work: Rect) -> Rect {
    rect.x = rect.x.clamp(
        work.x,
        (work.x + work.width as i32 - rect.width as i32).max(work.x),
    );
    rect.y = rect.y.clamp(
        work.y,
        (work.y + work.height as i32 - rect.height as i32).max(work.y),
    );
    rect
}
pub fn reset(app: &tauri::AppHandle, window: &tauri::WebviewWindow) -> Result<(), String> {
    let monitor = window
        .primary_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("未检测到主显示器")?;
    let scale = monitor.scale_factor();
    let work = bounds(&monitor);
    let rect = visible(
        Rect {
            x: work.x + work.width as i32 - (200.0 * scale) as i32,
            y: work.y + work.height as i32 - (82.0 * scale) as i32,
            width: (WIDTH * scale).round() as u32,
            height: (HEIGHT * scale).round() as u32,
        },
        work,
    );
    let state = app.state::<AppState>();
    *state.dock.lock().unwrap() = Controller::default();
    let click_through = state
        .runtime
        .lock()
        .unwrap()
        .engine
        .data
        .settings
        .click_through;
    apply(window, rect, View::default(), click_through)
}
pub fn begin_drag(app: &tauri::AppHandle, window: &tauri::WebviewWindow) -> Result<(), String> {
    let state = app.state::<AppState>();
    let full = {
        let mut controller = state.dock.lock().unwrap();
        let full = controller.dock.map(|d| d.full);
        *controller = Controller::default();
        full
    };
    let click_through = state
        .runtime
        .lock()
        .unwrap()
        .engine
        .data
        .settings
        .click_through;
    if let Some(full) = full {
        apply(window, full, View::default(), click_through)?;
    }
    window.start_dragging().map_err(|e| e.to_string())
}
fn button_down(_window: &tauri::WebviewWindow) -> bool {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON},
            WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON},
        };
        let key = if GetSystemMetrics(SM_SWAPBUTTON) == 0 {
            VK_LBUTTON
        } else {
            VK_RBUTTON
        };
        GetAsyncKeyState(key as i32) as u16 & 0x8000 != 0
    }
    #[cfg(target_os = "macos")]
    {
        #[link(name = "ApplicationServices", kind = "framework")]
        unsafe extern "C" {
            fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
        }
        unsafe { CGEventSourceButtonState(0, 0) }
    }
    #[cfg(target_os = "linux")]
    {
        use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
        if let (Ok(display), Ok(xlib)) = (_window.display_handle(), x11_dl::xlib::Xlib::open())
            && let RawDisplayHandle::Xlib(display) = display.as_raw()
            && let Some(display) = display.display
        {
            unsafe {
                let display = display.as_ptr().cast();
                let mut root = 0;
                let mut child = 0;
                let mut root_x = 0;
                let mut root_y = 0;
                let mut win_x = 0;
                let mut win_y = 0;
                let mut mask = 0;
                (xlib.XQueryPointer)(
                    display,
                    (xlib.XDefaultRootWindow)(display),
                    &mut root,
                    &mut child,
                    &mut root_x,
                    &mut root_y,
                    &mut win_x,
                    &mut win_y,
                    &mut mask,
                );
                return mask & x11_dl::xlib::Button1Mask != 0;
            }
        }
        false
    }
}
fn tick(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let (enabled, edge_hide, click_through) = {
        let rt = state.runtime.lock().unwrap();
        let s = &rt.engine.data.settings;
        (
            s.floating && rt.engine.phase != Phase::Resting,
            s.edge_hide,
            s.click_through,
        )
    };
    if !enabled {
        return Ok(());
    }
    let Some(window) = app.get_webview_window("widget") else {
        return Ok(());
    };
    if !window.is_visible().unwrap_or(false) {
        return Ok(());
    }
    let pointer = app.cursor_position().map_err(|e| e.to_string())?;
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.inner_size().map_err(|e| e.to_string())?;
    let Some(monitor) = window.current_monitor().map_err(|e| e.to_string())? else {
        return Ok(());
    };
    let work = bounds(&monitor);
    let scale = monitor.scale_factor();
    let frame = Rect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    let pressed = button_down(&window);
    let now = Instant::now();
    let update = {
        let mut controller = state.dock.lock().unwrap();
        if !edge_hide {
            let full = controller.dock.map(|d| d.full);
            *controller = Controller::default();
            full.map(|full| (visible(full, work), View::default()))
        } else if let Some(dock) = controller.dock {
            let expected = if controller.hidden {
                dock.collapsed()
            } else {
                dock.full
            };
            if frame != expected && now < controller.transition_until {
                // Native resize/move operations may complete after the dispatcher returns.
                None
            } else if frame != expected || work != dock.work {
                let full = visible(
                    Rect {
                        width: (WIDTH * scale).round() as u32,
                        height: (HEIGHT * scale).round() as u32,
                        ..frame
                    },
                    work,
                );
                *controller = Controller::default();
                Some((full, View::default()))
            } else {
                let hovered = work.contains(pointer.x, pointer.y)
                    && expected
                        .padded((2.0 * scale).ceil() as i32)
                        .contains(pointer.x, pointer.y);
                // A global button press may belong to another application.
                // Dragging the widget is explicitly started by begin_drag.
                if hovered {
                    controller.outside_since = None;
                    if controller.hidden {
                        controller.hidden = false;
                        Some((dock.full, controller.view()))
                    } else {
                        None
                    }
                } else if !controller.hidden
                    && now.duration_since(*controller.outside_since.get_or_insert(now))
                        >= Duration::from_millis(650)
                {
                    controller.hidden = true;
                    Some((dock.collapsed(), controller.view()))
                } else {
                    None
                }
            }
        } else {
            if controller.observed != Some(frame) || pressed {
                controller.observed = Some(frame);
                controller.moved_at = now;
            }
            if !pressed && now.duration_since(controller.moved_at) >= Duration::from_millis(350) {
                if let Some(dock) = Dock::snap(frame, work, scale) {
                    controller.dock = Some(dock);
                    controller.hidden = false;
                    controller.outside_since = None;
                    Some((dock.full, controller.view()))
                } else {
                    None
                }
            } else {
                None
            }
        }
    };
    if let Some((rect, view)) = update {
        state.dock.lock().unwrap().transition_until = now + Duration::from_millis(500);
        apply(&window, rect, view, click_through)?;
    }
    Ok(())
}
pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(120));
            let _ = tick(&app);
        }
    });
}
