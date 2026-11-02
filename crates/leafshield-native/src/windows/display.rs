use leafshield_core::colour;
use std::sync::{Mutex, OnceLock};

#[derive(Clone)]
pub struct Status {
    pub active: bool,
    pub monitors: usize,
    pub error: Option<String>,
    pub rgb: [f32; 3],
    pub transitioning: bool,
    pub updates: u64,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            active: false,
            monitors: 0,
            error: None,
            rgb: [1.0; 3],
            transitioning: false,
            updates: 0,
        }
    }
}
fn state() -> &'static Mutex<Status> {
    static STATE: OnceLock<Mutex<Status>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(Status::default()))
}
pub fn status() -> Status {
    state().lock().unwrap().clone()
}

#[cfg(windows)]
mod native {
    use super::*;
    use colour::{Gains, NEUTRAL, Transition};
    use std::{
        ptr,
        sync::Condvar,
        thread::JoinHandle,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{LPARAM, RECT},
        Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR},
        UI::Magnification::*,
    };

    pub struct Session {
        original: MAGCOLOREFFECT,
        last: MAGCOLOREFFECT,
        pub updates: u64,
    }
    fn same_matrix(a: &MAGCOLOREFFECT, b: &MAGCOLOREFFECT) -> bool {
        a.transform
            .iter()
            .zip(b.transform.iter())
            .all(|(a, b)| (a - b).abs() < 0.00001)
    }
    fn read() -> Result<MAGCOLOREFFECT, String> {
        let mut matrix = MAGCOLOREFFECT::default();
        if unsafe { MagGetFullscreenColorEffect(&mut matrix) } == 0 {
            Err(std::io::Error::last_os_error().to_string())
        } else {
            Ok(matrix)
        }
    }
    impl Session {
        pub fn new() -> Result<Self, String> {
            if unsafe { MagInitialize() } == 0 {
                return Err("系统颜色调节初始化失败".into());
            }
            let original = match read() {
                Ok(value) => value,
                Err(e) => {
                    unsafe {
                        MagUninitialize();
                    }
                    return Err(e);
                }
            };
            if !same_matrix(
                &original,
                &MAGCOLOREFFECT {
                    transform: colour::matrix(NEUTRAL),
                },
            ) {
                unsafe {
                    MagUninitialize();
                }
                return Err("系统颜色正由其他程序调节，请关闭冲突程序后重新启动一叶".into());
            }
            Ok(Self {
                original,
                last: original,
                updates: 0,
            })
        }
        pub fn apply(&mut self, rgb: Gains) -> Result<bool, String> {
            let next = MAGCOLOREFFECT {
                transform: colour::matrix(rgb),
            };
            if same_matrix(&next, &self.last) {
                return Ok(false);
            }
            self.check_owner()?;
            if unsafe { MagSetFullscreenColorEffect(&next) } == 0 {
                return Err("系统颜色调节失败，已停止调整".into());
            }
            self.last = next;
            self.updates += 1;
            Ok(true)
        }
        fn check_owner(&self) -> Result<(), String> {
            if !same_matrix(&read()?, &self.last) {
                Err("颜色已由其他程序接管，请关闭冲突程序后重新启动一叶".into())
            } else {
                Ok(())
            }
        }
    }
    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                if read().is_ok_and(|current| same_matrix(&current, &self.last)) {
                    MagSetFullscreenColorEffect(&self.original);
                }
                MagUninitialize();
            }
        }
    }
    #[derive(Clone, Copy)]
    struct Target {
        temperature: u32,
        brightness: u32,
        enabled: bool,
        revision: u64,
        stop: bool,
    }
    struct Control {
        target: Mutex<Target>,
        changed: Condvar,
        worker: Mutex<Option<JoinHandle<()>>>,
    }
    fn control() -> &'static Control {
        static CONTROL: OnceLock<Control> = OnceLock::new();
        CONTROL.get_or_init(|| Control {
            target: Mutex::new(Target {
                temperature: 6500,
                brightness: 100,
                enabled: false,
                revision: 0,
                stop: false,
            }),
            changed: Condvar::new(),
            worker: Mutex::new(None),
        })
    }
    pub fn target(temperature: u32, brightness: u32, enabled: bool) {
        let c = control();
        let mut value = c.target.lock().unwrap();
        if (value.temperature, value.brightness, value.enabled)
            != (temperature, brightness, enabled)
        {
            value.temperature = temperature;
            value.brightness = brightness;
            value.enabled = enabled;
            value.revision += 1;
            c.changed.notify_one();
        }
    }
    unsafe extern "system" fn monitor(_: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> i32 {
        unsafe {
            *(data as *mut usize) += 1;
        }
        1
    }
    fn monitors() -> usize {
        let mut count = 0usize;
        unsafe {
            EnumDisplayMonitors(
                ptr::null_mut(),
                ptr::null(),
                Some(monitor),
                &mut count as *mut _ as LPARAM,
            );
        }
        count
    }
    pub fn start(settings: &leafshield_core::Settings) {
        let c = control();
        {
            let mut target = c.target.lock().unwrap();
            target.stop = false;
        }
        target(
            settings.temperature_kelvin,
            settings.brightness_percent,
            settings.display_enabled,
        );
        let worker = std::thread::spawn(|| {
            let result = (|| -> Result<(), String> {
                let mut session = Session::new()?;
                let mut transition = Transition::new(Instant::now());
                let mut revision = u64::MAX;
                loop {
                    let target = *control().target.lock().unwrap();
                    if target.stop {
                        break;
                    }
                    let now = Instant::now();
                    if target.revision != revision {
                        transition.target(
                            colour::gains(target.temperature, target.brightness, target.enabled),
                            now,
                        );
                        revision = target.revision;
                    }
                    let rgb = transition.sample(now);
                    session.apply(rgb)?;
                    {
                        let mut value = state().lock().unwrap();
                        value.active = !colour::same(rgb, NEUTRAL);
                        value.monitors = monitors();
                        value.rgb = rgb;
                        value.transitioning = !transition.settled();
                        value.updates = session.updates;
                    }
                    let wait = if transition.settled() {
                        Duration::from_secs(1)
                    } else {
                        Duration::from_millis(16)
                    };
                    let c = control();
                    let guard = c.target.lock().unwrap();
                    if guard.stop {
                        break;
                    }
                    if guard.revision == revision {
                        let (guard, timeout) = c.changed.wait_timeout(guard, wait).unwrap();
                        drop(guard);
                        if timeout.timed_out() {
                            session.check_owner()?;
                        }
                    }
                }
                Ok(())
            })();
            let mut value = state().lock().unwrap();
            value.error = result.err();
            value.active = false;
            value.transitioning = false;
            value.rgb = NEUTRAL;
        });
        *c.worker.lock().unwrap() = Some(worker);
    }
    pub fn stop() {
        let c = control();
        {
            let mut value = c.target.lock().unwrap();
            value.stop = true;
            c.changed.notify_one();
        }
        let worker = c.worker.lock().unwrap().take();
        if let Some(worker) = worker {
            let _ = worker.join();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        #[ignore = "changes desktop colour briefly; run explicitly with the app closed"]
        fn native_rgb_readback_idle_and_restore() {
            let mut session = Session::new().unwrap();
            let original = read().unwrap();
            let rgb = colour::gains(5000, 80, true);
            assert!(session.apply(rgb).unwrap());
            assert!(same_matrix(
                &read().unwrap(),
                &MAGCOLOREFFECT {
                    transform: colour::matrix(rgb)
                }
            ));
            let count = session.updates;
            for _ in 0..10 {
                assert!(!session.apply(rgb).unwrap());
            }
            assert_eq!(session.updates, count);
            let begin = Instant::now();
            let mut fade = Transition::new(begin);
            fade.target(rgb, begin);
            fade.sample(begin + colour::FADE);
            let next = colour::gains(3500, 80, true);
            fade.target(next, begin + colour::FADE);
            for frame in 0..=12 {
                session
                    .apply(fade.sample(begin + colour::FADE + Duration::from_millis(frame * 16)))
                    .unwrap();
                assert!((read().unwrap().transform[0] - 0.8).abs() < 0.00001);
            }
            drop(session);
            assert_ne!(unsafe { MagInitialize() }, 0);
            assert!(same_matrix(&read().unwrap(), &original));
            unsafe {
                MagUninitialize();
            }
        }
    }
}

pub use native::{start, stop, target};
