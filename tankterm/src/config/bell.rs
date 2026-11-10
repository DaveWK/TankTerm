use std::time::Duration;

use serde::Serialize;

use tankterm_config_derive::ConfigDeserialize;

use crate::config::ui_config::Program;
use crate::display::color::Rgb;

#[derive(ConfigDeserialize, Serialize, Clone, Debug, PartialEq)]
pub struct BellConfig {
    /// Visual bell animation function.
    pub animation: BellAnimation,

    /// Spring used by the `Spring` animation.
    pub spring: Spring,

    /// Command to run on bell.
    pub command: Option<Program>,

    /// Visual bell flash color.
    pub color: Rgb,

    /// Visual bell duration in milliseconds.
    duration: u16,
}

impl Default for BellConfig {
    fn default() -> Self {
        Self {
            color: Rgb::new(255, 255, 255),
            animation: Default::default(),
            spring: Default::default(),
            command: Default::default(),
            duration: Default::default(),
        }
    }
}

impl BellConfig {
    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.duration as u64)
    }

    /// Turn off the visual bell, keeping the command and urgency hints.
    pub fn disable_visual(&mut self) {
        self.duration = 0;
    }
}

/// A damped spring, like the springs of Charm's Harmonica.
#[derive(ConfigDeserialize, Serialize, Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    /// Angular frequency in radians per second; higher is faster.
    pub frequency: f32,

    /// Damping ratio: below 1 the spring oscillates, 1 settles as fast as possible without
    /// oscillating, and above 1 it settles more slowly.
    pub damping: f32,
}

impl Default for Spring {
    fn default() -> Self {
        Self { frequency: 18., damping: 0.35 }
    }
}

impl Spring {
    /// Position of the spring `seconds` after starting at 0, at rest, pulled toward 1.
    ///
    /// This is the closed form of a damped harmonic oscillator, the model behind Harmonica's
    /// springs. Invalid settings settle immediately.
    pub fn position(&self, seconds: f64) -> f64 {
        let (frequency, damping) = (f64::from(self.frequency), f64::from(self.damping));
        if !(frequency > 0. && damping >= 0. && frequency.is_finite() && damping.is_finite()) {
            return 1.;
        }

        let t = seconds.max(0.);
        let offset = if (damping - 1.).abs() < 1e-6 {
            // Critically damped.
            (-frequency * t).exp() * (1. + frequency * t)
        } else if damping < 1. {
            // Underdamped: oscillates around the target while settling.
            let damped = frequency * (1. - damping * damping).sqrt();
            let decay = (-damping * frequency * t).exp();
            decay * ((damped * t).cos() + damping * frequency / damped * (damped * t).sin())
        } else {
            // Overdamped: approaches the target without overshooting.
            let root = (damping * damping - 1.).sqrt();
            let (r1, r2) = (frequency * (-damping + root), frequency * (-damping - root));
            (r2 * (r1 * t).exp() - r1 * (r2 * t).exp()) / (r2 - r1)
        };

        1. - offset
    }
}

/// `VisualBellAnimations` are modeled after a subset of CSS transitions and Robert
/// Penner's Easing Functions.
#[derive(ConfigDeserialize, Serialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BellAnimation {
    // CSS animation.
    Ease,
    // CSS animation.
    EaseOut,
    // Penner animation.
    EaseOutSine,
    // Penner animation.
    EaseOutQuad,
    // Penner animation.
    EaseOutCubic,
    // Penner animation.
    EaseOutQuart,
    // Penner animation.
    EaseOutQuint,
    // Penner animation.
    EaseOutExpo,
    // Penner animation.
    EaseOutCirc,
    // Penner animation.
    #[default]
    Linear,
    /// A damped spring, configured by `bell.spring`; it bounces when underdamped.
    Spring,
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::UiConfig;

    fn spring(frequency: f32, damping: f32) -> Spring {
        Spring { frequency, damping }
    }

    fn samples(spring: Spring) -> impl Iterator<Item = f64> {
        (0..=400).map(move |step| spring.position(f64::from(step) * 0.005))
    }

    #[test]
    fn starts_at_rest_and_settles() {
        for spring in [spring(18., 0.35), spring(18., 1.), spring(18., 2.)] {
            assert!(spring.position(0.).abs() < 1e-9, "{spring:?}");
            assert!((spring.position(5.) - 1.).abs() < 1e-3, "{spring:?}");
        }
    }

    #[test]
    fn underdamped_overshoots() {
        assert!(samples(spring(18., 0.35)).any(|position| position > 1.05));
    }

    #[test]
    fn critical_and_overdamped_dont_overshoot() {
        for spring in [spring(18., 1.), spring(18., 2.5)] {
            let positions: Vec<_> = samples(spring).collect();
            assert!(positions.iter().all(|&p| p <= 1. + 1e-9), "{spring:?}");
            assert!(
                positions.windows(2).all(|w| w[1] >= w[0] - 1e-12),
                "{spring:?} isn't monotonic"
            );
        }
    }

    #[test]
    fn invalid_springs_settle_immediately() {
        for spring in [spring(0., 0.5), spring(-1., 0.5), spring(5., -1.), spring(f32::NAN, 1.)] {
            assert_eq!(spring.position(0.), 1., "{spring:?}");
        }
    }

    #[test]
    fn spring_config() {
        let config: UiConfig = toml::from_str(
            "[bell]\nanimation = 'Spring'\nduration = 300\nspring = { frequency = 10.0, damping = \
             1.0 }\n",
        )
        .unwrap();

        assert_eq!(config.bell.animation, BellAnimation::Spring);
        assert_eq!(config.bell.spring, spring(10., 1.));
        assert_eq!(BellConfig::default().spring, Spring::default());
    }
}
