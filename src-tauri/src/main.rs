#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod display;
mod platform;
mod program_picker;
mod startup;
mod wallpapers;
mod widget_dock;

use leafshield_core::locale::tr;
use leafshield_core::{Effect, Engine, Persisted, Phase, Settings, Snapshot};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_notification::NotificationExt;

struct Runtime {
    engine: Engine,
    last_tick: Instant,
    notice: Option<String>,
}
struct AppState {
    runtime: Mutex<Runtime>,
    file: PathBuf,
    smoke: bool,
    smoke_ready: std::sync::atomic::AtomicBool,
    smoke_title: Mutex<Option<String>>,
    dock: Mutex<widget_dock::Controller>,
}
fn localize(app: &tauri::AppHandle, text: &str) -> String {
    let language = app
        .state::<AppState>()
        .runtime
        .lock()
        .unwrap()
        .engine
        .data
        .settings
        .language;
    tr(language, text)
}
fn sync_language(app: &tauri::AppHandle) {
    if let Some(tray) = app.tray_by_id("leafshield")
        && let Ok(menu) = tray_menu(app)
    {
        let _ = tray.set_menu(Some(menu));
    }
    for (label, window) in app.webview_windows() {
        let title = match label.as_str() {
            "main" => "一叶 · LeafShield",
            "widget" => "一叶 · 悬浮计时",
            "preview" => "休息预览 · 一叶",
            "warning" => "休息提醒",
            _ if label.starts_with("rest-") => "休息时间 · 一叶",
            _ => continue,
        };
        let _ = window.set_title(&localize(app, title));
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewState {
    #[serde(flatten)]
    timer: Snapshot,
    keyboard_protection: bool,
    platform: String,
    notice: Option<String>,
    data_path: String,
    smoke_mode: bool,
    display_status: display::Status,
}
fn view(app: &tauri::AppHandle) -> ViewState {
    let state = app.state::<AppState>();
    let rt = state.runtime.lock().unwrap();
    ViewState {
        timer: rt.engine.snapshot(),
        keyboard_protection: platform::ready(),
        platform: std::env::consts::OS.into(),
        notice: rt.notice.clone(),
        data_path: state.file.display().to_string(),
        smoke_mode: state.smoke,
        display_status: display::status(),
    }
}
fn save(path: &Path, data: &Persisted) -> Result<(), String> {
    use std::io::Write;
    let parent = path.parent().ok_or("配置路径无效")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("tmp");
    let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(data).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    // Readers such as antivirus/indexers can briefly deny replacement on Windows.
    // Keep the previous file intact and retry the final atomic rename only.
    for attempt in 0..10 {
        match std::fs::rename(&temporary, path) {
            Ok(()) => return Ok(()),
            Err(error)
                if cfg!(windows)
                    && attempt < 9
                    && matches!(error.raw_os_error(), Some(5 | 32 | 33)) =>
            {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    unreachable!()
}
#[cfg(all(test, windows))]
mod persistence_tests {
    use super::*;
    #[test]
    fn atomic_settings_replace_survives_a_brief_windows_read_lock() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = std::env::temp_dir().join(format!(
            "leafshield-save-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("settings.json");
        let mut data = Persisted::default();
        save(&path, &data).unwrap();
        let reader = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(70));
            drop(reader);
        });
        data.settings.temperature_kelvin = 4300;
        assert!(save(&path, &data).is_ok());
        release.join().unwrap();
        let actual: Persisted = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(actual.settings.temperature_kelvin, 4300);
        assert!(dir.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
fn changed(app: &tauri::AppHandle) {
    let _ = app.emit("timer-update", view(app));
}
fn mutate(
    app: &tauri::AppHandle,
    action: impl FnOnce(&mut Engine) -> Result<Effect, String>,
) -> Result<ViewState, String> {
    let state = app.state::<AppState>();
    let effect = {
        let mut rt = state.runtime.lock().map_err(|_| "状态暂时不可用")?;
        let previous = rt.engine.clone();
        let result = action(&mut rt.engine)?;
        if let Err(error) = save(&state.file, &rt.engine.data) {
            rt.engine = previous;
            return Err(format!("保存失败，操作未生效：{error}"));
        }
        result
    };
    apply_effect(app, effect);
    changed(app);
    Ok(view(app))
}
fn main_only(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        Err("请在主窗口执行此操作".into())
    } else {
        Ok(())
    }
}
fn rest_only(window: &tauri::WebviewWindow) -> Result<(), String> {
    if !window.label().starts_with("rest-") {
        Err("请在休息窗口执行此操作".into())
    } else {
        Ok(())
    }
}
#[tauri::command]
fn get_state(app: tauri::AppHandle) -> ViewState {
    view(&app)
}
#[tauri::command]
fn smoke_ready(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    title: String,
) -> Result<(), String> {
    main_only(&window)?;
    let state = app.state::<AppState>();
    if !state.smoke || title.is_empty() {
        return Err("当前不是桌面冒烟验证".into());
    }
    *state.smoke_title.lock().unwrap() = Some(title);
    state
        .smoke_ready
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = window.hide();
    Ok(())
}
#[tauri::command]
async fn save_settings(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    settings: Settings,
) -> Result<ViewState, String> {
    main_only(&window)?;
    let previous_language = app
        .state::<AppState>()
        .runtime
        .lock()
        .unwrap()
        .engine
        .data
        .settings
        .language;
    wallpapers::validate(&app, &settings)?;
    let result = mutate(&app, |e| {
        e.update_settings(settings)?;
        Ok(Effect::None)
    });
    display::sync(&app);
    let result = result?;
    configure_widget(&app)?;
    if result.timer.settings.language != previous_language {
        sync_language(&app);
    }
    Ok(result)
}
#[tauri::command]
async fn widget_drag(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    if window.label() != "widget" {
        return Err("此操作仅供悬浮窗使用".into());
    }
    widget_dock::begin_drag(&app, &window)
}
#[tauri::command]
fn get_widget_state(app: tauri::AppHandle) -> widget_dock::View {
    app.state::<AppState>().dock.lock().unwrap().view()
}
#[tauri::command]
async fn reset_widget(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<ViewState, String> {
    main_only(&window)?;
    let result = mutate(&app, |e| {
        let mut settings = e.data.settings.clone();
        settings.floating = true;
        e.update_settings(settings)?;
        Ok(Effect::None)
    })?;
    configure_widget(&app)?;
    if let Some(widget) = app.get_webview_window("widget") {
        widget_dock::reset(&app, &widget)?;
        widget.show().map_err(|e| e.to_string())?;
    }
    Ok(result)
}
#[tauri::command]
async fn toggle_pause(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<ViewState, String> {
    main_only(&window)?;
    mutate(&app, |e| {
        e.toggle_pause()?;
        Ok(Effect::None)
    })
}
#[tauri::command]
async fn start_rest(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<ViewState, String> {
    main_only(&window)?;
    mutate(&app, |e| {
        if e.phase == Phase::Resting {
            return Err("已经在休息中".into());
        }
        Ok(e.start_rest())
    })
}
#[tauri::command]
async fn unlock(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    text: String,
) -> Result<ViewState, String> {
    rest_only(&window)?;
    mutate(&app, |e| e.unlock(&text))
}
#[tauri::command]
async fn emergency_key(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<ViewState, String> {
    rest_only(&window)?;
    let effect = {
        let state = app.state::<AppState>();
        let mut rt = state.runtime.lock().map_err(|_| "状态暂时不可用")?;
        let previous = rt.engine.clone();
        let now = Instant::now();
        let elapsed = now.duration_since(rt.last_tick).as_millis() as u64;
        rt.last_tick = now;
        let elapsed_effect = rt.engine.tick(elapsed, None);
        let effect = if elapsed_effect == Effect::Unlock {
            elapsed_effect
        } else {
            rt.engine.emergency_key()?
        };
        if effect == Effect::Unlock
            && let Err(error) = save(&state.file, &rt.engine.data)
        {
            rt.engine = previous;
            return Err(error);
        }
        effect
    };
    apply_effect(&app, effect);
    changed(&app);
    Ok(view(&app))
}
#[tauri::command]
async fn focus_rest_input(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    if window.label() != "preview" {
        rest_only(&window)?;
        if app.state::<AppState>().runtime.lock().unwrap().engine.phase != Phase::Resting {
            return Err("当前不在休息中".into());
        }
    }
    window.set_focus().map_err(|e| e.to_string())?;
    window.as_ref().set_focus().map_err(|e| e.to_string())
}
#[tauri::command]
async fn preview_rest(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    main_only(&window)?;
    if let Some(window) = app.get_webview_window("preview") {
        window.show().map_err(|e| e.to_string())?;
        return window.set_focus().map_err(|e| e.to_string());
    }
    WebviewWindowBuilder::new(
        &app,
        "preview",
        WebviewUrl::App("index.html#preview".into()),
    )
    .title(localize(&app, "休息预览 · 一叶"))
    .inner_size(760.0, 620.0)
    .center()
    .decorations(false)
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
fn window_action(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    action: String,
) -> Result<(), String> {
    match action.as_str() {
        "minimize" if window.label() == "main" => window.minimize().map_err(|e| e.to_string()),
        "hide" if window.label() == "main" => window.hide().map_err(|e| e.to_string()),
        "close" if ["preview", "warning"].contains(&window.label()) => {
            window.close().map_err(|e| e.to_string())
        }
        "show" if view(&app).timer.phase != Phase::Resting => {
            if let Some(main) = app.get_webview_window("main") {
                let _ = main.show();
                let _ = main.set_focus();
            }
            Ok(())
        }
        _ => Err("此窗口当前不允许执行该操作".into()),
    }
}
fn configure_widget(app: &tauri::AppHandle) -> Result<(), String> {
    let snapshot = view(app);
    if !snapshot.timer.settings.floating || snapshot.timer.phase == Phase::Resting {
        if let Some(window) = app.get_webview_window("widget") {
            let _ = window.hide();
        }
        return Ok(());
    }
    let window = if let Some(window) = app.get_webview_window("widget") {
        window
    } else {
        let window =
            WebviewWindowBuilder::new(app, "widget", WebviewUrl::App("index.html#widget".into()))
                .title(localize(app, "一叶 · 悬浮计时"))
                .inner_size(widget_dock::WIDTH, widget_dock::HEIGHT)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .focused(false)
                .build()
                .map_err(|e| e.to_string())?;
        widget_dock::reset(app, &window)?;
        window
    };
    window
        .set_ignore_cursor_events(
            snapshot.timer.settings.click_through
                && !app.state::<AppState>().dock.lock().unwrap().view().hidden,
        )
        .map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())
}
fn rest_windows(app: &tauri::AppHandle, hidden: bool) -> Result<usize, String> {
    let main = app.get_webview_window("main").ok_or("主窗口不可用")?;
    let monitors = main.available_monitors().map_err(|e| e.to_string())?;
    if monitors.is_empty() {
        return Err("没有检测到显示器".into());
    }
    for (index, monitor) in monitors.iter().enumerate() {
        let label = format!("rest-{index}");
        if let Some(window) = app.get_webview_window(&label) {
            if !hidden && window.outer_position().ok().as_ref() != Some(monitor.position()) {
                let _ = window.set_fullscreen(false);
                let _ = window.set_position(*monitor.position());
                let _ = window.set_size(*monitor.size());
                let _ = window.set_fullscreen(true);
            }
            continue;
        }
        let window =
            WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html#rest".into()))
                .title(localize(app, "休息时间 · 一叶"))
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .closable(false)
                .minimizable(false)
                .background_color(tauri::window::Color(18, 25, 30, 255))
                .focused(false)
                .visible(false)
                .build()
                .map_err(|e| e.to_string())?;
        window
            .set_position(*monitor.position())
            .map_err(|e| e.to_string())?;
        window
            .set_size(PhysicalSize::new(
                monitor.size().width,
                monitor.size().height,
            ))
            .map_err(|e| e.to_string())?;
        if !hidden {
            window.set_fullscreen(true).map_err(|e| e.to_string())?;
            window.show().map_err(|e| e.to_string())?;
        }
    }
    for (label, window) in app.webview_windows() {
        if let Some(index) = label
            .strip_prefix("rest-")
            .and_then(|s| s.parse::<usize>().ok())
            && index >= monitors.len()
        {
            let _ = window.destroy();
        }
    }
    if !hidden {
        let focused = app.webview_windows().iter().any(|(label, window)| {
            label.starts_with("rest-") && window.is_focused().unwrap_or(false)
        });
        if let Some(window) = app.get_webview_window("rest-0") {
            if !focused {
                let _ = window.set_focus();
                let _ = window.as_ref().set_focus();
            }
            if !platform::LOCKED.load(std::sync::atomic::Ordering::Relaxed) {
                platform::capture(&window);
            }
        }
    }
    Ok(monitors.len())
}
fn close_rest(app: &tauri::AppHandle) {
    platform::release();
    for (label, window) in app.webview_windows() {
        if label.starts_with("rest-") {
            let _ = window.destroy();
        }
    }
    let _ = configure_widget(app);
}
fn apply_effect(app: &tauri::AppHandle, effect: Effect) {
    if effect == Effect::None {
        return;
    }
    let app = app.clone();
    let handle = app.clone();
    let _ = handle.run_on_main_thread(move || match effect {
        Effect::Lock => {
            if let Some(window) = app.get_webview_window("warning") {
                let _ = window.close();
            }
            match rest_windows(&app, false) {
                Ok(_) => {
                    for name in ["main", "widget", "preview"] {
                        if let Some(window) = app.get_webview_window(name) {
                            let _ = window.hide();
                        }
                    }
                }
                Err(error) => {
                    close_rest(&app);
                    let state = app.state::<AppState>();
                    let mut rt = state.runtime.lock().unwrap();
                    rt.engine.lock_failed(&error);
                    rt.notice = Some(error);
                    let _ = save(&state.file, &rt.engine.data);
                    drop(rt);
                    if let Some(main) = app.get_webview_window("main") {
                        let _ = main.show();
                    }
                    changed(&app);
                }
            }
        }
        Effect::Unlock => {
            close_rest(&app);
            if let Some(main) = app.get_webview_window("main") {
                let _ = main.show();
            }
        }
        Effect::Warn(seconds) => warning(&app, seconds),
        Effect::None => {}
    });
}
fn warning(app: &tauri::AppHandle, seconds: u32) {
    if !view(app).timer.settings.notifications {
        return;
    }
    let _ = app
        .notification()
        .builder()
        .title(localize(app, "一叶 · 休息将至"))
        .body(
            localize(app, "{seconds} 秒后开始休息，记得保存手边的工作。")
                .replace("{seconds}", &seconds.to_string()),
        )
        .show();
    if let Some(old) = app.get_webview_window("warning") {
        let _ = old.destroy();
    }
    if let Ok(window) =
        WebviewWindowBuilder::new(app, "warning", WebviewUrl::App("index.html#warning".into()))
            .title(localize(app, "休息提醒"))
            .inner_size(360.0, 130.0)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(false)
            .build()
    {
        if let Ok(Some(monitor)) = window.primary_monitor() {
            let p = monitor.position();
            let s = monitor.size();
            let scale = monitor.scale_factor();
            let _ = window.set_position(PhysicalPosition::new(
                p.x + s.width as i32 - (385.0 * scale) as i32,
                p.y + s.height as i32 - (300.0 * scale) as i32,
            ));
        }
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(9));
            let _ = window.close();
        });
    }
}
fn tray_icon(progress: f64) -> tauri::image::Image<'static> {
    let mut pixels = vec![0u8; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let dx = x as f64 - 15.5;
            let dy = y as f64 - 15.5;
            let radius = (dx * dx + dy * dy).sqrt();
            let angle = (dy.atan2(dx) + std::f64::consts::FRAC_PI_2)
                .rem_euclid(std::f64::consts::TAU)
                / std::f64::consts::TAU;
            let color = if (11.5..=15.0).contains(&radius) {
                if angle <= progress {
                    [132, 190, 125, 255]
                } else {
                    [110, 125, 113, 180]
                }
            } else if dx * dx / 35.0 + dy * dy / 60.0 < 1.0 {
                [175, 212, 143, 255]
            } else {
                [0, 0, 0, 0]
            };
            let index = (y * 32 + x) * 4;
            pixels[index..index + 4].copy_from_slice(&color);
        }
    }
    tauri::image::Image::new_owned(pixels, 32, 32)
}
fn tray_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem};
    let show = MenuItem::with_id(app, "show", localize(app, "打开一叶"), true, None::<&str>)?;
    let pause = MenuItem::with_id(
        app,
        "pause",
        localize(app, "暂停 / 继续计时"),
        true,
        None::<&str>,
    )?;
    let rest = MenuItem::with_id(app, "rest", localize(app, "现在休息"), true, None::<&str>)?;
    let widget = MenuItem::with_id(
        app,
        "widget",
        localize(app, "显示 / 隐藏悬浮计时"),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", localize(app, "退出一叶"), true, None::<&str>)?;
    Menu::with_items(app, &[&show, &pause, &rest, &widget, &quit])
}
fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let menu = tray_menu(app)?;
    tauri::tray::TrayIconBuilder::with_id("leafshield")
        .icon(tray_icon(1.0))
        .tooltip(localize(app, "一叶 · 给眼睛一点留白"))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let locked = view(app).timer.phase == Phase::Resting;
            match event.id.as_ref() {
                "show" if !locked => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "pause" if !locked => {
                    let _ = mutate(app, |e| {
                        e.toggle_pause()?;
                        Ok(Effect::None)
                    });
                }
                "rest" if !locked => {
                    let _ = mutate(app, |e| Ok(e.start_rest()));
                }
                "widget" if !locked => {
                    let _ = mutate(app, |e| {
                        let mut settings = e.data.settings.clone();
                        settings.floating = !settings.floating;
                        e.update_settings(settings)?;
                        Ok(Effect::None)
                    });
                    let _ = configure_widget(app);
                }
                "quit" if !locked => {
                    display::stop();
                    platform::release();
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                tauri::tray::TrayIconEvent::Click {
                    button: tauri::tray::MouseButton::Left,
                    button_state: tauri::tray::MouseButtonState::Up,
                    ..
                }
            ) {
                let app = tray.app_handle();
                if view(app).timer.phase != Phase::Resting
                    && let Some(window) = app.get_webview_window("main")
                {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;
    Ok(())
}
fn start_worker(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut ticks = 0u64;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            ticks += 1;
            let state = app.state::<AppState>();
            let needs_foreground = {
                let rt = state.runtime.lock().unwrap();
                !rt.engine.data.settings.whitelist.is_empty()
                    && (rt.engine.phase == Phase::Deferred
                        || (rt.engine.phase == Phase::Working
                            && rt.engine.snapshot().remaining_seconds <= 2))
            };
            let foreground = if needs_foreground {
                platform::foreground()
            } else {
                None
            };
            let effect = {
                let mut rt = state.runtime.lock().unwrap();
                let now = Instant::now();
                let elapsed = now.duration_since(rt.last_tick).as_millis() as u64;
                rt.last_tick = now;
                let effect = rt.engine.tick(elapsed, foreground.as_deref());
                if (ticks.is_multiple_of(30) || matches!(effect, Effect::Lock | Effect::Unlock))
                    && let Err(error) = save(&state.file, &rt.engine.data)
                {
                    rt.notice = Some(format!("保存失败：{error}"));
                }
                effect
            };
            apply_effect(&app, effect);
            changed(&app);
            let snapshot = view(&app);
            if let Some(tray) = app.tray_by_id("leafshield") {
                let seconds = snapshot.timer.remaining_seconds;
                let _ = tray.set_tooltip(Some(format!(
                    "{} · {:02}:{:02} · {}",
                    localize(&app, "一叶"),
                    seconds / 60,
                    seconds % 60,
                    localize(
                        &app,
                        if snapshot.timer.phase == Phase::Resting {
                            "休息中"
                        } else {
                            "专注中"
                        }
                    )
                )));
                if ticks.is_multiple_of(15) || effect != Effect::None {
                    let _ = tray.set_icon(Some(tray_icon(
                        seconds as f64 / snapshot.timer.total_seconds.max(1) as f64,
                    )));
                }
            }
            if snapshot.timer.phase == Phase::Resting && ticks.is_multiple_of(3) {
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || {
                    let _ = rest_windows(&handle, false);
                });
            }
        }
    });
}
fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let smoke_report = arguments
        .iter()
        .position(|s| s == "--smoke-test")
        .and_then(|i| arguments.get(i + 1))
        .cloned();
    let is_smoke = smoke_report.is_some();
    let background_start = arguments.iter().any(|s| s == "--autostart");
    let builder = tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app, args, _| {
        if args.iter().any(|s| s == "--autostart") { return; }
        if let Some(state) = app.try_state::<AppState>() && state.runtime.lock().unwrap().engine.phase == Phase::Resting { return; }
        if let Some(window) = app.get_webview_window("main") { let _ = window.show(); let _ = window.set_focus(); }
    })).plugin(tauri_plugin_notification::init()).invoke_handler(tauri::generate_handler![get_state, smoke_ready, display::preview_display, program_picker::pick_whitelist_programs, startup::get_startup, startup::set_startup, save_settings, wallpapers::set_custom_wallpaper, wallpapers::get_custom_wallpaper, widget_drag, get_widget_state, reset_widget, toggle_pause, start_rest, unlock, emergency_key, focus_rest_input, preview_rest, window_action])
    .setup(move |app| {
        let file = app.path().app_data_dir()?.join("settings.json");
        let (data, notice) = match std::fs::read(&file) {
            Ok(bytes) => match serde_json::from_slice::<Persisted>(&bytes) { Ok(data) => (data, None), Err(error) => { let backup = file.with_extension(format!("invalid-{}.json", chrono::Local::now().format("%Y%m%d-%H%M%S"))); let _ = std::fs::copy(&file, backup); (Persisted::default(), Some(format!("配置读取失败，已保留原文件并恢复默认设置：{error}"))) } },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Persisted::default(), None),
            Err(e) => (Persisted::default(), Some(format!("配置文件不可读：{e}")))
        };
        app.manage(AppState { runtime: Mutex::new(Runtime { engine: Engine::new(data), last_tick: Instant::now(), notice }), file, smoke: is_smoke, smoke_ready: std::sync::atomic::AtomicBool::new(false), smoke_title:Mutex::new(None), dock:Mutex::new(widget_dock::Controller::default()) });
        if let Some(report) = &smoke_report {
            // Let WebView2 finish loading the compact main page before hiding it.
            if let Some(main) = app.get_webview_window("main") { main.show()?; }
            let monitor_count = rest_windows(app.handle(), true).map_err(std::io::Error::other)?;
            let snapshot = view(app.handle());
            let report = PathBuf::from(report);
            if let Some(parent) = report.parent() { std::fs::create_dir_all(parent)?; }
            std::fs::write(&report, serde_json::to_vec_pretty(&serde_json::json!({"ok":true,"frontendReady":false,"monitors":monitor_count,"restWindows":app.webview_windows().keys().filter(|s|s.starts_with("rest-")).count(),"state":snapshot,"keyboardHookEnabled":false,"mode":"native-smoke-hidden-rest-windows"}))?)?;
            let handle = app.handle().clone(); std::thread::spawn(move || {
                for _ in 0..200 {
                    std::thread::sleep(Duration::from_millis(100));
                    if handle.state::<AppState>().smoke_ready.load(std::sync::atomic::Ordering::SeqCst) {
                        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                            let mut data:serde_json::Value=serde_json::from_slice(&std::fs::read(&report)?)?;
                            data["frontendReady"]=true.into();
                            data["renderedTitle"]=serde_json::to_value(handle.state::<AppState>().smoke_title.lock().unwrap().clone())?;
                            std::fs::write(&report,serde_json::to_vec_pretty(&data)?)?;
                            Ok(())
                        })();
                        handle.exit(if result.is_ok(){0}else{1});return;
                    }
                }
                handle.exit(1);
            });
        } else { if let Err(error) = startup::initialize(app.handle()) { app.state::<AppState>().runtime.lock().unwrap().notice = Some(error); } display::start(app.handle().clone()); platform::install(); setup_tray(app.handle())?; sync_language(app.handle()); configure_widget(app.handle()).map_err(std::io::Error::other)?; start_worker(app.handle().clone()); widget_dock::start(app.handle().clone()); if !background_start && let Some(main) = app.get_webview_window("main") { main.show()?; } }
        Ok(())
    }).on_window_event(|window, event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            if window.label() == "main" { api.prevent_close(); let _ = window.hide(); }
            if window.label().starts_with("rest-") && let Some(state) = window.app_handle().try_state::<AppState>() && state.runtime.lock().unwrap().engine.phase == Phase::Resting { api.prevent_close(); }
        }
    });
    let application = builder
        .build(tauri::generate_context!())
        .expect("无法启动一叶");
    application.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event
            && let Some(state) = app.try_state::<AppState>()
        {
            if !state.smoke && state.runtime.lock().unwrap().engine.phase == Phase::Resting {
                api.prevent_exit();
            } else {
                display::stop();
                platform::release();
                let rt = state.runtime.lock().unwrap();
                let _ = save(&state.file, &rt.engine.data);
            }
        }
    });
}
