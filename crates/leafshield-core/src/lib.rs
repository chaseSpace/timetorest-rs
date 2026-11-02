use chrono::Local;
use serde::{Deserialize, Serialize};
pub mod colour;
pub mod docking;
pub mod locale;
pub mod wallpaper;
use locale::{Language, phrase_index, phrases, tr};

pub const PHRASES: [&str; 5] = [
    "我知道休息不足可能会伤害眼睛和大脑，继续解锁",
    "工作可以稍等片刻，我的眼睛值得被认真照顾",
    "我选择有意识地继续工作，并记得稍后好好休息",
    "停下来不是浪费时间，而是为了更清醒地出发",
    "此刻我愿意放慢脚步，给眼睛和身体一点温柔",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub language: Language,
    pub work_minutes: u32,
    pub rest_minutes: u32,
    pub phrase: String,
    pub theme: String,
    pub temperature_kelvin: u32,
    pub brightness_percent: u32,
    pub display_enabled: bool,
    pub custom_wallpaper: Option<String>,
    pub floating: bool,
    pub click_through: bool,
    pub edge_hide: bool,
    pub notifications: bool,
    pub emergency_enabled: bool,
    pub whitelist: Vec<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::default(),
            work_minutes: 45,
            rest_minutes: 2,
            phrase: PHRASES[0].into(),
            theme: "forest".into(),
            temperature_kelvin: 5000,
            brightness_percent: 80,
            display_enabled: true,
            custom_wallpaper: None,
            floating: true,
            click_through: false,
            edge_hide: true,
            notifications: true,
            emergency_enabled: true,
            whitelist: vec![],
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !(1500..=6500).contains(&self.temperature_kelvin)
            || !(20..=100).contains(&self.brightness_percent)
        {
            return Err(tr(self.language, "色温应为 1500–6500 K，亮度应为 20–100%"));
        }
        if !(1..=240).contains(&self.work_minutes) {
            return Err(tr(self.language, "工作时长应为 1–240 分钟"));
        }
        if !(1..=60).contains(&self.rest_minutes) {
            return Err(tr(self.language, "休息时长应为 1–60 分钟"));
        }
        if !phrases(self.language).contains(&self.phrase) {
            return Err(tr(self.language, "解锁文案只能从 5 条预设中选择"));
        }
        if !["forest", "mountain", "night", "custom"].contains(&self.theme.as_str()) {
            return Err(tr(self.language, "请选择有效的休息主题"));
        }
        if self
            .custom_wallpaper
            .as_ref()
            .is_some_and(|name| !wallpaper::owned_name(name))
            || (self.theme == "custom" && self.custom_wallpaper.is_none())
        {
            return Err(tr(self.language, "请重新选择自定义背景图片"));
        }
        if self.whitelist.len() > 5
            || self
                .whitelist
                .iter()
                .any(|s| s.is_empty() || s.len() > 80 || s.contains(['/', '\\', '\n', '\r']))
        {
            return Err(tr(
                self.language,
                "最多添加 5 个白名单程序，只填写进程文件名",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Working,
    Paused,
    Resting,
    Deferred,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub at: String,
    pub kind: String,
    pub detail: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Statistics {
    pub date: String,
    pub focus_seconds: u64,
    pub rest_seconds: u64,
    pub completed_rests: u32,
    pub mindful_unlocks: u32,
    pub emergency_unlocks: u32,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Persisted {
    pub settings: Settings,
    pub statistics: Statistics,
    pub logs: Vec<LogEntry>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub phase: Phase,
    pub remaining_seconds: u64,
    pub total_seconds: u64,
    pub settings: Settings,
    pub statistics: Statistics,
    pub logs: Vec<LogEntry>,
    pub phrases: Vec<String>,
    pub emergency_count: u32,
    pub deferred_by: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    None,
    Warn(u32),
    Lock,
    Unlock,
}

#[derive(Clone)]
pub struct Engine {
    pub data: Persisted,
    pub phase: Phase,
    remaining_ms: u64,
    warned_60: bool,
    warned_30: bool,
    emergency_count: u32,
    emergency_first_ms: u64,
    emergency_last_ms: u64,
    clock_ms: u64,
    statistics_remainder_ms: u64,
    pub deferred_by: Option<String>,
}
impl Engine {
    pub fn new(mut data: Persisted) -> Self {
        // Older releases allowed custom text. Preserve other preferences when migrating it.
        data.settings.phrase = phrases(data.settings.language)
            [phrase_index(&data.settings.phrase).unwrap_or(0)]
        .clone();
        if data.settings.validate().is_err() {
            data.settings = Settings::default();
        }
        let remaining_ms = data.settings.work_minutes as u64 * 60_000;
        let mut engine = Self {
            data,
            phase: Phase::Working,
            remaining_ms,
            warned_60: false,
            warned_30: false,
            emergency_count: 0,
            emergency_first_ms: 0,
            emergency_last_ms: 0,
            clock_ms: 0,
            statistics_remainder_ms: 0,
            deferred_by: None,
        };
        engine.roll_day();
        engine
    }
    fn roll_day(&mut self) {
        let date = Local::now().format("%Y-%m-%d").to_string();
        if self.data.statistics.date != date {
            self.data.statistics = Statistics {
                date,
                ..Default::default()
            };
        }
    }
    fn log(&mut self, kind: &str, detail: &str) {
        self.data.logs.insert(
            0,
            LogEntry {
                at: Local::now().to_rfc3339(),
                kind: kind.into(),
                detail: tr(self.data.settings.language, detail),
            },
        );
        self.data.logs.truncate(200);
    }
    pub fn remaining_seconds(&self) -> u64 {
        self.remaining_ms.div_ceil(1000)
    }
    pub fn emergency_count(&self) -> u32 {
        self.emergency_count
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            phase: self.phase,
            remaining_seconds: self.remaining_seconds(),
            total_seconds: if self.phase == Phase::Resting {
                self.data.settings.rest_minutes as u64 * 60
            } else {
                self.data.settings.work_minutes as u64 * 60
            },
            settings: self.data.settings.clone(),
            statistics: self.data.statistics.clone(),
            logs: self.data.logs.clone(),
            phrases: phrases(self.data.settings.language),
            emergency_count: self.emergency_count,
            deferred_by: self.deferred_by.clone(),
        }
    }
    pub fn tick(&mut self, elapsed_ms: u64, foreground: Option<&str>) -> Effect {
        self.roll_day();
        self.clock_ms = self.clock_ms.saturating_add(elapsed_ms);
        if self.emergency_count > 0 && self.clock_ms.saturating_sub(self.emergency_last_ms) > 2000 {
            self.emergency_count = 0;
        }
        if self.phase == Phase::Paused {
            return Effect::None;
        }
        if self.phase == Phase::Deferred {
            if let Some(process) = self.allowed_foreground(foreground) {
                self.deferred_by = Some(process);
                return Effect::None;
            }
            return self.start_rest();
        }
        let consumed = elapsed_ms.min(self.remaining_ms);
        self.statistics_remainder_ms += consumed;
        let seconds = self.statistics_remainder_ms / 1000;
        self.statistics_remainder_ms %= 1000;
        if self.phase == Phase::Working {
            self.data.statistics.focus_seconds += seconds;
        } else {
            self.data.statistics.rest_seconds += seconds;
        }
        self.remaining_ms = self.remaining_ms.saturating_sub(elapsed_ms);
        if self.phase == Phase::Resting {
            if self.remaining_ms == 0 {
                self.data.statistics.completed_rests += 1;
                self.log("completed", "完成了一次完整休息");
                self.restart_work();
                return Effect::Unlock;
            }
            return Effect::None;
        }
        if self.remaining_ms == 0 {
            if let Some(process) = self.allowed_foreground(foreground) {
                self.phase = Phase::Deferred;
                self.deferred_by = Some(process);
                self.log("deferred", "前台白名单程序正在使用，休息已延后");
                return Effect::None;
            }
            return self.start_rest();
        }
        if self.remaining_ms <= 30_000 && !self.warned_30 {
            self.warned_30 = true;
            self.warned_60 = true;
            return Effect::Warn(30);
        }
        if self.remaining_ms <= 60_000 && !self.warned_60 {
            self.warned_60 = true;
            return Effect::Warn(60);
        }
        Effect::None
    }
    fn allowed_foreground(&self, foreground: Option<&str>) -> Option<String> {
        foreground
            .filter(|name| {
                self.data
                    .settings
                    .whitelist
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(name))
            })
            .map(str::to_owned)
    }
    pub fn start_rest(&mut self) -> Effect {
        self.phase = Phase::Resting;
        self.remaining_ms = self.data.settings.rest_minutes as u64 * 60_000;
        self.emergency_count = 0;
        self.statistics_remainder_ms = 0;
        self.deferred_by = None;
        self.log("rest", "放下手边的工作，给眼睛一点休息时间");
        Effect::Lock
    }
    fn restart_work(&mut self) {
        self.phase = Phase::Working;
        self.remaining_ms = self.data.settings.work_minutes as u64 * 60_000;
        self.warned_60 = false;
        self.warned_30 = false;
        self.emergency_count = 0;
        self.statistics_remainder_ms = 0;
        self.deferred_by = None;
    }
    pub fn toggle_pause(&mut self) -> Result<(), String> {
        match self.phase {
            Phase::Working => self.phase = Phase::Paused,
            Phase::Paused => self.phase = Phase::Working,
            _ => {
                return Err(tr(
                    self.data.settings.language,
                    "休息期间不能暂停，请完成休息或手输正念文案",
                ));
            }
        };
        Ok(())
    }
    pub fn update_settings(&mut self, mut settings: Settings) -> Result<(), String> {
        if self.phase == Phase::Resting {
            return Err(tr(self.data.settings.language, "休息结束后可以修改设置"));
        }
        if settings.language != self.data.settings.language
            && let Some(index) = phrase_index(&settings.phrase)
        {
            settings.phrase = phrases(settings.language)[index].clone();
        }
        settings.validate()?;
        let duration_changed = self.data.settings.work_minutes != settings.work_minutes;
        self.data.settings = settings;
        if duration_changed {
            let paused = self.phase == Phase::Paused;
            self.restart_work();
            if paused {
                self.phase = Phase::Paused;
            }
        }
        Ok(())
    }
    pub fn unlock(&mut self, text: &str) -> Result<Effect, String> {
        if self.phase != Phase::Resting {
            return Err(tr(self.data.settings.language, "当前没有正在进行的休息"));
        }
        if text != self.data.settings.phrase {
            return Err(tr(
                self.data.settings.language,
                "请完整、准确地手动输入上方文案",
            ));
        }
        self.data.statistics.mindful_unlocks += 1;
        self.log("mindful", "手动输入正念文案，提前结束休息");
        self.restart_work();
        Ok(Effect::Unlock)
    }
    pub fn emergency_key(&mut self) -> Result<Effect, String> {
        if self.phase != Phase::Resting || !self.data.settings.emergency_enabled {
            return Err(tr(self.data.settings.language, "紧急通道当前不可用"));
        }
        if self.emergency_count == 0
            || self.clock_ms.saturating_sub(self.emergency_last_ms) > 2000
            || self.clock_ms.saturating_sub(self.emergency_first_ms) > 20_000
        {
            self.emergency_count = 0;
            self.emergency_first_ms = self.clock_ms;
        }
        if self.emergency_count > 0 && self.clock_ms.saturating_sub(self.emergency_last_ms) < 35 {
            return Ok(Effect::None);
        }
        self.emergency_last_ms = self.clock_ms;
        self.emergency_count += 1;
        if self.emergency_count >= 50 {
            self.data.statistics.emergency_unlocks += 1;
            self.log(
                "emergency",
                "紧急解锁：20 秒内连续手动敲击 50 次，已记录到偷懒日志",
            );
            self.restart_work();
            return Ok(Effect::Unlock);
        }
        Ok(Effect::None)
    }
    pub fn lock_failed(&mut self, error: &str) {
        self.restart_work();
        self.phase = Phase::Paused;
        self.log("error", &format!("休息窗口创建失败，计时已暂停：{error}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_change_preserves_preset_and_preferences() {
        let mut e = Engine::new(Persisted::default());
        let mut settings = e.data.settings.clone();
        settings.phrase = PHRASES[2].into();
        settings.temperature_kelvin = 4700;
        settings.brightness_percent = 89;
        e.update_settings(settings).unwrap();
        let mut settings = e.data.settings.clone();
        settings.language = Language::English;
        e.update_settings(settings).unwrap();
        assert_eq!(e.data.settings.phrase, phrases(Language::English)[2]);
        assert_eq!(e.snapshot().phrases.len(), 5);
        assert_eq!(e.data.settings.temperature_kelvin, 4700);
        assert_eq!(e.data.settings.brightness_percent, 89);
        let mut settings = e.data.settings.clone();
        settings.language = Language::Chinese;
        e.update_settings(settings).unwrap();
        assert_eq!(e.data.settings.phrase, PHRASES[2]);
    }
    #[test]
    fn english_unlock_requires_the_exact_english_phrase() {
        let mut settings = Settings {
            language: Language::English,
            phrase: phrases(Language::English)[0].clone(),
            ..Settings::default()
        };
        let mut e = Engine::new(Persisted {
            settings: settings.clone(),
            ..Default::default()
        });
        e.start_rest();
        assert!(e.unlock(PHRASES[0]).is_err());
        assert!(e.unlock(&settings.phrase.to_lowercase()).is_err());
        assert_eq!(e.unlock(&settings.phrase).unwrap(), Effect::Unlock);
        settings.phrase = PHRASES[0].into();
        assert!(settings.validate().is_err());
        settings.phrase = "An arbitrary custom unlock phrase".into();
        assert!(settings.validate().is_err());
    }
    #[test]
    fn legacy_configuration_defaults_to_chinese_and_language_round_trips() {
        let old: Persisted =
            serde_json::from_str(r#"{"settings":{"workMinutes":25,"restMinutes":2}}"#).unwrap();
        let e = Engine::new(old);
        assert_eq!(e.data.settings.language, Language::Chinese);
        assert_eq!(e.data.settings.work_minutes, 25);
        let data = Persisted {
            settings: Settings {
                language: Language::English,
                phrase: phrases(Language::English)[4].clone(),
                ..Settings::default()
            },
            ..Default::default()
        };
        let decoded: Persisted =
            serde_json::from_slice(&serde_json::to_vec(&data).unwrap()).unwrap();
        let restored = Engine::new(decoded);
        assert_eq!(restored.data.settings.language, Language::English);
        assert_eq!(restored.data.settings.phrase, phrases(Language::English)[4]);
    }
    fn engine(work: u32, rest: u32) -> Engine {
        Engine::new(Persisted {
            settings: Settings {
                work_minutes: work,
                rest_minutes: rest,
                ..Settings::default()
            },
            ..Default::default()
        })
    }
    #[test]
    fn warnings_fire_once_then_lock_and_complete() {
        let mut e = engine(2, 1);
        assert_eq!(e.tick(60_000, None), Effect::Warn(60));
        assert_eq!(e.tick(1_000, None), Effect::None);
        assert_eq!(e.tick(29_000, None), Effect::Warn(30));
        assert_eq!(e.tick(30_000, None), Effect::Lock);
        assert_eq!(e.tick(60_000, None), Effect::Unlock);
        assert_eq!(e.data.statistics.completed_rests, 1);
        assert_eq!(e.data.statistics.focus_seconds, 120);
        assert_eq!(e.data.statistics.rest_seconds, 60);
    }
    #[test]
    fn pause_and_partial_seconds_do_not_drift() {
        let mut e = engine(1, 1);
        e.toggle_pause().unwrap();
        e.tick(99_000, None);
        assert_eq!(e.snapshot().remaining_seconds, 60);
        e.toggle_pause().unwrap();
        for _ in 0..10 {
            e.tick(100, None);
        }
        assert_eq!(e.data.statistics.focus_seconds, 1);
        assert_eq!(e.snapshot().remaining_seconds, 59);
    }
    #[test]
    fn whitelist_matches_only_foreground_and_resumes_immediately() {
        let mut e = engine(1, 1);
        e.data.settings.whitelist.push("Teams.exe".into());
        assert_eq!(e.tick(60_000, Some("teams.EXE")), Effect::None);
        assert_eq!(e.phase, Phase::Deferred);
        assert_eq!(e.tick(5000, Some("Teams.exe")), Effect::None);
        assert_eq!(e.tick(1000, Some("Code.exe")), Effect::Lock);
    }
    #[test]
    fn unlock_requires_exact_unicode_phrase_and_locked_phase() {
        let mut e = engine(1, 1);
        let phrase = e.data.settings.phrase.clone();
        assert!(e.unlock(&phrase).is_err());
        e.start_rest();
        assert!(e.unlock(&(phrase.clone() + " ")).is_err());
        assert!(e.unlock("我想跳过").is_err());
        assert_eq!(e.unlock(&phrase).unwrap(), Effect::Unlock);
        assert_eq!(e.data.statistics.mindful_unlocks, 1);
    }
    #[test]
    fn rest_cannot_be_paused_or_reconfigured() {
        let mut e = engine(1, 1);
        e.start_rest();
        assert!(e.toggle_pause().is_err());
        assert!(e.update_settings(Settings::default()).is_err());
    }
    #[test]
    fn only_presets_are_allowed_including_backend_requests() {
        for phrase in PHRASES {
            let s = Settings {
                phrase: phrase.into(),
                ..Default::default()
            };
            assert!(s.validate().is_ok());
            assert!(phrase.chars().count() > 10);
        }
        let mut e = engine(45, 5);
        let s = Settings {
            phrase: "这是一条长度足够但是不允许的自定义文案".into(),
            ..Default::default()
        };
        assert!(e.update_settings(s).is_err());
        assert_eq!(e.data.settings.phrase, PHRASES[0]);
    }
    #[test]
    fn old_custom_phrase_migrates_without_losing_other_preferences() {
        let mut data = Persisted::default();
        data.settings.work_minutes = 25;
        data.settings.phrase = "旧版本的自定义解锁文案需要自动迁移".into();
        data.settings.theme = "night".into();
        let e = Engine::new(data);
        assert_eq!(e.data.settings.phrase, PHRASES[0]);
        assert_eq!(e.data.settings.work_minutes, 25);
        assert_eq!(e.data.settings.theme, "night");
    }
    #[test]
    fn emergency_needs_fifty_non_burst_keys_and_is_logged() {
        let mut e = engine(1, 1);
        e.start_rest();
        for _ in 0..49 {
            e.tick(100, None);
            assert_eq!(e.emergency_key().unwrap(), Effect::None);
        }
        assert_eq!(e.emergency_key().unwrap(), Effect::None);
        e.tick(100, None);
        assert_eq!(e.emergency_key().unwrap(), Effect::Unlock);
        assert_eq!(e.data.statistics.emergency_unlocks, 1);
        assert_eq!(e.data.logs[0].kind, "emergency");
    }
    #[test]
    fn idle_resets_emergency_progress_and_disabled_means_disabled() {
        let mut e = engine(1, 1);
        e.start_rest();
        e.emergency_key().unwrap();
        e.tick(2100, None);
        assert_eq!(e.snapshot().emergency_count, 0);
        e.data.settings.emergency_enabled = false;
        assert!(e.emergency_key().is_err());
    }
    #[test]
    fn invalid_stored_settings_recover_and_log_is_bounded() {
        let mut data = Persisted::default();
        data.settings.rest_minutes = 0;
        let mut e = Engine::new(data);
        assert_eq!(e.data.settings.rest_minutes, 2);
        for _ in 0..205 {
            e.start_rest();
        }
        assert_eq!(e.data.logs.len(), 200);
    }
}
