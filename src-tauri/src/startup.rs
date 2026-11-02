use serde::Serialize;

#[cfg(windows)]
fn read_preference(path: &std::path::Path) -> Result<bool, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<bool>(&bytes)
            .map_err(|_| "开机启动偏好无效，请在选项中重新设置".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e.to_string()),
    }
}
#[cfg(windows)]
fn write_preference(path: &std::path::Path, enabled: bool) -> Result<(), String> {
    use std::io::Write;
    std::fs::create_dir_all(path.parent().ok_or("偏好目录无效")?).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("tmp");
    let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
    file.write_all(if enabled { b"true" } else { b"false" })
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    std::fs::rename(temporary, path).map_err(|e| e.to_string())
}
#[cfg(windows)]
fn preference_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("startup.json"))
}
#[cfg(windows)]
fn configure(app: &tauri::AppHandle, enabled: bool) -> Result<Info, String> {
    let command = if enabled {
        Some(windows::command(
            &std::env::current_exe().map_err(|e| e.to_string())?,
        )?)
    } else {
        None
    };
    let previous = windows::read(windows::RUN, windows::NAME)?;
    let path = preference_path(app)?;
    windows::write(windows::RUN, windows::NAME, command.as_deref())?;
    if let Err(error) = write_preference(&path, enabled) {
        windows::write(windows::RUN, windows::NAME, previous.as_deref())
            .map_err(|rollback| format!("{error}；恢复启动项失败：{rollback}"))?;
        return Err(error);
    }
    info()
}
pub fn initialize(app: &tauri::AppHandle) -> Result<Info, String> {
    #[cfg(windows)]
    {
        configure(app, read_preference(&preference_path(app)?)?)
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        info()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    supported: bool,
    enabled: bool,
    path_matches: bool,
    executable: String,
}

#[cfg(windows)]
mod windows {
    use std::{path::Path, ptr};
    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::*,
    };

    pub const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    pub const NAME: &str = "LeafShield";
    pub struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    fn error(code: u32) -> String {
        format!(
            "无法设置开机启动：{}",
            std::io::Error::from_raw_os_error(code as i32)
        )
    }
    fn open(path: &str, create: bool, access: u32) -> Result<Option<Key>, String> {
        let mut key = ptr::null_mut();
        let path = wide(path);
        let code = unsafe {
            if create {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    path.as_ptr(),
                    0,
                    ptr::null(),
                    0,
                    access,
                    ptr::null(),
                    &mut key,
                    ptr::null_mut(),
                )
            } else {
                RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut key)
            }
        };
        match code {
            ERROR_SUCCESS => Ok(Some(Key(key))),
            ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(None),
            code => Err(error(code)),
        }
    }
    pub fn command(path: &Path) -> Result<String, String> {
        let path = path.to_str().ok_or("程序路径包含无法识别的字符")?;
        let normalized = if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{unc}")
        } else {
            path.strip_prefix(r"\\?\").unwrap_or(path).to_owned()
        };
        let path = normalized.as_str();
        if path.is_empty() || path.contains(['"', '\0', '\r', '\n']) {
            return Err("程序路径无效，请将 EXE 放在固定目录".into());
        }
        let command = format!("\"{path}\" --autostart");
        if command.encode_utf16().count() > 260 {
            return Err("程序路径过长，开机启动命令最多 260 个字符，请移动 EXE 后重试".into());
        }
        Ok(command)
    }
    pub fn read(path: &str, name: &str) -> Result<Option<String>, String> {
        let Some(key) = open(path, false, KEY_QUERY_VALUE)? else {
            return Ok(None);
        };
        let name = wide(name);
        let mut kind = 0;
        let mut size = 0;
        let code = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                ptr::null(),
                &mut kind,
                ptr::null_mut(),
                &mut size,
            )
        };
        if code == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if code != ERROR_SUCCESS {
            return Err(error(code));
        }
        if kind != REG_SZ || size > 65536 || !size.is_multiple_of(2) {
            return Err("启动项格式无效，请检查 Windows 启动项".into());
        }
        let mut value = vec![0u16; (size / 2) as usize];
        let code = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                ptr::null(),
                &mut kind,
                value.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if code != ERROR_SUCCESS {
            return Err(error(code));
        }
        value.truncate((size / 2) as usize);
        while value.last() == Some(&0) {
            value.pop();
        }
        String::from_utf16(&value)
            .map(Some)
            .map_err(|_| "启动项编码无效".into())
    }
    pub fn write(path: &str, name: &str, value: Option<&str>) -> Result<(), String> {
        let Some(key) = open(path, value.is_some(), KEY_SET_VALUE)? else {
            return Ok(());
        };
        let name = wide(name);
        let code = unsafe {
            match value {
                Some(value) => {
                    let value = wide(value);
                    RegSetValueExW(
                        key.0,
                        name.as_ptr(),
                        0,
                        REG_SZ,
                        value.as_ptr().cast(),
                        (value.len() * 2) as u32,
                    )
                }
                None => RegDeleteValueW(key.0, name.as_ptr()),
            }
        };
        if code == ERROR_SUCCESS || (value.is_none() && code == ERROR_FILE_NOT_FOUND) {
            Ok(())
        } else {
            Err(error(code))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn quotes_unicode_and_spaces_and_rejects_invalid_or_long_commands() {
            assert_eq!(
                command(Path::new(r"D:\我的软件\Leaf Shield.exe")).unwrap(),
                "\"D:\\我的软件\\Leaf Shield.exe\" --autostart"
            );
            assert_eq!(
                command(Path::new(r"\\?\D:\软件\app.exe")).unwrap(),
                "\"D:\\软件\\app.exe\" --autostart"
            );
            assert!(command(Path::new("bad\"path.exe")).is_err());
            assert!(command(Path::new(&"a".repeat(261))).is_err());
        }
        #[test]
        fn registry_roundtrip_updates_one_value_and_preserves_neighbors() {
            let path = format!(r"Software\LeafShield\Tests\startup-{}", std::process::id());
            struct Cleanup(String);
            impl Drop for Cleanup {
                fn drop(&mut self) {
                    unsafe {
                        RegDeleteKeyW(HKEY_CURRENT_USER, wide(&self.0).as_ptr());
                    }
                }
            }
            let _cleanup = Cleanup(path.clone());
            assert_eq!(read(&path, NAME).unwrap(), None);
            write(&path, "Neighbor", Some("keep")).unwrap();
            let first = command(Path::new(r"D:\我的软件\Leaf Shield.exe")).unwrap();
            write(&path, NAME, Some(&first)).unwrap();
            assert_eq!(read(&path, NAME).unwrap(), Some(first));
            write(&path, NAME, Some("updated")).unwrap();
            assert_eq!(read(&path, NAME).unwrap().as_deref(), Some("updated"));
            write(&path, NAME, None).unwrap();
            write(&path, NAME, None).unwrap();
            assert_eq!(read(&path, NAME).unwrap(), None);
            assert_eq!(read(&path, "Neighbor").unwrap().as_deref(), Some("keep"));
            write(&path, "Neighbor", None).unwrap();
        }
    }
}

pub fn info() -> Result<Info, String> {
    #[cfg(windows)]
    {
        let path = std::env::current_exe().map_err(|e| e.to_string())?;
        let expected = windows::command(&path).ok();
        let existing = windows::read(windows::RUN, windows::NAME)?;
        Ok(Info {
            supported: true,
            enabled: existing.is_some(),
            path_matches: expected.is_some() && existing == expected,
            executable: path.display().to_string(),
        })
    }
    #[cfg(not(windows))]
    Ok(Info {
        supported: false,
        enabled: false,
        path_matches: false,
        executable: String::new(),
    })
}

#[tauri::command]
pub fn get_startup(window: tauri::WebviewWindow) -> Result<Info, String> {
    crate::main_only(&window)?;
    info()
}

#[tauri::command]
pub fn set_startup(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    enabled: bool,
) -> Result<Info, String> {
    crate::main_only(&window)?;
    #[cfg(windows)]
    {
        configure(&app, enabled)
    }
    #[cfg(not(windows))]
    {
        let _ = (app, enabled);
        Err("当前仅支持 Windows 开机启动".into())
    }
}

#[cfg(all(test, windows))]
mod preference_tests {
    use super::*;
    #[test]
    fn defaults_on_and_explicit_off_survives_reload() {
        let path = std::env::temp_dir().join(format!(
            "leafshield-startup-choice-{}.json",
            std::process::id()
        ));
        assert!(read_preference(&path).unwrap());
        write_preference(&path, false).unwrap();
        assert!(!read_preference(&path).unwrap());
        write_preference(&path, true).unwrap();
        assert!(read_preference(&path).unwrap());
        std::fs::remove_file(path).unwrap();
    }
}
