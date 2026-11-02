use std::time::{Duration, Instant};

pub type Gains = [f32; 3];
pub const NEUTRAL: Gains = [1.0; 3];
pub const FADE: Duration = Duration::from_millis(180);

fn gaussian(wave: f64, centre: f64, left: f64, right: f64) -> f64 {
    let distance = (wave - centre) * if wave < centre { left } else { right };
    (-0.5 * distance * distance).exp()
}

// Integrate Planck's spectrum using Wyman/Sloan/Shirley's analytic CIE 1931
// matching functions (JCGT 2(2), 2013). Convert XYZ to sRGB, normalize at 6500 K.
// This is a visual white-point approximation, not display calibration.
fn white_point(kelvin: f64) -> [f64; 3] {
    let mut xyz = [0.0; 3];
    for wave in (380..=780).step_by(5) {
        let w = wave as f64;
        let power = 1.0 / (w.powi(5) * (14_387_768.77 / (w * kelvin)).exp_m1());
        let matching = [
            1.056 * gaussian(w, 599.8, 0.0264, 0.0323) + 0.362 * gaussian(w, 442.0, 0.0624, 0.0374)
                - 0.065 * gaussian(w, 501.1, 0.0490, 0.0382),
            0.821 * gaussian(w, 568.8, 0.0213, 0.0247) + 0.286 * gaussian(w, 530.9, 0.0613, 0.0322),
            1.217 * gaussian(w, 437.0, 0.0845, 0.0278) + 0.681 * gaussian(w, 459.0, 0.0385, 0.0725),
        ];
        for channel in 0..3 {
            xyz[channel] += power * matching[channel];
        }
    }
    let [x, y, z] = xyz.map(|v| v / xyz[1]);
    let rgb = [
        3.2406 * x - 1.5372 * y - 0.4986 * z,
        -0.9689 * x + 1.8758 * y + 0.0415 * z,
        0.0557 * x - 0.2040 * y + 1.0570 * z,
    ];
    let maximum = rgb.into_iter().fold(0.0_f64, f64::max);
    rgb.map(|v| {
        let linear = (v / maximum).max(0.0);
        if linear <= 0.0031308 {
            12.92 * linear
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        }
    })
}

pub fn gains(kelvin: u32, brightness: u32, enabled: bool) -> Gains {
    if !enabled {
        return NEUTRAL;
    }
    let kelvin = kelvin.clamp(1500, 6500);
    let brightness = brightness.clamp(20, 100) as f64 / 100.0;
    if kelvin == 6500 {
        return [brightness as f32; 3];
    }
    let point = white_point(kelvin as f64);
    let reference = white_point(6500.0);
    std::array::from_fn(|channel| {
        ((point[channel] / reference[channel]).clamp(0.0, 1.0) * brightness) as f32
    })
}

pub fn matrix(rgb: Gains) -> [f32; 25] {
    let mut value = [0.0; 25];
    for (channel, gain) in rgb.into_iter().enumerate() {
        value[channel * 6] = gain;
    }
    value[18] = 1.0;
    value[24] = 1.0;
    value
}
pub fn same(left: Gains, right: Gains) -> bool {
    left.into_iter()
        .zip(right)
        .all(|(a, b)| (a - b).abs() < 0.00001)
}

pub struct Transition {
    current: Gains,
    from: Gains,
    target: Gains,
    began: Instant,
}
impl Transition {
    pub fn new(now: Instant) -> Self {
        Self {
            current: NEUTRAL,
            from: NEUTRAL,
            target: NEUTRAL,
            began: now,
        }
    }
    pub fn target(&mut self, target: Gains, now: Instant) {
        if same(target, self.target) {
            return;
        }
        self.from = self.sample(now);
        self.target = target;
        self.began = now;
    }
    pub fn sample(&mut self, now: Instant) -> Gains {
        let progress =
            (now.saturating_duration_since(self.began).as_secs_f32() / FADE.as_secs_f32()).min(1.0);
        let smooth = progress * progress * (3.0 - 2.0 * progress);
        self.current =
            std::array::from_fn(|i| self.from[i] + (self.target[i] - self.from[i]) * smooth);
        if progress >= 1.0 {
            self.current = self.target;
        }
        self.current
    }
    pub fn settled(&self) -> bool {
        same(self.current, self.target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn neutral_disabled_and_brightness_preserve_black() {
        assert_eq!(gains(6500, 100, true), NEUTRAL);
        assert_eq!(gains(1500, 20, false), NEUTRAL);
        assert_eq!(gains(6500, 80, true), [0.8; 3]);
        let warm = gains(5000, 80, true);
        assert_eq!(warm[0], 0.8);
        assert!(warm[1] < warm[0] && warm[2] < warm[1]);
        let value = matrix(warm);
        assert_eq!(&value[20..23], &[0.0; 3]);
        for row in 0..3 {
            for col in 0..3 {
                if row != col {
                    assert_eq!(value[row * 5 + col], 0.0);
                }
            }
        }
    }
    #[test]
    fn temperature_is_monotonic_and_all_slider_values_remain_bounded() {
        let mut previous = [0.0; 3];
        for temperature in (1500..=6500).step_by(100) {
            let rgb = gains(temperature, 100, true);
            assert!(
                rgb.into_iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
            );
            assert!(rgb[1] >= previous[1] - 0.0001 && rgb[2] >= previous[2] - 0.0001);
            previous = rgb;
        }
        for temperature in [0, u32::MAX] {
            assert!(
                gains(temperature, 0, true)
                    .into_iter()
                    .all(|v| v.is_finite() && (0.0..=0.2).contains(&v))
            );
        }
    }
    #[test]
    fn retargeting_never_passes_through_neutral_and_idle_does_not_drift() {
        let began = Instant::now();
        let first = gains(5000, 80, true);
        let second = gains(3500, 80, true);
        let mut transition = Transition::new(began);
        transition.target(first, began);
        assert_eq!(transition.sample(began + FADE), first);
        transition.target(second, began + FADE);
        for frame in 1..=12 {
            let current = transition.sample(began + FADE + Duration::from_millis(frame * 16));
            assert!((current[0] - 0.8).abs() < 0.00001);
            assert!(current[1] <= first[1] && current[1] >= second[1]);
            assert!(current[2] <= first[2] && current[2] >= second[2]);
        }
        assert!(transition.settled());
        assert_eq!(transition.sample(began + Duration::from_secs(60)), second);
    }
}
