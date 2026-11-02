use crate::{AppState, ViewState, main_only, mutate};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use leafshield_core::{Effect, Settings, wallpaper};
use std::{io::Write, path::PathBuf, sync::Mutex};
use tauri::Manager;

static UPLOAD: Mutex<()> = Mutex::new(());

fn path(app: &tauri::AppHandle, name: &str) -> Result<PathBuf, String> {
    if !wallpaper::owned_name(name) {
        return Err("背景图片路径无效".into());
    }
    let state = app.state::<AppState>();
    Ok(state
        .file
        .parent()
        .ok_or("配置目录无效")?
        .join("wallpapers")
        .join(name))
}

pub fn validate(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
    settings.validate()?;
    if settings.theme == "custom" {
        let file = path(
            app,
            settings.custom_wallpaper.as_deref().ok_or("请先选择图片")?,
        )?;
        let metadata = std::fs::metadata(file).map_err(|_| "自定义图片已丢失，请重新选择")?;
        if !metadata.is_file() || metadata.len() > wallpaper::MAX_STORED as u64 {
            return Err("自定义图片无效，请重新选择".into());
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn set_custom_wallpaper(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    data: String,
) -> Result<ViewState, String> {
    main_only(&window)?;
    if data.len() > wallpaper::MAX_UPLOAD.div_ceil(3) * 4 {
        return Err("请选择不超过 5 MB 的图片".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = UPLOAD.lock().map_err(|_| "图片处理暂时不可用")?;
        let bytes = STANDARD.decode(data).map_err(|_| "图片数据无效")?;
        let prepared = wallpaper::prepare(&bytes)?;
        let file = path(&app, &prepared.filename)?;
        std::fs::create_dir_all(file.parent().ok_or("图片目录无效")?).map_err(|e| e.to_string())?;
        let created = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file)
        {
            Ok(mut output) => {
                if let Err(error) = output
                    .write_all(&prepared.bytes)
                    .and_then(|_| output.sync_all())
                {
                    drop(output);
                    let _ = std::fs::remove_file(&file);
                    return Err(error.to_string());
                }
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
            Err(error) => return Err(error.to_string()),
        };
        let mut old = None;
        let result = mutate(&app, |engine| {
            old = engine.data.settings.custom_wallpaper.clone();
            let mut settings = engine.data.settings.clone();
            settings.theme = "custom".into();
            settings.custom_wallpaper = Some(prepared.filename.clone());
            engine.update_settings(settings)?;
            Ok(Effect::None)
        });
        if result.is_err() && created {
            let _ = std::fs::remove_file(file);
        } else if result.is_ok()
            && let Some(old) = old
            && old != prepared.filename
            && let Ok(file) = path(&app, &old)
        {
            let _ = std::fs::remove_file(file);
        }
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_custom_wallpaper(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let name = app
        .state::<AppState>()
        .runtime
        .lock()
        .map_err(|_| "状态暂时不可用")?
        .engine
        .data
        .settings
        .custom_wallpaper
        .clone();
    let Some(name) = name else {
        return Ok(None);
    };
    let file = path(&app, &name)?;
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(file)
            .map_err(|_| "自定义图片已丢失，暂时使用林间背景")?
            .take(wallpaper::MAX_STORED as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.is_empty() || bytes.len() > wallpaper::MAX_STORED {
            return Err("自定义图片无效，暂时使用林间背景".into());
        }
        Ok(Some(format!(
            "data:image/jpeg;base64,{}",
            STANDARD.encode(bytes)
        )))
    })
    .await
    .map_err(|e| e.to_string())?
}
