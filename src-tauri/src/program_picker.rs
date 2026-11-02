#[cfg(windows)]
mod windows_picker {
    use std::{
        path::{Path, PathBuf},
        ptr,
    };
    use windows::{
        Win32::{
            Foundation::{ERROR_CANCELLED, HWND},
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize, IPersistFile, STGM_READ,
            },
            UI::Shell::{Common::COMDLG_FILTERSPEC, *},
        },
        core::{Interface, PCWSTR, PWSTR, w},
    };

    struct Apartment;
    impl Apartment {
        fn new() -> Result<Self, String> {
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }
                .map_err(|e| e.to_string())?;
            Ok(Self)
        }
    }
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    struct ShellString(PWSTR);
    impl Drop for ShellString {
        fn drop(&mut self) {
            unsafe {
                CoTaskMemFree(Some(self.0.0.cast()));
            }
        }
    }
    fn wide(path: &Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }
    fn is_extension(path: &Path, extension: &str) -> bool {
        path.extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case(extension))
    }
    fn executable_name(path: &Path) -> Result<String, String> {
        if !is_extension(path, "exe") || !path.is_file() {
            return Err("请选择存在的 EXE 程序，或指向 EXE 的快捷方式".into());
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("程序文件名无法识别")?;
        if name.len() > 80 || name.contains(['/', '\\', '\r', '\n']) {
            return Err("程序文件名过长或格式无效".into());
        }
        Ok(name.to_owned())
    }
    fn resolve(path: &Path) -> Result<String, String> {
        if is_extension(path, "exe") {
            return executable_name(path);
        }
        if !is_extension(path, "lnk") {
            return Err("仅支持 EXE 和 Windows 快捷方式（.lnk）".into());
        }
        let metadata = std::fs::metadata(path).map_err(|_| "快捷方式已丢失或无法读取")?;
        if !metadata.is_file() || metadata.len() > 1_000_000 {
            return Err("快捷方式文件无效或过大".into());
        }
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| e.to_string())?;
            let persist: IPersistFile = link.cast().map_err(|e| e.to_string())?;
            persist
                .Load(PCWSTR(wide(path).as_ptr()), STGM_READ)
                .map_err(|_| "无法读取此快捷方式")?;
            // Resolve metadata only: no launching, UI, search, tracking, or shortcut updates.
            let flags = (SLR_NO_UI.0 | SLR_NOUPDATE.0 | SLR_NOSEARCH.0 | SLR_NOTRACK.0) as u32
                | (1500 << 16);
            link.Resolve(HWND::default(), flags)
                .map_err(|_| "快捷方式目标已丢失或无法解析")?;
            let mut target = vec![0u16; 32768];
            link.GetPath(&mut target, ptr::null_mut(), 0)
                .map_err(|_| "无法读取快捷方式的目标程序")?;
            let end = target.iter().position(|c| *c == 0).unwrap_or(target.len());
            let target =
                String::from_utf16(&target[..end]).map_err(|_| "快捷方式目标路径无法识别")?;
            executable_name(Path::new(&target))
                .map_err(|_| "快捷方式没有指向可用的 EXE 程序".into())
        }
    }
    fn resolve_all(paths: &[PathBuf]) -> Result<Vec<String>, String> {
        if paths.len() > 32 {
            return Err("一次最多选择 32 个文件，白名单最多保留 5 个程序".into());
        }
        let mut names: Vec<String> = Vec::new();
        for path in paths {
            let name = resolve(path).map_err(|e| {
                format!(
                    "{}：{e}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )
            })?;
            if !names.iter().any(|old| old.eq_ignore_ascii_case(&name)) {
                names.push(name);
            }
        }
        if names.len() > 5 {
            return Err("白名单最多 5 个程序，请减少选择数量".into());
        }
        Ok(names)
    }
    pub fn pick(owner: usize) -> Result<Vec<String>, String> {
        let _apartment = Apartment::new()?;
        unsafe {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| e.to_string())?;
            dialog
                .SetTitle(w!("选择白名单程序（EXE 或快捷方式）"))
                .map_err(|e| e.to_string())?;
            dialog
                .SetFileTypes(&[
                    COMDLG_FILTERSPEC {
                        pszName: w!("程序和快捷方式 (*.exe;*.lnk)"),
                        pszSpec: w!("*.exe;*.lnk"),
                    },
                    COMDLG_FILTERSPEC {
                        pszName: w!("可执行程序 (*.exe)"),
                        pszSpec: w!("*.exe"),
                    },
                    COMDLG_FILTERSPEC {
                        pszName: w!("快捷方式 (*.lnk)"),
                        pszSpec: w!("*.lnk"),
                    },
                ])
                .map_err(|e| e.to_string())?;
            dialog
                .SetOptions(
                    dialog.GetOptions().map_err(|e| e.to_string())?
                        | FOS_ALLOWMULTISELECT
                        | FOS_FILEMUSTEXIST
                        | FOS_PATHMUSTEXIST
                        | FOS_FORCEFILESYSTEM
                        | FOS_NODEREFERENCELINKS
                        | FOS_NOCHANGEDIR
                        | FOS_DONTADDTORECENT,
                )
                .map_err(|e| e.to_string())?;
            if let Ok(desktop) =
                SHGetKnownFolderItem::<IShellItem>(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None)
            {
                let _ = dialog.SetDefaultFolder(&desktop);
            }
            if let Err(error) = dialog.Show(Some(HWND(owner as *mut _))) {
                if error.code() == ERROR_CANCELLED.to_hresult() {
                    return Ok(vec![]);
                }
                return Err(format!("无法打开程序选择窗口：{error}"));
            }
            let items = dialog.GetResults().map_err(|e| e.to_string())?;
            let count = items.GetCount().map_err(|e| e.to_string())?;
            if count > 32 {
                return Err("一次最多选择 32 个文件".into());
            }
            let mut paths = Vec::new();
            for index in 0..count {
                let item = items.GetItemAt(index).map_err(|e| e.to_string())?;
                let value = ShellString(
                    item.GetDisplayName(SIGDN_FILESYSPATH)
                        .map_err(|_| "请选择文件系统中的程序或快捷方式")?,
                );
                paths.push(PathBuf::from(
                    value.0.to_string().map_err(|_| "文件路径无法识别")?,
                ));
            }
            resolve_all(&paths)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                let dir = std::env::temp_dir().join(format!(
                    "leafshield-picker-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                ));
                std::fs::create_dir(&dir).unwrap();
                Self(dir)
            }
            fn file(&self, name: &str) -> PathBuf {
                let path = self.0.join(name);
                std::fs::write(&path, []).unwrap();
                path
            }
            fn link(&self, name: &str, target: &Path) -> PathBuf {
                let path = self.0.join(name);
                unsafe {
                    let link: IShellLinkW =
                        CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
                    link.SetPath(PCWSTR(wide(target).as_ptr())).unwrap();
                    link.SetArguments(w!("--ignored=other.exe")).unwrap();
                    link.cast::<IPersistFile>()
                        .unwrap()
                        .Save(PCWSTR(wide(&path).as_ptr()), true)
                        .unwrap();
                }
                path
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                assert!(self.0.starts_with(std::env::temp_dir()));
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        #[test]
        fn exe_selection_keeps_only_unicode_filename_and_deduplicates_links() {
            let _com = Apartment::new().unwrap();
            let f = Fixture::new();
            let target = f.file("我的 程序.EXE");
            let shortcut = f.link("桌面显示名称.lnk", &target);
            assert_eq!(
                resolve_all(&[target, shortcut]).unwrap(),
                vec!["我的 程序.EXE"]
            );
        }
        #[test]
        fn missing_corrupt_and_non_exe_shortcuts_are_rejected() {
            let _com = Apartment::new().unwrap();
            let f = Fixture::new();
            assert!(resolve(&f.file("bad.lnk")).is_err());
            assert!(resolve(&f.link("missing.lnk", &f.0.join("missing.exe"))).is_err());
            assert!(resolve(&f.link("document.lnk", &f.file("document.txt"))).is_err());
            assert!(resolve(&f.file("website.url")).is_err());
        }
        #[test]
        fn multi_selection_is_atomic_and_enforces_the_existing_limit() {
            let _com = Apartment::new().unwrap();
            let f = Fixture::new();
            let paths: Vec<_> = (0..6).map(|n| f.file(&format!("app{n}.exe"))).collect();
            assert_eq!(resolve_all(&paths[..5]).unwrap().len(), 5);
            assert!(resolve_all(&paths).is_err());
            assert!(resolve_all(&[paths[0].clone(), f.file("bad.lnk")]).is_err());
        }
    }
}

#[tauri::command]
pub async fn pick_whitelist_programs(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Vec<String>, String> {
    crate::main_only(&window)?;
    use tauri::Manager;
    if app
        .state::<crate::AppState>()
        .runtime
        .lock()
        .map_err(|_| "状态暂时不可用")?
        .engine
        .phase
        == leafshield_core::Phase::Resting
    {
        return Err("休息结束后可以修改白名单".into());
    }
    #[cfg(windows)]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static OPEN: AtomicBool = AtomicBool::new(false);
        OPEN.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "程序选择窗口已打开")?;
        struct Busy;
        impl Drop for Busy {
            fn drop(&mut self) {
                OPEN.store(false, Ordering::SeqCst);
            }
        }
        let _busy = Busy;
        let owner = window.hwnd().map_err(|e| e.to_string())?.0 as usize;
        tauri::async_runtime::spawn_blocking(move || windows_picker::pick(owner))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    Err("当前程序选择仅支持 Windows，请手动填写进程名".into())
}
