//! UI-independent host. Both native frontends can use the existing engine and data format.
#![cfg_attr(not(windows), allow(dead_code))]
use leafshield_core::{Effect, Engine, Persisted, Settings, wallpaper};
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub struct Runtime {
    pub engine: Engine,
    pub directory: PathBuf,
    pub notice: Option<String>,
    last_tick: Instant,
    saved_at: Instant,
}

pub fn save(path: &Path, data: &Persisted) -> Result<(), String> {
    std::fs::create_dir_all(path.parent().ok_or("配置路径无效")?).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("native.tmp");
    let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(data).map_err(|e| e.to_string())?)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
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

impl Runtime {
    pub fn load(directory: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let file = directory.join("settings.json");
        let (data, notice) = match std::fs::read(&file) {
            Ok(bytes) => match serde_json::from_slice::<Persisted>(&bytes) {
                Ok(data) => (data, None),
                Err(error) => {
                    let backup = directory.join(format!(
                        "settings.invalid-native-{}.json",
                        chrono::Local::now().format("%Y%m%d-%H%M%S")
                    ));
                    std::fs::write(backup, bytes).map_err(|e| e.to_string())?;
                    (
                        Persisted::default(),
                        Some(format!("配置读取失败，已保留原文件并恢复默认设置：{error}")),
                    )
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (Persisted::default(), None)
            }
            Err(error) => return Err(error.to_string()),
        };
        Ok(Self {
            engine: Engine::new(data),
            directory,
            notice,
            last_tick: Instant::now(),
            saved_at: Instant::now(),
        })
    }
    pub fn persist(&mut self) -> Result<(), String> {
        save(&self.directory.join("settings.json"), &self.engine.data)?;
        self.saved_at = Instant::now();
        Ok(())
    }
    pub fn tick(&mut self, foreground: Option<&str>) -> Effect {
        let now = Instant::now();
        let elapsed = now
            .duration_since(self.last_tick)
            .as_millis()
            .min(u64::MAX as u128) as u64;
        self.last_tick = now;
        let effect = self.engine.tick(elapsed, foreground);
        if (self.saved_at.elapsed() >= Duration::from_secs(30)
            || matches!(effect, Effect::Lock | Effect::Unlock))
            && let Err(error) = self.persist()
        {
            self.notice = Some(error);
        }
        effect
    }
    pub fn settings(&mut self, settings: Settings) -> Result<(), String> {
        if settings.theme == "custom" {
            let name = settings.custom_wallpaper.as_deref().ok_or("请先选择图片")?;
            if !wallpaper::owned_name(name)
                || !self.directory.join("wallpapers").join(name).is_file()
            {
                return Err("自定义图片已丢失，请重新选择".into());
            }
        }
        let previous = self.engine.clone();
        self.engine.update_settings(settings)?;
        if let Err(error) = self.persist() {
            self.engine = previous;
            return Err(error);
        }
        Ok(())
    }
    pub fn action(
        &mut self,
        action: impl FnOnce(&mut Engine) -> Result<Effect, String>,
    ) -> Result<Effect, String> {
        let previous = self.engine.clone();
        let effect = action(&mut self.engine)?;
        if let Err(error) = self.persist() {
            self.engine = previous;
            return Err(error);
        }
        Ok(effect)
    }
    pub fn import_wallpaper(&mut self, path: &Path) -> Result<(), String> {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(wallpaper::MAX_UPLOAD as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let prepared = wallpaper::prepare(&bytes)?;
        let directory = self.directory.join("wallpapers");
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        // Content-addressed names preserve the existing background and allow safe rollback.
        let file = directory.join(&prepared.filename);
        if !file.exists() {
            std::fs::write(&file, prepared.bytes).map_err(|e| e.to_string())?;
        }
        let mut settings = self.engine.data.settings.clone();
        settings.theme = "custom".into();
        settings.custom_wallpaper = Some(prepared.filename);
        self.settings(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "leafshield-native-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn legacy_configuration_and_statistics_survive_native_round_trip() {
        let dir = Fixture::new();
        let mut data = Persisted::default();
        data.settings.temperature_kelvin = 4700;
        data.settings.work_minutes = 25;
        data.statistics.date = chrono::Local::now().format("%Y-%m-%d").to_string();
        data.statistics.focus_seconds = 123;
        data.logs.push(leafshield_core::LogEntry {
            at: "before".into(),
            kind: "completed".into(),
            detail: "keep".into(),
        });
        save(&dir.0.join("settings.json"), &data).unwrap();
        let mut runtime = Runtime::load(dir.0.clone()).unwrap();
        let mut settings = runtime.engine.data.settings.clone();
        settings.rest_minutes = 3;
        runtime.settings(settings).unwrap();
        let restored = Runtime::load(dir.0.clone()).unwrap();
        assert_eq!(restored.engine.data.settings.temperature_kelvin, 4700);
        assert_eq!(restored.engine.data.settings.work_minutes, 25);
        assert_eq!(restored.engine.data.settings.rest_minutes, 3);
        assert_eq!(restored.engine.data.logs[0].detail, "keep");
        assert_eq!(restored.engine.data.statistics.focus_seconds, 123);
    }
    #[test]
    fn failed_persistence_rolls_back_changes_and_unlock() {
        let dir = Fixture::new();
        let mut runtime = Runtime::load(dir.0.clone()).unwrap();
        runtime.engine.start_rest();
        let phrase = runtime.engine.data.settings.phrase.clone();
        std::fs::create_dir(dir.0.join("settings.json")).unwrap();
        assert!(runtime.action(|engine| engine.unlock(&phrase)).is_err());
        assert_eq!(runtime.engine.phase, leafshield_core::Phase::Resting);
        assert_eq!(runtime.engine.data.statistics.mindful_unlocks, 0);
    }
    #[test]
    fn malformed_configuration_is_backed_up_before_recovery() {
        let dir = Fixture::new();
        std::fs::write(dir.0.join("settings.json"), b"bad json").unwrap();
        let runtime = Runtime::load(dir.0.clone()).unwrap();
        assert!(runtime.notice.is_some());
        let backup = std::fs::read_dir(&dir.0)
            .unwrap()
            .flatten()
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("settings.invalid-native-")
            })
            .unwrap();
        assert_eq!(std::fs::read(backup.path()).unwrap(), b"bad json");
    }
}
